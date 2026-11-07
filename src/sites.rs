use std::collections::HashSet;
use std::fs;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::config;
use crate::provision;

const PORT_RANGE: std::ops::Range<u16> = 8100..9000;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SiteSummary {
    pub name: String,
    pub title: String,
    pub port: u16,
    pub url: String,
    pub running: bool,
    pub exported: bool,
}

#[derive(Serialize)]
pub struct TrashedSite {
    pub id: String,
    pub name: String,
}

pub fn list() -> Result<Vec<SiteSummary>> {
    let dir = provision::sites_dir()?;
    fs::create_dir_all(&dir)?;
    let mut sites = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || !entry.file_type()?.is_dir() {
            continue;
        }
        let Ok(port) = provision::port_of(&name) else {
            continue;
        };
        sites.push(SiteSummary {
            name: name.clone(),
            title: provision::project_name(&name),
            port,
            url: config::gateway_root(&name),
            running: is_listening(port),
            exported: provision::is_exported(&name),
        });
    }
    sites.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(sites)
}

fn trash_dir() -> Result<PathBuf> {
    Ok(provision::sites_dir()?.join(".trash"))
}

pub fn trash(name: &str) -> Result<()> {
    let from = provision::site_dir(name)?;
    if !from.exists() {
        bail!("no site named {name}");
    }
    let dir = trash_dir()?;
    fs::create_dir_all(&dir)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    fs::rename(&from, dir.join(format!("{name}--{stamp}")))?;
    Ok(())
}

pub fn list_trash() -> Result<Vec<TrashedSite>> {
    let dir = trash_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut items = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let id = entry?.file_name().to_string_lossy().into_owned();
        if let Ok((name, _)) = split_id(&id) {
            items.push(TrashedSite { id, name });
        }
    }
    items.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(items)
}

pub fn restore(id: &str) -> Result<String> {
    let (name, _) = split_id(id)?;
    let to = provision::site_dir(&name)?;
    if to.exists() {
        bail!("a site named {name} already exists");
    }
    fs::rename(trash_dir()?.join(id), &to)?;
    Ok(name)
}

pub fn purge(id: &str, confirm: &str) -> Result<()> {
    let (name, _) = split_id(id)?;
    if confirm != name {
        bail!("type the site name to confirm permanent deletion");
    }
    fs::remove_dir_all(trash_dir()?.join(id))?;
    let _ = crate::credentials::clear_site_password(&name);
    Ok(())
}

fn split_id(id: &str) -> Result<(String, String)> {
    if id.contains('/') || id.contains("..") || id.starts_with('.') {
        bail!("invalid trash id");
    }
    let (name, stamp) = id.rsplit_once("--").context("invalid trash id")?;
    Ok((name.to_string(), stamp.to_string()))
}

pub fn next_port() -> Result<u16> {
    let mut used = HashSet::new();
    for dir in [provision::sites_dir()?, trash_dir()?] {
        if !dir.exists() {
            continue;
        }
        for entry in fs::read_dir(dir)? {
            if let Ok(port) = read_port(&entry?.path()) {
                used.insert(port);
            }
        }
    }
    PORT_RANGE
        .clone()
        .find(|port| !used.contains(port) && !is_listening(*port))
        .context("no free site port")
}

fn read_port(site: &Path) -> Result<u16> {
    let raw = fs::read_to_string(site.join(".kandang-port"))?;
    Ok(raw.trim().parse()?)
}

pub fn is_listening(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trash_ids_must_be_plain_names() {
        assert!(split_id("blog--1700000000").is_ok());
        assert!(split_id("../blog--1").is_err());
        assert!(split_id("a/b--1").is_err());
        assert!(split_id(".hidden--1").is_err());
        assert!(split_id("noseparator").is_err());
    }
}
