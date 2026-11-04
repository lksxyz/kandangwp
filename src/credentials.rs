use std::fs::{self, File};
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use anyhow::{Context, Result, bail};

pub const WRANGLER: &str = "wrangler@3.114.17";

const LABEL: &str = "Kandang Cloudflare API token";
const ATTRS: [&str; 4] = ["service", "kandang", "key", "cloudflare"];

fn secret_tool() -> Command {
    let mut command = Command::new("secret-tool");
    command
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

/// Whether the desktop provides a Secret Service implementation.
pub fn available() -> bool {
    // secret-tool has no --version and exits non-zero for usage, so ask the
    // keyring for the item: running at all means the binary is present.
    secret_tool().args(["lookup"]).args(ATTRS).output().is_ok()
}

/// Stores the token in the desktop keyring. The token never touches disk in
/// plain text, and is never written to a log or returned by the Dashboard.
pub fn store(token: &str) -> Result<()> {
    store_with(&ATTRS, LABEL, token)
}

fn store_with(attrs: &[&str], label: &str, secret: &str) -> Result<()> {
    let token = secret.trim();
    if token.is_empty() {
        bail!("the secret is empty");
    }
    if token.contains(['\n', '\r']) {
        bail!("the secret contains a line break");
    }

    let mut child = secret_tool()
        .args(["store", "--label", label])
        .args(attrs)
        .stdin(Stdio::piped())
        .spawn()
        .context("running secret-tool (is libsecret installed?)")?;
    child
        .stdin
        .take()
        .context("opening secret-tool input")?
        .write_all(token.as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        let detail = String::from_utf8_lossy(&out.stderr);
        bail!("could not save the token in the keyring: {}", detail.trim());
    }
    Ok(())
}

/// Reads the token from the desktop keyring, or `None` when not connected.
pub fn load() -> Result<Option<String>> {
    load_with(&ATTRS)
}

fn load_with(attrs: &[&str]) -> Result<Option<String>> {
    let out = secret_tool()
        .args(["lookup"])
        .args(attrs)
        .output()
        .context("running secret-tool (is libsecret installed?)")?;
    // secret-tool exits non-zero when no item matches.
    if !out.status.success() {
        return Ok(None);
    }
    let token = String::from_utf8(out.stdout)
        .context("keyring returned a non-UTF-8 token")?
        .trim_end_matches(['\n', '\r'])
        .to_string();
    Ok((!token.is_empty()).then_some(token))
}

fn site_attrs(name: &str) -> Vec<String> {
    vec![
        "service".into(),
        "kandang".into(),
        "key".into(),
        format!("site-admin-{name}"),
    ]
}

fn site_labels(name: &str) -> String {
    format!("Kandang WordPress admin: {name}")
}

pub fn store_site_password(name: &str, password: &str) -> Result<()> {
    let attrs = site_attrs(name);
    let refs: Vec<&str> = attrs.iter().map(String::as_str).collect();
    store_with(&refs, &site_labels(name), password)
}

pub fn load_site_password(name: &str) -> Result<Option<String>> {
    let attrs = site_attrs(name);
    let refs: Vec<&str> = attrs.iter().map(String::as_str).collect();
    load_with(&refs)
}

pub fn clear_site_password(name: &str) -> Result<()> {
    let attrs = site_attrs(name);
    let refs: Vec<&str> = attrs.iter().map(String::as_str).collect();
    clear_with(&refs)
}

/// How the current Cloudflare credential was obtained.
pub fn method() -> Option<&'static str> {
    if load().ok().flatten().is_some() {
        return Some("token");
    }
    if session_connected() {
        return Some("session");
    }
    None
}

pub fn connected() -> bool {
    method().is_some()
}

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_default()
}

fn wrangler_config() -> PathBuf {
    config_home().join(".wrangler/config/default.toml")
}

/// Whether Wrangler has an OAuth session on this machine.
pub fn session_connected() -> bool {
    fs::read_to_string(wrangler_config())
        .map(|text| {
            text.lines().any(|line| {
                let line = line.trim_start();
                line.starts_with("oauth_token")
                    && line
                        .split_once('=')
                        .map(|(_, value)| value.trim().trim_matches('"').len() > 8)
                        .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// A running `wrangler login` browser sign-in.
pub struct Login {
    child: Child,
    log: PathBuf,
    opened: bool,
}

pub struct LoginStatus {
    pub running: bool,
    pub url: Option<String>,
}

impl Login {
    pub fn update(&mut self) -> LoginStatus {
        let url = self.login_url();
        if !self.opened {
            if let Some(link) = &url {
                let _ = Command::new("xdg-open")
                    .arg(link)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn();
            }
            self.opened = url.is_some();
        }
        LoginStatus {
            running: self.child.try_wait().ok().flatten().is_none(),
            url,
        }
    }

    fn login_url(&self) -> Option<String> {
        let text = fs::read_to_string(&self.log).ok()?;
        let start = text.find("https://dash.cloudflare.com/")?;
        let rest = &text[start..];
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '\x1b' || c == '"' || c == '\'')
            .unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }

    pub fn cancel(&mut self) {
        // The child is a process-group leader, so this also stops the Node
        // process npx starts, which would otherwise keep port 8976 open.
        let pid = self.child.id() as i32;
        // SAFETY: sends a signal to the group we created; no memory is touched.
        unsafe {
            libc::kill(-pid, libc::SIGTERM);
        }
        let _ = self.child.wait();
    }
}

/// Starts `wrangler login` without opening the browser itself, so the
/// Dashboard can show the sign-in link and the engine controls the process.
pub fn start_login() -> Result<Login> {
    if session_connected() {
        bail!("already signed in to Cloudflare");
    }
    let dir = crate::config::runtime_dir()?;
    fs::create_dir_all(&dir)?;
    let log_path = dir.join("wrangler-login.log");
    let log = File::create(&log_path)?;

    // Pages-only scopes: publish works without granting Workers, D1, KV, and
    // the rest of the default wrangler access.
    let mut command = Command::new("npx");
    command
        .args([
            "--yes",
            WRANGLER,
            "login",
            "--browser=false",
            "--scopes",
            "account:read",
            "user:read",
            "pages:write",
            "zone:read",
        ])
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .process_group(0);
    let child = command
        .spawn()
        .context("running npx wrangler login (is Node.js installed?)")?;
    Ok(Login {
        child,
        log: log_path,
        opened: false,
    })
}

pub fn logout_session() -> Result<()> {
    if !session_connected() {
        return Ok(());
    }
    let out = Command::new("npx")
        .args(["--yes", WRANGLER, "logout"])
        .output()
        .context("running npx wrangler logout")?;
    if !out.status.success() {
        let detail = String::from_utf8_lossy(&out.stderr);
        bail!("could not sign out of Wrangler: {}", detail.trim());
    }
    Ok(())
}

pub fn clear() -> Result<()> {
    clear_with(&ATTRS)
}

fn clear_with(attrs: &[&str]) -> Result<()> {
    let out = secret_tool().args(["clear"]).args(attrs).output()?;
    if out.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&out.stderr);
    // Nothing stored is not an error.
    if detail.trim().is_empty() || detail.contains("No such item") {
        return Ok(());
    }
    bail!(
        "could not remove the token from the keyring: {}",
        detail.trim()
    )
}
