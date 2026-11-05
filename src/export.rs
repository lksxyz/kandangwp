use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use url::Url;

const SKIP_PREFIXES: &[&str] = &[
    "/author/",
    "/wp-admin",
    "/wp-login.php",
    "/wp-json",
    "/xmlrpc.php",
    "/wp-cron.php",
];
const MAX_PAGES: usize = 10_000;

pub struct Options {
    pub source: Url,
    pub public_base: String,
    pub out: PathBuf,
    /// Fetch from this local port instead of the source URL's host, keeping the source host as the Host header.
    pub connect: Option<u16>,
}

#[derive(Debug, Default)]
pub struct Report {
    pub files: usize,
    pub reused: usize,
    pub broken: Vec<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct ExportState {
    source: String,
    public_base: String,
    /// Hash of the homepage HTML from the last export. A theme or plugin
    /// change rewrites every page without touching sitemap lastmods, so a
    /// different homepage means no page may be reused this run.
    #[serde(default)]
    home_fingerprint: Option<String>,
    /// Digest of every sitemap URL and lastmod. A new or removed post changes
    /// listing pages (blog index, feeds) whose own lastmod does not move, so
    /// any change here means no page may be reused this run.
    #[serde(default)]
    sitemap_fingerprint: Option<String>,
    pages: BTreeMap<String, PageState>,
}

#[derive(Serialize, Deserialize, Clone)]
struct PageState {
    lastmod: String,
    links: Vec<String>,
}

pub fn state_path(out: &Path) -> PathBuf {
    PathBuf::from(format!("{}.state.json", out.display()))
}

pub fn export(opts: &Options) -> Result<Report> {
    let previous = load_state(opts);
    let staging = PathBuf::from(format!("{}.tmp", opts.out.display()));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;

    match crawl(opts, &staging, &previous) {
        Ok((report, state)) => {
            if opts.out.exists() {
                fs::remove_dir_all(&opts.out)?;
            }
            fs::rename(&staging, &opts.out).context("replacing the output folder")?;
            fs::write(state_path(&opts.out), serde_json::to_string_pretty(&state)?)?;
            Ok(report)
        }
        Err(e) => {
            let _ = fs::remove_dir_all(&staging);
            Err(e)
        }
    }
}

fn load_state(opts: &Options) -> ExportState {
    let text = match fs::read_to_string(state_path(&opts.out)) {
        Ok(text) => text,
        Err(_) => return ExportState::default(),
    };
    match serde_json::from_str::<ExportState>(&text) {
        Ok(state)
            if state.source == opts.source.as_str() && state.public_base == opts.public_base =>
        {
            state
        }
        _ => ExportState::default(),
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into()
}

fn crawl(opts: &Options, staging: &Path, previous: &ExportState) -> Result<(Report, ExportState)> {
    let agent = agent();
    let source_origin = opts.source.origin().ascii_serialization();
    let lastmods = discover_lastmods(&agent, opts, &source_origin);
    let sitemap_unchanged =
        previous.sitemap_fingerprint.as_deref() == Some(sitemap_fingerprint(&lastmods).as_str());

    let mut queue: VecDeque<Url> = VecDeque::new();
    let mut seen_urls: HashSet<String> = HashSet::new();
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();
    let mut report = Report::default();
    let home_key = opts.source.join("/")?.to_string();
    let mut reuse_allowed = false;
    let mut state = ExportState {
        source: opts.source.as_str().to_string(),
        public_base: opts.public_base.clone(),
        pages: BTreeMap::new(),
        sitemap_fingerprint: Some(sitemap_fingerprint(&lastmods)),
        ..ExportState::default()
    };

    let enqueue = |url: Url, queue: &mut VecDeque<Url>, seen: &mut HashSet<String>| {
        if url.origin().ascii_serialization() == source_origin
            && !should_skip(&url)
            && seen.insert(url.to_string())
        {
            queue.push_back(url);
        }
    };

    enqueue(opts.source.join("/")?, &mut queue, &mut seen_urls);
    enqueue(
        opts.source.join("/wp-sitemap.xml")?,
        &mut queue,
        &mut seen_urls,
    );
    enqueue(
        opts.source.join("/sitemap.xml")?,
        &mut queue,
        &mut seen_urls,
    );

    while let Some(url) = queue.pop_front() {
        if report.files >= MAX_PAGES {
            bail!("stopped after {MAX_PAGES} files; the site looks too large or endless");
        }

        let key = url.to_string();
        let relative = output_path(&url);
        let reusable = if reuse_allowed && key != home_key {
            lastmods
                .get(&key)
                .and_then(|lastmod| previous.pages.get(&key).filter(|p| &p.lastmod == lastmod))
        } else {
            None
        };
        if let Some(page) = reusable {
            let existing = opts.out.join(&relative);
            if existing.is_file() && seen_paths.insert(relative.clone()) {
                let target = staging.join(&relative);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&existing, &target)?;
                for link in &page.links {
                    if let Ok(link) = Url::parse(link) {
                        enqueue(link, &mut queue, &mut seen_urls);
                    }
                }
                state.pages.insert(key, page.clone());
                report.files += 1;
                report.reused += 1;
                continue;
            }
        }

        let optional = is_sitemap_probe(&url);
        let (fetch_url, host_header) = fetch_target(&url, opts.connect);
        let mut request = agent.get(fetch_url.as_str());
        if let Some(host) = &host_header {
            request = request.header("Host", host);
        }
        let mut response = match request.call() {
            Ok(response) => response,
            Err(e) => {
                if !optional {
                    report.broken.push(format!("{url} ({e})"));
                }
                continue;
            }
        };
        let status = response.status().as_u16();
        if status != 200 {
            if !optional {
                report.broken.push(format!("{url} (HTTP {status})"));
            }
            continue;
        }
        let mime = response
            .body()
            .mime_type()
            .unwrap_or("application/octet-stream")
            .to_string();
        let body = response.body_mut().read_to_vec()?;

        if key == home_key && mime.contains("html") {
            let fingerprint = digest(&body);
            reuse_allowed = sitemap_unchanged
                && previous.home_fingerprint.as_deref() == Some(fingerprint.as_str());
            state.home_fingerprint = Some(fingerprint);
        }

        if url.query().is_some() && mime.contains("html") {
            continue;
        }

        if !seen_paths.insert(relative.clone()) {
            continue;
        }

        let bytes = if is_text(&mime) {
            let text = String::from_utf8_lossy(&body).into_owned();
            let mut found: Vec<Url> = Vec::new();
            if mime.contains("html") {
                found.extend(extract_links(&text, &url));
                found.extend(extract_css_urls(&text, &url));
            } else if mime.contains("css") {
                found.extend(extract_css_urls(&text, &url));
            } else if mime.contains("xml") {
                found.extend(extract_sitemap_locs(&text, &url));
            }
            if let Some(lastmod) = lastmods.get(&key).filter(|_| mime.contains("html")) {
                state.pages.insert(
                    key,
                    PageState {
                        lastmod: lastmod.clone(),
                        links: found.iter().map(|u| u.to_string()).collect(),
                    },
                );
            }
            for link in found {
                enqueue(link, &mut queue, &mut seen_urls);
            }
            let text = if mime.contains("html") {
                strip_discovery_links(&text)
            } else {
                text
            };
            text.replace(&source_origin, opts.public_base.trim_end_matches('/'))
                .into_bytes()
        } else {
            body
        };

        let target = staging.join(&relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&target, bytes).with_context(|| format!("writing {}", target.display()))?;
        report.files += 1;
    }

    if report.files == 0 {
        bail!("no pages exported; is the site running at {}?", opts.source);
    }
    Ok((report, state))
}

fn discover_lastmods(agent: &ureq::Agent, opts: &Options, origin: &str) -> HashMap<String, String> {
    let mut lastmods = HashMap::new();
    let mut queue: VecDeque<Url> = VecDeque::new();
    let mut seen: HashSet<String> = HashSet::new();
    for path in ["/wp-sitemap.xml", "/sitemap.xml"] {
        if let Ok(url) = opts.source.join(path) {
            queue.push_back(url);
        }
    }

    while let Some(url) = queue.pop_front() {
        if !seen.insert(url.to_string()) {
            continue;
        }
        let Some(text) = fetch_text(agent, &url, opts.connect) else {
            continue;
        };
        for (loc, lastmod) in extract_sitemap_entries(&text, &url) {
            if loc.origin().ascii_serialization() != origin {
                continue;
            }
            if loc.path().ends_with(".xml") {
                queue.push_back(loc);
            } else if let Some(lastmod) = lastmod {
                lastmods.insert(loc.to_string(), lastmod);
            }
        }
    }
    lastmods
}

fn fetch_text(agent: &ureq::Agent, url: &Url, connect: Option<u16>) -> Option<String> {
    let (fetch_url, host_header) = fetch_target(url, connect);
    let mut request = agent.get(fetch_url.as_str());
    if let Some(host) = &host_header {
        request = request.header("Host", host);
    }
    let mut response = request.call().ok()?;
    if response.status().as_u16() != 200 {
        return None;
    }
    let body = response.body_mut().read_to_vec().ok()?;
    Some(String::from_utf8_lossy(&body).into_owned())
}

fn extract_sitemap_entries(xml: &str, base: &Url) -> Vec<(Url, Option<String>)> {
    let mut entries = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<loc>") {
        let after = &rest[start + 5..];
        let Some(end) = after.find("</loc>") else {
            break;
        };
        let loc = after[..end].trim();
        let tail = &after[end + 6..];
        let block = &tail[..tail.find("<loc>").unwrap_or(tail.len())];
        let lastmod = block.find("<lastmod>").and_then(|i| {
            let value = &block[i + 9..];
            value
                .find("</lastmod>")
                .map(|j| value[..j].trim().to_string())
        });
        if let Ok(url) = base.join(loc) {
            entries.push((url, lastmod));
        }
        rest = tail;
    }
    entries
}

fn fetch_target(url: &Url, connect: Option<u16>) -> (Url, Option<String>) {
    let Some(port) = connect else {
        return (url.clone(), None);
    };
    let host = match url.port() {
        Some(p) => format!("{}:{p}", url.host_str().unwrap_or_default()),
        None => url.host_str().unwrap_or_default().to_string(),
    };
    let mut target = url.clone();
    let _ = target.set_host(Some("127.0.0.1"));
    let _ = target.set_port(Some(port));
    (target, Some(host))
}

fn is_sitemap_probe(url: &Url) -> bool {
    url.path() == "/wp-sitemap.xml" || url.path() == "/sitemap.xml"
}

fn should_skip(url: &Url) -> bool {
    let path = url.path();
    if path.ends_with(".php")
        || path.ends_with("/feed/")
        || path.contains("/feed/")
        || path.contains("wp-sitemap-users")
    {
        return true;
    }
    SKIP_PREFIXES
        .iter()
        .any(|prefix| url.path().starts_with(prefix))
}

fn sitemap_fingerprint(lastmods: &HashMap<String, String>) -> String {
    let sorted: BTreeMap<&String, &String> = lastmods.iter().collect();
    let text: String = sorted
        .iter()
        .map(|(url, lastmod)| format!("{url}\n{lastmod}\n"))
        .collect();
    digest(text.as_bytes())
}

fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn is_text(mime: &str) -> bool {
    mime.starts_with("text/")
        || mime.contains("html")
        || mime.contains("xml")
        || mime.contains("javascript")
        || mime.contains("json")
}

fn extract_links(html: &str, base: &Url) -> Vec<Url> {
    let document = Html::parse_document(html);
    let selector = Selector::parse(
        "a[href], link[href], script[src], img[src], img[srcset], source[src], source[srcset], video[src], iframe[src]",
    )
    .expect("valid selector");

    let mut links = Vec::new();
    for element in document.select(&selector) {
        for attr in ["href", "src"] {
            if let Some(value) = element.value().attr(attr) {
                push_link(&mut links, base, value);
            }
        }
        if let Some(srcset) = element.value().attr("srcset") {
            for candidate in srcset.split(',') {
                if let Some(url) = candidate.split_whitespace().next() {
                    push_link(&mut links, base, url);
                }
            }
        }
    }
    links
}

fn strip_discovery_links(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find("<link") {
        out.push_str(&rest[..start]);
        let tag_end = match rest[start..].find('>') {
            Some(end) => start + end + 1,
            None => {
                out.push_str(&rest[start..]);
                return out;
            }
        };
        let tag = &rest[start..tag_end];
        let server_only = tag.contains("EditURI")
            || tag.contains("api.w.org")
            || tag.contains("wlwmanifest")
            || (tag.contains("alternate") && (tag.contains("rss") || tag.contains("atom")));
        if !server_only {
            out.push_str(tag);
        }
        rest = &rest[tag_end..];
    }
    out.push_str(rest);
    out
}

fn extract_css_urls(css: &str, base: &Url) -> Vec<Url> {
    let mut links = Vec::new();

    let mut rest = css;
    while let Some(start) = rest.find("url(") {
        let after = &rest[start + 4..];
        let Some(end) = after.find(')') else { break };
        let raw = after[..end].trim().trim_matches(|c| c == '"' || c == '\'');
        if !raw.is_empty() && !raw.starts_with("data:") {
            push_link(&mut links, base, raw);
        }
        rest = &after[end + 1..];
    }

    let mut rest = css;
    while let Some(start) = rest.find("@import") {
        let after = rest[start + 7..].trim_start();
        let quote = after.chars().next();
        if let Some(q @ ('"' | '\'')) = quote
            && let Some(end) = after[1..].find(q)
        {
            push_link(&mut links, base, &after[1..1 + end]);
            rest = &after[1 + end..];
            continue;
        }
        rest = after;
    }

    links
}

fn extract_sitemap_locs(xml: &str, base: &Url) -> Vec<Url> {
    let mut links = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<loc>") {
        let after = &rest[start + 5..];
        match after.find("</loc>") {
            Some(end) => {
                push_link(&mut links, base, after[..end].trim());
                rest = &after[end + 6..];
            }
            None => break,
        }
    }
    links
}

fn push_link(links: &mut Vec<Url>, base: &Url, value: &str) {
    if let Ok(mut url) = base.join(value.trim()) {
        url.set_fragment(None);
        links.push(url);
    }
}

fn output_path(url: &Url) -> PathBuf {
    let path = url.path().trim_start_matches('/');
    let last = path.rsplit('/').next().unwrap_or("");
    if path.is_empty() || path.ends_with('/') || !last.contains('.') {
        PathBuf::from(path).join("index.html")
    } else {
        PathBuf::from(path)
    }
}

pub fn stored_public_base(out: &Path) -> Option<String> {
    let text = fs::read_to_string(state_path(out)).ok()?;
    let state: ExportState = serde_json::from_str(&text).ok()?;
    Some(state.public_base)
}

#[cfg(test)]
mod tests;
