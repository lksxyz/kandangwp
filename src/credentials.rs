use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

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
    // keyring for an item: running at all means the binary is present.
    secret_tool()
        .args(["lookup", "service", "kandang"])
        .output()
        .is_ok()
}

fn store_with(attrs: &[&str], label: &str, secret: &str) -> Result<()> {
    let secret = secret.trim();
    if secret.is_empty() {
        bail!("the secret is empty");
    }
    if secret.contains(['\n', '\r']) {
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
        .write_all(secret.as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        let detail = String::from_utf8_lossy(&out.stderr);
        bail!(
            "could not save the secret in the keyring: {}",
            detail.trim()
        );
    }
    Ok(())
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
    let secret = String::from_utf8(out.stdout)
        .context("keyring returned a non-UTF-8 secret")?
        .trim_end_matches(['\n', '\r'])
        .to_string();
    Ok((!secret.is_empty()).then_some(secret))
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
        "could not remove the secret from the keyring: {}",
        detail.trim()
    )
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

const CLOUDFLARE_ATTRS: [&str; 4] = ["service", "kandang", "key", "cloudflare-session"];
const CLOUDFLARE_LABEL: &str = "Kandang Cloudflare session";

pub fn store_cloudflare_session(session: &str) -> Result<()> {
    store_with(&CLOUDFLARE_ATTRS, CLOUDFLARE_LABEL, session)
}

pub fn load_cloudflare_session() -> Result<Option<String>> {
    load_with(&CLOUDFLARE_ATTRS)
}

pub fn clear_cloudflare_session() -> Result<()> {
    clear_with(&CLOUDFLARE_ATTRS)
}
