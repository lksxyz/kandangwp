use std::env;
use std::fs::DirBuilder;
use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};

pub const PORT: u16 = 10042;
pub const GATEWAY_PORT: u16 = 18080;
pub const UNIT_NAME: &str = "kandang.service";

pub fn gateway_url(site: &str) -> String {
    format!("http://{site}.localhost:{GATEWAY_PORT}/")
}

pub fn gateway_root(site: &str) -> String {
    gateway_url(site).trim_end_matches('/').to_string()
}

pub fn preview_url(site: &str) -> String {
    format!("http://{site}.preview.localhost:{GATEWAY_PORT}")
}

pub fn dashboard_url() -> String {
    format!("http://localhost:{PORT}/")
}

pub fn runtime_dir() -> Result<PathBuf> {
    let base = env::var_os("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR is not set")?;
    Ok(PathBuf::from(base).join("kandang"))
}

pub fn create_runtime_dir() -> Result<PathBuf> {
    let dir = runtime_dir()?;
    DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
    Ok(dir)
}

pub fn socket_path() -> Result<PathBuf> {
    Ok(runtime_dir()?.join("control.sock"))
}

pub fn data_dir() -> Result<PathBuf> {
    Ok(xdg_base("XDG_DATA_HOME", ".local/share")?.join("kandang"))
}

pub fn unit_path() -> Result<PathBuf> {
    Ok(xdg_base("XDG_CONFIG_HOME", ".config")?
        .join("systemd/user")
        .join(UNIT_NAME))
}

fn xdg_base(var: &str, home_fallback: &str) -> Result<PathBuf> {
    if let Some(dir) = env::var_os(var) {
        return Ok(PathBuf::from(dir));
    }
    let home = env::var_os("HOME").ok_or_else(|| anyhow!("HOME is not set"))?;
    Ok(PathBuf::from(home).join(home_fallback))
}

// An AppImage runs from a temporary mount, so current_exe() is not stable across launches.
pub fn executable() -> Result<PathBuf> {
    match env::var_os("APPIMAGE") {
        Some(path) => Ok(PathBuf::from(path)),
        None => Ok(env::current_exe()?),
    }
}
