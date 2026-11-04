use std::fs;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::config;
use crate::export;
use crate::{provision, sites};

const MAX_HEAD: usize = 16 * 1024;

pub enum Route {
    Site(String),
    Preview(String),
}

pub fn route_for_host(host: &str) -> Option<Route> {
    let hostname = host.split(':').next()?;
    if let Some(name) = hostname.strip_suffix(".preview.localhost") {
        return valid_label(name).then(|| Route::Preview(name.to_string()));
    }
    let name = hostname.strip_suffix(".localhost")?;
    valid_label(name).then(|| Route::Site(name.to_string()))
}

fn valid_label(name: &str) -> bool {
    !name.is_empty() && !name.contains('.')
}

pub fn serve(listener: TcpListener) {
    for stream in listener.incoming().flatten() {
        thread::spawn(move || {
            let _ = handle(stream);
        });
    }
}

fn handle(mut client: TcpStream) -> io::Result<()> {
    client.set_read_timeout(Some(Duration::from_secs(10)))?;
    let head = read_head(&mut client)?;
    let host = header_value(&head, "host");

    let route = host.as_deref().and_then(route_for_host);
    let name = match route {
        Some(Route::Preview(name)) => {
            let path = request_path(&head);
            return serve_preview(&mut client, &name, &path);
        }
        Some(Route::Site(name)) => name,
        None => return respond(&mut client, 404, "Unknown site."),
    };

    let Ok(port) = provision::port_of(&name) else {
        return respond(&mut client, 404, "Unknown site.");
    };
    if !sites::is_listening(port) {
        return respond(
            &mut client,
            503,
            "This site is stopped. Start it from the Kandang dashboard.",
        );
    }

    let mut upstream = TcpStream::connect(("127.0.0.1", port))?;
    upstream.write_all(&head)?;

    let mut client_reader = client.try_clone()?;
    let mut upstream_writer = upstream.try_clone()?;
    let forward = thread::spawn(move || {
        let _ = io::copy(&mut client_reader, &mut upstream_writer);
        let _ = upstream_writer.shutdown(Shutdown::Write);
    });
    let _ = io::copy(&mut upstream, &mut client);
    let _ = client.shutdown(Shutdown::Write);
    let _ = forward.join();
    Ok(())
}

fn request_path(head: &[u8]) -> String {
    let text = String::from_utf8_lossy(head);
    text.split_whitespace().nth(1).unwrap_or("/").to_string()
}

fn serve_preview(client: &mut TcpStream, name: &str, raw_path: &str) -> io::Result<()> {
    let out = match provision::site_dir(name) {
        Ok(dir) => dir.join("public"),
        Err(_) => return respond(client, 404, "Unknown site."),
    };
    if !out.join("index.html").is_file() {
        return respond(
            client,
            404,
            "Nothing exported yet. Run Publish or kandang export first.",
        );
    }

    let path = raw_path.split(['?', '#']).next().unwrap_or("/");
    let relative = path.trim_start_matches('/');
    if relative.split('/').any(|part| part == "..") {
        return respond(client, 404, "Not found.");
    }
    if !relative.is_empty() && !path.ends_with('/') && out.join(relative).is_dir() {
        return redirect(client, &format!("{path}/"));
    }
    let mut file = out.join(relative);
    if file.is_dir() || relative.is_empty() || relative.ends_with('/') {
        file = file.join("index.html");
    }
    if !file.is_file() {
        if !path.ends_with('/') && out.join(relative).join("index.html").is_file() {
            return redirect(client, &format!("{path}/"));
        }
        return respond(client, 404, "Not found.");
    }

    let body = fs::read(&file)?;
    let mime = mime_for(&file);
    let body = match (
        mime.starts_with("text/")
            || mime.contains("javascript")
            || mime.contains("json")
            || mime.contains("xml"),
        export::stored_public_base(&out),
    ) {
        (true, Some(public)) => {
            let preview = format!("http://{name}.preview.localhost:{}", config::GATEWAY_PORT);
            String::from_utf8_lossy(&body)
                .replace(public.trim_end_matches('/'), &preview)
                .into_bytes()
        }
        _ => body,
    };
    write!(
        client,
        "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",
        body.len()
    )?;
    client.write_all(&body)
}

fn redirect(client: &mut TcpStream, location: &str) -> io::Result<()> {
    write!(
        client,
        "HTTP/1.1 301 Moved Permanently\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
}

fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "json" => "application/json",
        "xml" => "application/xml; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "mp4" => "video/mp4",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

fn read_head(client: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let read = client.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|w| w == b"\r\n\r\n") || buffer.len() > MAX_HEAD {
            break;
        }
    }
    Ok(buffer)
}

fn header_value(head: &[u8], name: &str) -> Option<String> {
    let text = String::from_utf8_lossy(head);
    text.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim()
            .eq_ignore_ascii_case(name)
            .then(|| value.trim().to_string())
    })
}

fn respond(client: &mut TcpStream, status: u16, body: &str) -> io::Result<()> {
    let reason = if status == 404 {
        "Not Found"
    } else {
        "Service Unavailable"
    };
    write!(
        client,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_hosts_to_routes() {
        assert!(
            matches!(route_for_host("blog.localhost:18080"), Some(Route::Site(n)) if n == "blog")
        );
        assert!(
            matches!(route_for_host("blog.preview.localhost:18080"), Some(Route::Preview(n)) if n == "blog")
        );
        assert!(route_for_host("localhost:18080").is_none());
        assert!(route_for_host("a.b.localhost:18080").is_none());
        assert!(route_for_host("evil.com").is_none());
        assert!(route_for_host(".localhost").is_none());
        assert!(route_for_host(".preview.localhost").is_none());
    }
}
