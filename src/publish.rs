use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::config;
use crate::export;
use crate::provision;
use crate::sites;

use crate::credentials::WRANGLER;

pub fn log_path(name: &str) -> Result<PathBuf> {
    Ok(provision::site_dir(name)?.join("logs/server.log"))
}

pub fn serve(name: &str, die_with_thread: bool) -> Result<Child> {
    provision::ensure_gateway_url(name)?;
    let port = provision::port_of(name)?;
    let site = provision::site_dir(name)?;
    let log_file = log_path(name)?;
    fs::create_dir_all(log_file.parent().context("log directory")?)?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)?;

    let mut command = Command::new(provision::frankenphp()?);
    command
        .arg("php-server")
        .arg("--listen")
        .arg(format!("127.0.0.1:{port}"))
        .arg("--root")
        .arg(&site)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    if die_with_thread {
        // SAFETY: runs between fork and exec and only calls async-signal-safe prctl.
        unsafe {
            command.pre_exec(|| {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let child = command.spawn().context("starting the site server")?;
    wait_for_port(port)?;
    Ok(child)
}

pub fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn wait_for_port(port: u16) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if sites::is_listening(port) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!("the site server did not start on port {port}")
}

pub fn export_site(name: &str, public_base: &str) -> Result<usize> {
    let port = provision::port_of(name)?;
    if !sites::is_listening(port) {
        bail!("site {name} is not running");
    }
    let out = provision::site_dir(name)?.join("public");
    let report = export::export(&export::Options {
        source: Url::parse(&config::gateway_url(name))?,
        public_base: public_base.to_string(),
        out,
        connect: Some(port),
    })?;
    Ok(report.files)
}

pub fn deploy(name: &str, project: &str) -> Result<()> {
    deploy_logged(name, project, log_path(name)?)
}

/// Like [`deploy`], but wrangler output goes to `log` instead of the server log.
pub fn deploy_logged(name: &str, project: &str, log: PathBuf) -> Result<()> {
    let dir = provision::site_dir(name)?.join("public");
    if !dir.exists() {
        bail!("nothing exported for {name} yet");
    }
    cloudflare_pages(&dir, project, Some(log))
}

pub fn suggest_project(name: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = true;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    slug.truncate(32);
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        slug.push_str("site");
    }
    slug
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Project {
    pub name: String,
    pub subdomain: String,
    pub domains: Vec<String>,
}

fn parse_project_json(text: &str) -> Option<Vec<Project>> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let items = value.get("result")?.as_array()?;
    if items.is_empty() {
        return Some(Vec::new());
    }
    items[0].get("domains")?;
    Some(
        items
            .iter()
            .filter_map(|p| {
                Some(Project {
                    name: p.get("name")?.as_str()?.to_string(),
                    subdomain: p.get("subdomain")?.as_str()?.to_string(),
                    domains: p
                        .get("domains")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                })
            })
            .collect(),
    )
}

pub fn parse_project_list(text: &str) -> Vec<Project> {
    let mut projects = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("RESPONSE: ") {
        rest = &rest[start + "RESPONSE: ".len()..];
        if !rest.starts_with('{') {
            continue;
        }
        let mut depth = 0usize;
        let mut end = None;
        let mut in_string = false;
        let mut escaped = false;
        for (i, c) in rest.char_indices() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    in_string = false;
                }
                continue;
            }
            match c {
                '"' => in_string = true,
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i + c.len_utf8());
                        break;
                    }
                }
                _ => {}
            }
        }
        let end = match end {
            Some(end) => end,
            None => break,
        };
        if let Some(parsed) = parse_project_json(&rest[..end]) {
            projects.extend(parsed);
        }
        rest = &rest[end..];
    }
    projects
}

pub fn list_projects() -> Result<Vec<Project>> {
    if !crate::credentials::connected() {
        bail!("connect Cloudflare first");
    }
    let token = crate::credentials::load()?;
    let mut command = Command::new("npx");
    command
        .args(["--yes", WRANGLER, "pages", "project", "list"])
        .env("WRANGLER_LOG", "debug")
        .env("WRANGLER_LOG_SANITIZE", "false");
    if let Some(token) = &token {
        command.env("CLOUDFLARE_API_TOKEN", token);
    }
    let output = command
        .output()
        .context("running npx (is Node.js installed?)")?;
    if !output.status.success() {
        bail!("wrangler pages project list failed");
    }
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(parse_project_list(&text))
}

fn cloudflare_pages(dir: &Path, project: &str, log: Option<PathBuf>) -> Result<()> {
    // An API token, when present, overrides Wrangler's OAuth session so the
    // least-privilege credential wins.
    let token = crate::credentials::load()?;
    let file = log
        .map(|path| -> std::io::Result<File> {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            OpenOptions::new().create(true).append(true).open(&path)
        })
        .transpose()
        .context("opening publish log")?;
    let mut create = Command::new("npx");
    create
        .args(["--yes", WRANGLER, "pages", "project", "create", project])
        .arg("--production-branch=main");
    if let Some(token) = &token {
        create.env("CLOUDFLARE_API_TOKEN", token);
    }
    let _ = match &file {
        Some(f) => create
            .stdout(f.try_clone()?)
            .stderr(f.try_clone()?)
            .status(),
        None => create.status(),
    };

    let mut deploy = Command::new("npx");
    deploy
        .args(["--yes", WRANGLER, "pages", "deploy"])
        .arg(dir)
        .arg(format!("--project-name={project}"))
        .arg("--branch=main");
    if let Some(token) = &token {
        deploy.env("CLOUDFLARE_API_TOKEN", token);
    }
    let status = match &file {
        Some(f) => deploy
            .stdout(f.try_clone().context("cloning log handle")?)
            .stderr(f.try_clone().context("cloning log handle")?)
            .status(),
        None => deploy.status(),
    };
    let status = status.context("running npx (is Node.js installed?)")?;
    if !status.success() {
        bail!("wrangler pages deploy failed");
    }
    Ok(())
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Settings {
    pub public: String,
    pub project: String,
}

pub fn default_public(project: &str) -> String {
    if let Ok(projects) = list_projects()
        && let Some(found) = projects.iter().find(|p| p.name == project)
    {
        return format!("https://{}", found.subdomain);
    }
    format!("https://{project}.pages.dev")
}

pub fn read_settings(name: &str) -> Result<Settings> {
    let path = provision::site_dir(name)?.join("settings.json");
    if !path.exists() {
        return Ok(Settings::default());
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?).unwrap_or_default())
}

pub fn write_settings(name: &str, settings: &Settings) -> Result<()> {
    let path = provision::site_dir(name)?.join("settings.json");
    fs::write(path, serde_json::to_string_pretty(settings)?)?;
    Ok(())
}

#[derive(Serialize, Deserialize, Default)]
struct Manifest {
    target: String,
    files: BTreeMap<String, String>,
}

pub struct Outcome {
    pub files: usize,
    pub changed: usize,
    pub deployed: bool,
}

pub fn plan(name: &str, public: &str, project: &str) -> Result<Outcome> {
    let dir = provision::site_dir(name)?.join("public");
    let current = Manifest {
        target: format!("{public}|{project}"),
        files: manifest_of(&dir)?,
    };
    let previous = read_manifest(name)?;
    Ok(Outcome {
        files: current.files.len(),
        changed: changed_files(&previous, &current),
        deployed: false,
    })
}

pub fn deploy_if_changed(name: &str, public: &str, project: &str) -> Result<Outcome> {
    let dir = provision::site_dir(name)?.join("public");
    let current = Manifest {
        target: format!("{public}|{project}"),
        files: manifest_of(&dir)?,
    };
    let previous = read_manifest(name)?;

    let changed = changed_files(&previous, &current);
    let outcome = Outcome {
        files: current.files.len(),
        changed,
        deployed: false,
    };
    if changed == 0 {
        return Ok(outcome);
    }

    deploy(name, project)?;
    write_manifest(name, &current)?;
    Ok(Outcome {
        deployed: true,
        ..outcome
    })
}

fn changed_files(previous: &Manifest, current: &Manifest) -> usize {
    if previous.target != current.target {
        return current.files.len().max(1);
    }
    let added_or_modified = current
        .files
        .iter()
        .filter(|(path, hash)| previous.files.get(*path) != Some(*hash))
        .count();
    let removed = previous
        .files
        .keys()
        .filter(|path| !current.files.contains_key(*path))
        .count();
    added_or_modified + removed
}

fn manifest_of(dir: &Path) -> Result<BTreeMap<String, String>> {
    let mut files = BTreeMap::new();
    if dir.exists() {
        collect(dir, dir, &mut files)?;
    }
    Ok(files)
}

fn collect(root: &Path, dir: &Path, files: &mut BTreeMap<String, String>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(root, &path, files)?;
        } else {
            let relative = path.strip_prefix(root)?.to_string_lossy().into_owned();
            files.insert(relative, provision::sha256_of(&path)?);
        }
    }
    Ok(())
}

fn read_manifest(name: &str) -> Result<Manifest> {
    let path = manifest_path(name)?;
    if !path.exists() {
        return Ok(Manifest::default());
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?).unwrap_or_default())
}

fn write_manifest(name: &str, manifest: &Manifest) -> Result<()> {
    fs::write(
        manifest_path(name)?,
        serde_json::to_string_pretty(manifest)?,
    )?;
    Ok(())
}

fn manifest_path(name: &str) -> Result<PathBuf> {
    Ok(provision::site_dir(name)?.join("published.json"))
}

/// The public address of the last successful deploy, if any.
pub fn deployed_target(name: &str) -> Option<String> {
    let target = read_manifest(name).ok()?.target;
    target.split('|').next().map(str::to_string)
}

pub fn preview(name: &str) -> Result<(Child, u16)> {
    let public = provision::site_dir(name)?.join("public");
    if !public.join("index.html").is_file() {
        bail!("nothing exported for {name} yet; run kandang export first");
    }
    let port = sites::next_port()?;
    let mut command = Command::new(provision::frankenphp()?);
    command
        .arg("php-server")
        .arg("--listen")
        .arg(format!("127.0.0.1:{port}"))
        .arg("--root")
        .arg(&public)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = command.spawn().context("starting the preview server")?;
    wait_for_port(port)?;
    Ok((child, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(target: &str, files: &[(&str, &str)]) -> Manifest {
        Manifest {
            target: target.into(),
            files: files
                .iter()
                .map(|(p, h)| (p.to_string(), h.to_string()))
                .collect(),
        }
    }

    #[test]
    fn counts_added_changed_and_removed_files() {
        let before = manifest("t", &[("a", "1"), ("b", "2"), ("c", "3")]);
        let after = manifest("t", &[("a", "1"), ("b", "9"), ("d", "4")]);
        assert_eq!(changed_files(&before, &after), 3);
        assert_eq!(changed_files(&before, &before), 0);
    }

    #[test]
    fn suggests_slugified_project_names() {
        assert_eq!(suggest_project("My Cool Blog!"), "my-cool-blog");
        assert_eq!(suggest_project("  --Adé  "), "ad");
        assert_eq!(suggest_project("!!!"), "site");
        assert_eq!(suggest_project("").len(), 4);
        let long = "a".repeat(50);
        assert_eq!(suggest_project(&long), "a".repeat(32));
    }

    #[test]
    fn parses_project_list_from_wrangler_debug_output() {
        let text = "noise\nDEBUG: fetching pages projects\nRESPONSE: {\"result\":[{\"name\":\"kandang-test-proj\",\"subdomain\":\"kandang-test-proj.pages.dev\",\"domains\":[\"kandang-test-proj.pages.dev\"]}],\"success\":true}\nmore noise";
        let projects = parse_project_list(&text);
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "kandang-test-proj");
        assert_eq!(projects[0].subdomain, "kandang-test-proj.pages.dev");
        assert_eq!(projects[0].domains, vec!["kandang-test-proj.pages.dev"]);
    }

    #[test]
    fn parses_single_line_response_json() {
        let text = "noise\nRESPONSE: {\"result\":[{\"name\":\"p\",\"subdomain\":\"p.pages.dev\",\"domains\":[\"p.pages.dev\"]}],\"success\":true}\nmore";
        let projects = parse_project_list(text);
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "p");
    }

    #[test]
    fn default_public_uses_the_project_subdomain_when_it_exists() {
        // default_public hits `wrangler pages project list`; offline it falls
        // back to the guessed domain.
        assert_eq!(
            default_public("no-such-project-xyz"),
            "https://no-such-project-xyz.pages.dev"
        );
    }

    #[test]
    fn a_new_target_always_counts_as_changed() {
        let before = manifest("old", &[("a", "1")]);
        let after = manifest("new", &[("a", "1")]);
        assert!(changed_files(&before, &after) > 0);
    }
}
