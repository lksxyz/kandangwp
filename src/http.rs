use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;

use serde_json::Value;

const MAX_BODY: usize = 1 << 20;

pub const MIME_JSON: &str = "application/json";
pub const MIME_HTML: &str = "text/html; charset=utf-8";
pub const MIME_CSS: &str = "text/css; charset=utf-8";
pub const MIME_SVG: &str = "image/svg+xml";
pub const MIME_WOFF2: &str = "font/woff2";
pub const MIME_TEXT: &str = "text/plain; charset=utf-8";

pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }
}

pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl Response {
    pub fn json(status: u16, value: &Value) -> Self {
        Self {
            status,
            content_type: MIME_JSON,
            body: value.to_string().into_bytes(),
        }
    }

    pub fn html(body: String) -> Self {
        Self {
            status: 200,
            content_type: MIME_HTML,
            body: body.into_bytes(),
        }
    }

    pub fn css(body: &'static str) -> Self {
        Self {
            status: 200,
            content_type: MIME_CSS,
            body: body.as_bytes().to_vec(),
        }
    }

    pub fn js(body: &'static str) -> Self {
        Self {
            status: 200,
            content_type: "text/javascript; charset=utf-8",
            body: body.as_bytes().to_vec(),
        }
    }

    pub fn svg(body: String) -> Self {
        Self {
            status: 200,
            content_type: MIME_SVG,
            body: body.into_bytes(),
        }
    }

    pub fn font(body: &'static [u8]) -> Self {
        Self {
            status: 200,
            content_type: MIME_WOFF2,
            body: body.to_vec(),
        }
    }

    pub fn text(status: u16, body: String) -> Self {
        Self {
            status,
            content_type: MIME_TEXT,
            body: body.into_bytes(),
        }
    }
}

pub fn read_request(stream: &TcpStream) -> std::io::Result<Option<Request>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or("/").to_string();

    let mut headers = HashMap::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 {
            break;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let length: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if length > MAX_BODY {
        return Ok(None);
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;

    Ok(Some(Request {
        method,
        path,
        headers,
        body,
    }))
}

pub fn write_response(mut stream: &TcpStream, response: Response) -> std::io::Result<()> {
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Internal Server Error",
    };
    write!(
        stream,
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len()
    )?;
    stream.write_all(&response.body)
}
