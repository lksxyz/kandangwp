use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use url::Url;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

use crate::cloudflare;
use crate::config;
use crate::export;
use crate::pages;
use crate::provision;
use crate::sites;

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
    let out = public_dir(name)?;
    let report = export::export(&export::Options {
        source: Url::parse(&config::gateway_url(name))?,
        public_base: public_base.to_string(),
        out,
        connect: Some(port),
    })?;
    Ok(report.files)
}

fn public_dir(name: &str) -> Result<PathBuf> {
    let dir = provision::site_dir(name)?.join("public");
    if !dir.exists() {
        bail!("nothing exported for {name} yet");
    }
    Ok(dir)
}

/// Where a publish goes. A Cloudflare sign-in makes Cloudflare the active
/// integration; without one, the default is a zip of the export.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    Cloudflare,
    Zip,
}

pub fn active_target() -> Target {
    if crate::cloudflare::connected() {
        Target::Cloudflare
    } else {
        Target::Zip
    }
}

/// The Cloudflare Pages projects in the signed-in account.
pub fn list_projects() -> Result<Vec<pages::Project>> {
    let account = cloudflare::account_id()?;
    pages::list_projects(&account)
}

/// The signed-in Cloudflare account, when connected.
pub fn account_id() -> Option<String> {
    cloudflare::account_id().ok()
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
    pub target: Target,
    /// Deployment URL for Cloudflare, archive path for the zip target.
    pub location: Option<String>,
}

pub fn plan(name: &str, public: &str, project: &str) -> Result<Outcome> {
    let current = current_manifest(name, public, project)?;
    let previous = read_manifest(name)?;
    Ok(Outcome {
        files: current.files.len(),
        changed: changed_files(&previous, &current),
        deployed: false,
        target: Target::Cloudflare,
        location: None,
    })
}

pub fn deploy_if_changed(name: &str, public: &str, project: &str) -> Result<Outcome> {
    let current = current_manifest(name, public, project)?;
    let previous = read_manifest(name)?;
    let changed = changed_files(&previous, &current);
    let files = current.files.len();
    if changed == 0 {
        return Ok(Outcome {
            files,
            changed,
            deployed: false,
            target: Target::Cloudflare,
            location: None,
        });
    }

    let dir = public_dir(name)?;
    let url = pages::deploy(project, &dir)?;
    write_manifest(name, &current)?;
    Ok(Outcome {
        files,
        changed,
        deployed: true,
        target: Target::Cloudflare,
        location: Some(url),
    })
}

/// Packs the exported site into `<site>/export.zip`. Used when no Cloudflare
/// integration is active.
pub fn export_zip(name: &str) -> Result<Outcome> {
    let dir = public_dir(name)?;
    let out = provision::site_dir(name)?.join("export.zip");
    let files = write_zip(&dir, &out)?;
    Ok(Outcome {
        files,
        changed: files,
        deployed: true,
        target: Target::Zip,
        location: Some(out.display().to_string()),
    })
}

fn write_zip(dir: &Path, out: &Path) -> Result<usize> {
    let entries = files_under(dir)?;
    let partial = out.with_extension("zip.part");
    let file = File::create(&partial).context("creating the export archive")?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (relative, path) in &entries {
        zip.start_file(relative.as_str(), options)?;
        io::copy(&mut File::open(path)?, &mut zip)?;
    }
    zip.finish()?;
    fs::rename(&partial, out).context("saving the export archive")?;
    Ok(entries.len())
}

fn current_manifest(name: &str, public: &str, project: &str) -> Result<Manifest> {
    let dir = provision::site_dir(name)?.join("public");
    Ok(Manifest {
        target: format!("{public}|{project}"),
        files: manifest_of(&dir)?,
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
    for (relative, path) in files_under(dir)? {
        files.insert(relative, provision::sha256_of(&path)?);
    }
    Ok(files)
}

/// Every file under `dir` as (forward-slash relative path, absolute path).
pub fn files_under(dir: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut files = Vec::new();
    if dir.exists() {
        collect(dir, dir, &mut files)?;
    }
    files.sort();
    Ok(files)
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, PathBuf)>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(root, &path, files)?;
        } else {
            let relative = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            files.push((relative, path));
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

/// The public address of the last successful Cloudflare deploy, if any.
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
    fn a_new_target_always_counts_as_changed() {
        let before = manifest("old", &[("a", "1")]);
        let after = manifest("new", &[("a", "1")]);
        assert!(changed_files(&before, &after) > 0);
    }

    #[test]
    fn default_public_falls_back_without_a_token() {
        assert_eq!(
            default_public("no-such-project-xyz"),
            "https://no-such-project-xyz.pages.dev"
        );
    }

    #[test]
    fn zip_holds_every_exported_file_at_its_relative_path() {
        let root = std::env::temp_dir().join(format!("kandang-zip-{}", std::process::id()));
        let site = root.join("public");
        fs::create_dir_all(site.join("assets")).unwrap();
        fs::write(site.join("index.html"), "<h1>hi</h1>").unwrap();
        fs::write(site.join("assets/app.css"), "body{}").unwrap();
        let out = root.join("export.zip");

        let written = write_zip(&site, &out).unwrap();

        let mut archive = zip::ZipArchive::new(File::open(&out).unwrap()).unwrap();
        let mut names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().unwrap().to_string())
            .collect();
        names.sort();
        assert_eq!(written, 2);
        assert_eq!(names, vec!["assets/app.css", "index.html"]);
        fs::remove_dir_all(&root).unwrap();
    }
}
