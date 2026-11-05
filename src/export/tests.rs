use super::*;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread;

fn serve(routes: Vec<(&'static str, &'static str, &'static [u8])>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = match stream {
                Ok(s) => s,
                Err(_) => continue,
            };
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
            let response = match routes.iter().find(|(p, _, _)| *p == path) {
                Some((_, mime, body)) => {
                    let mut out = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                    out.extend_from_slice(body);
                    out
                }
                None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_vec(),
            };
            let _ = stream.write_all(&response);
        }
    });
    format!("http://{addr}")
}

fn temp_out(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kandang-test-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

#[test]
fn exports_pages_assets_and_sitemap_pages_and_rewrites_origin() {
    let home = br#"<html><a href="/about/">About</a><link href="/style.css" rel="stylesheet"><img src="/img/a.png"><a href="/wp-admin/">admin</a><a href="/?s=x">search</a></html>"#;
    let about = br#"<html><a href="/">Home</a></html>"#;
    let hidden = b"<html>only in sitemap</html>";
    let sitemap = b"<urlset><url><loc>/hidden/</loc></url></urlset>";
    let png: &[u8] = &[137, 80, 78, 71];
    let style = b"body { color: red; }";
    let wp_admin = b"<html>admin</html>";

    let origin = serve(vec![
        ("/", "text/html; charset=utf-8", home),
        ("/about/", "text/html; charset=utf-8", about),
        ("/hidden/", "text/html; charset=utf-8", hidden),
        ("/wp-sitemap.xml", "application/xml", sitemap),
        ("/style.css", "text/css", style),
        ("/img/a.png", "image/png", png),
        ("/wp-admin/", "text/html; charset=utf-8", wp_admin),
        ("/?s=x", "text/html; charset=utf-8", b"<html>results</html>"),
    ]);

    let out = temp_out("basic");
    let report = export(&Options {
        source: Url::parse(&format!("{origin}/")).unwrap(),
        public_base: "https://example.com".into(),
        out: out.clone(),
        connect: None,
    })
    .unwrap();

    assert!(out.join("index.html").exists());
    assert!(out.join("about/index.html").exists());
    assert!(out.join("hidden/index.html").exists());
    assert_eq!(fs::read(out.join("img/a.png")).unwrap(), png);
    assert!(out.join("style.css").exists());
    assert!(!out.join("wp-admin").exists());
    assert!(report.broken.is_empty(), "{:?}", report.broken);

    let index = fs::read_to_string(out.join("index.html")).unwrap();
    assert!(index.contains(r#"href="/about/""#));
    assert!(!index.contains(&origin));
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn strips_server_only_discovery_links_and_skips_php() {
    let home = br#"<html><head><link rel="alternate" type="application/rss+xml" href="/?feed=rss2"><link rel="https://api.w.org/" href="/index.php?rest_route=/"><link rel="EditURI" href="/xmlrpc.php?rsd"><link rel="stylesheet" href="/s.css"></head><a href="/index.php?rest_route=/">api</a><a href="/about/">about</a></html>"#;
    let origin = serve(vec![
        ("/", "text/html; charset=utf-8", home),
        ("/index.php?rest_route=/", "application/json", br#"{"x":1}"#),
        ("/about/", "text/html; charset=utf-8", b"<html>about</html>"),
        ("/s.css", "text/css", b"p{}"),
    ]);

    let out = temp_out("discovery");
    export(&Options {
        source: Url::parse(&format!("{origin}/")).unwrap(),
        public_base: "https://example.com".into(),
        out: out.clone(),
        connect: None,
    })
    .unwrap();

    let index = fs::read_to_string(out.join("index.html")).unwrap();
    assert!(!index.contains("api.w.org"));
    assert!(!index.contains("EditURI"));
    assert!(!index.contains("application/rss+xml"));
    assert!(index.contains(r#"href="/s.css""#));
    assert!(out.join("about/index.html").exists());
    assert!(!out.join("index.php").exists());
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn fetches_through_connect_port_and_matches_source_host() {
    let home = br#"<html><a href="/about/">About</a></html>"#;
    let origin = serve(vec![
        ("/", "text/html; charset=utf-8", home),
        ("/about/", "text/html; charset=utf-8", b"<html>about</html>"),
    ]);
    let port: u16 = origin.rsplit(':').next().unwrap().parse().unwrap();

    let out = temp_out("connect");
    let report = export(&Options {
        source: Url::parse(&format!("http://site.localhost:{port}/")).unwrap(),
        public_base: "https://example.com".into(),
        out: out.clone(),
        connect: Some(port),
    })
    .unwrap();

    assert!(report.broken.is_empty(), "{:?}", report.broken);
    assert!(out.join("about/index.html").exists());
    let index = fs::read_to_string(out.join("index.html")).unwrap();
    assert!(index.contains(r#"href="/about/""#));
    let _ = fs::remove_dir_all(&out);
}

type Routes = std::sync::Arc<std::sync::Mutex<Vec<(String, String, Vec<u8>)>>>;

fn serve_shared() -> (String, Routes) {
    let routes: Routes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let shared = routes.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
            let found = shared
                .lock()
                .unwrap()
                .iter()
                .find(|(p, _, _)| *p == path)
                .cloned();
            let response = match found {
                Some((_, mime, body)) => {
                    let mut out = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                    out.extend_from_slice(&body);
                    out
                }
                None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_vec(),
            };
            let _ = stream.write_all(&response);
        }
    });
    (format!("http://{addr}"), routes)
}

#[test]
fn reuses_pages_whose_lastmod_is_unchanged() {
    let (origin, routes) = serve_shared();
    let set = |body: &str, lastmod: &str| {
        *routes.lock().unwrap() = vec![
            (
                "/".into(),
                "text/html".into(),
                b"<html><a href=\"/about/\">a</a></html>".to_vec(),
            ),
            (
                "/wp-sitemap.xml".into(),
                "application/xml".into(),
                format!(
                    "<urlset><url><loc>/about/</loc><lastmod>{lastmod}</lastmod></url></urlset>"
                )
                .into_bytes(),
            ),
            (
                "/about/".into(),
                "text/html".into(),
                body.as_bytes().to_vec(),
            ),
        ];
    };
    let out = temp_out("incremental");
    let run = || {
        export(&Options {
            source: Url::parse(&format!("{origin}/")).unwrap(),
            public_base: "https://example.com".into(),
            out: out.clone(),
            connect: None,
        })
        .unwrap()
    };

    set("<html>v1</html>", "2026-01-01");
    let first = run();
    assert_eq!(first.reused, 0);

    set("<html>v2</html>", "2026-01-01");
    let second = run();
    assert_eq!(second.reused, 1);
    assert!(
        fs::read_to_string(out.join("about/index.html"))
            .unwrap()
            .contains("v1")
    );

    set("<html>v3</html>", "2026-02-02");
    let third = run();
    assert_eq!(third.reused, 0);
    assert!(
        fs::read_to_string(out.join("about/index.html"))
            .unwrap()
            .contains("v3")
    );
    let _ = fs::remove_dir_all(&out);
    let _ = fs::remove_file(state_path(&out));
}

#[test]
fn a_new_post_refreshes_listing_pages_with_unchanged_lastmod() {
    let (origin, routes) = serve_shared();
    let set = |listing: &str, with_post: bool| {
        let mut sitemap =
            String::from("<urlset><url><loc>/blog/</loc><lastmod>2026-01-01</lastmod></url>");
        if with_post {
            sitemap.push_str("<url><loc>/new-post/</loc><lastmod>2026-03-03</lastmod></url>");
        }
        sitemap.push_str("</urlset>");
        *routes.lock().unwrap() = vec![
            (
                "/".into(),
                "text/html".into(),
                b"<html><a href=\"/blog/\">blog</a></html>".to_vec(),
            ),
            (
                "/wp-sitemap.xml".into(),
                "application/xml".into(),
                sitemap.into_bytes(),
            ),
            (
                "/blog/".into(),
                "text/html".into(),
                listing.as_bytes().to_vec(),
            ),
            (
                "/new-post/".into(),
                "text/html".into(),
                b"<html>new post</html>".to_vec(),
            ),
        ];
    };
    let out = temp_out("new-post");
    let run = || {
        export(&Options {
            source: Url::parse(&format!("{origin}/")).unwrap(),
            public_base: "https://example.com".into(),
            out: out.clone(),
            connect: None,
        })
        .unwrap()
    };

    set("<html>no posts yet</html>", false);
    run();

    set("<html><a href=\"/new-post/\">new post</a></html>", true);
    let second = run();
    let listing = fs::read_to_string(out.join("blog/index.html")).unwrap();
    assert!(listing.contains("new-post"), "stale listing: {listing}");
    assert_eq!(second.reused, 0);
    let _ = fs::remove_dir_all(&out);
    let _ = fs::remove_file(state_path(&out));
}

#[test]
fn refetches_pages_when_the_theme_changes_without_a_new_lastmod() {
    let (origin, routes) = serve_shared();
    let set = |home: &str| {
        *routes.lock().unwrap() = vec![
                ("/".into(), "text/html".into(), home.as_bytes().to_vec()),
                (
                    "/wp-sitemap.xml".into(),
                    "application/xml".into(),
                    b"<urlset><url><loc>/</loc><lastmod>2026-01-01</lastmod></url><url><loc>/about/</loc><lastmod>2026-01-01</lastmod></url></urlset>".to_vec(),
                ),
                (
                    "/about/".into(),
                    "text/html".into(),
                    b"<html><body>about</body></html>".to_vec(),
                ),
            ];
    };
    let out = temp_out("theme-change");
    let run = || {
        export(&Options {
            source: Url::parse(&format!("{origin}/")).unwrap(),
            public_base: "https://example.com".into(),
            out: out.clone(),
            connect: None,
        })
        .unwrap()
    };

    set("<html><body>OLD THEME</body></html>");
    let first = run();
    assert_eq!(first.reused, 0);

    // Switching themes changes the rendered HTML but not the post's
    // modified date, so the sitemap lastmod stays the same.
    set("<html><body>NEW THEME</body></html>");
    let second = run();
    let index = fs::read_to_string(out.join("index.html")).unwrap();
    assert!(index.contains("NEW THEME"), "stale export: {index}");
    assert_eq!(second.reused, 0);
    let _ = fs::remove_dir_all(&out);
    let _ = fs::remove_file(state_path(&out));
}

#[test]
fn follows_urls_and_imports_inside_css() {
    let home = br#"<html><link href="/css/main.css" rel="stylesheet"></html>"#;
    let origin = serve(vec![
            ("/", "text/html; charset=utf-8", home),
            (
                "/css/main.css",
                "text/css",
                br#"@import "extra.css"; body { background: url("../img/bg.png"); } @font-face { src: url(../fonts/f.woff2) format("woff2"); }"#,
            ),
            ("/css/extra.css", "text/css", b"em{}"),
            ("/img/bg.png", "image/png", b"png"),
            ("/fonts/f.woff2", "font/woff2", b"woff"),
        ]);

    let out = temp_out("css");
    let report = export(&Options {
        source: Url::parse(&format!("{origin}/")).unwrap(),
        public_base: "https://example.com".into(),
        out: out.clone(),
        connect: None,
    })
    .unwrap();

    assert!(report.broken.is_empty(), "{:?}", report.broken);
    assert!(out.join("css/extra.css").exists());
    assert!(out.join("img/bg.png").exists());
    assert!(out.join("fonts/f.woff2").exists());
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn exports_versioned_assets_and_skips_query_pages() {
    let home = br#"<html><link href="/style.css?ver=6.5" rel="stylesheet"><script src="/app.js?ver=1"></script><a href="/?s=x">search</a><a href="/?paged=2">page 2</a></html>"#;
    let origin = serve(vec![
        ("/", "text/html; charset=utf-8", home),
        ("/style.css?ver=6.5", "text/css", b"p{}"),
        ("/app.js?ver=1", "application/javascript", b"x"),
        ("/?s=x", "text/html; charset=utf-8", b"<html>results</html>"),
        (
            "/?paged=2",
            "text/html; charset=utf-8",
            b"<html>page 2</html>",
        ),
    ]);

    let out = temp_out("versioned");
    export(&Options {
        source: Url::parse(&format!("{origin}/")).unwrap(),
        public_base: "https://example.com".into(),
        out: out.clone(),
        connect: None,
    })
    .unwrap();

    assert_eq!(fs::read_to_string(out.join("style.css")).unwrap(), "p{}");
    assert_eq!(fs::read_to_string(out.join("app.js")).unwrap(), "x");
    let entries: Vec<_> = fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert!(
        !entries
            .iter()
            .any(|n| n.contains("search") || n.contains("paged"))
    );
    assert!(!out.join("index.html.tmp").exists());
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn keeps_previous_output_when_export_fails() {
    let out = temp_out("keep");
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("keep.html"), "old").unwrap();

    let result = export(&Options {
        source: Url::parse("http://127.0.0.1:1/").unwrap(),
        public_base: "https://example.com".into(),
        out: out.clone(),
        connect: None,
    });

    assert!(result.is_err());
    assert!(out.join("keep.html").exists());
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn output_paths_map_urls_to_index_files() {
    let base = Url::parse("http://x.test/").unwrap();
    assert_eq!(
        output_path(&base.join("/").unwrap()),
        PathBuf::from("index.html")
    );
    assert_eq!(
        output_path(&base.join("/about").unwrap()),
        PathBuf::from("about/index.html")
    );
    assert_eq!(
        output_path(&base.join("/about/").unwrap()),
        PathBuf::from("about/index.html")
    );
    assert_eq!(
        output_path(&base.join("/style.css").unwrap()),
        PathBuf::from("style.css")
    );
}
