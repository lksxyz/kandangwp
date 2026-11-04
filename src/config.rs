use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};

pub const PORT: u16 = 10042;
pub const GATEWAY_PORT: u16 = 18080;
pub const UNIT_NAME: &str = "kandang.service";

pub fn gateway_url(site: &str) -> String {
    format!("http://{site}.localhost:{GATEWAY_PORT}/")
}

pub fn dashboard_url() -> String {
    format!("http://localhost:{PORT}/")
}

pub fn runtime_dir() -> Result<PathBuf> {
    let base = env::var_os("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR is not set")?;
    Ok(PathBuf::from(base).join("kandang"))
}

pub fn socket_path() -> Result<PathBuf> {
    Ok(runtime_dir()?.join("control.sock"))
}

pub fn data_dir() -> Result<PathBuf> {
    let base = match env::var_os("XDG_DATA_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(env::var_os("HOME").ok_or_else(|| anyhow!("HOME is not set"))?)
            .join(".local/share"),
    };
    Ok(base.join("kandang"))
}

pub fn unit_path() -> Result<PathBuf> {
    let config = match env::var_os("XDG_CONFIG_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(env::var_os("HOME").ok_or_else(|| anyhow!("HOME is not set"))?)
            .join(".config"),
    };
    Ok(config.join("systemd/user").join(UNIT_NAME))
}

// An AppImage runs from a temporary mount, so current_exe() is not stable across launches.
pub fn executable() -> Result<PathBuf> {
    match env::var_os("APPIMAGE") {
        Some(path) => Ok(PathBuf::from(path)),
        None => Ok(env::current_exe()?),
    }
}
