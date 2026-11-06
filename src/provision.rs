use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::config;

const UNZIP_SCRIPT: &str = r#"<?php
$zip = new ZipArchive();
if ($zip->open($argv[1]) !== true) { fwrite(STDERR, "cannot open zip\n"); exit(1); }
$zip->extractTo($argv[2]);
$zip->close();
"#;

struct Artifact {
    file: &'static str,
    url: &'static str,
    sha256: &'static str,
}

const FRANKENPHP: Artifact = Artifact {
    file: "frankenphp",
    url: "https://github.com/lksxyz/kandangwp/releases/download/runtime-frankenphp-1.13.4-upx/frankenphp-linux-x86_64-upx",
    sha256: "725ce5965d6170fc136f0302885390fcaa6b4daa3d06cd3c1b688e0c1e15dc50",
};
const WP_CLI: Artifact = Artifact {
    file: "wp-cli.phar",
    url: "https://raw.githubusercontent.com/wp-cli/builds/gh-pages/phar/wp-cli.phar",
    sha256: "ce34ddd838f7351d6759068d09793f26755463b4a4610a5a5c0a97b68220d85c",
};
const WORDPRESS: Artifact = Artifact {
    file: "wordpress.zip",
    url: "https://downloads.wordpress.org/release/wordpress-7.1.4.zip",
    sha256: "a147146c159362778ecbbbfb07bb20ddf4e85b130f971c824e656a8f938c5f87",
};
const SQLITE_PLUGIN: Artifact = Artifact {
    file: "sqlite.zip",
    url: "https://github.com/WordPress/sqlite-database-integration/releases/download/v3.1.2/plugin-sqlite-database-integration.zip",
    sha256: "67dd43f93826c5d222a1d055c432ac5d11279503e4628dce3de72509f48c8a6a",
};

const PLUGIN_DIR: &str = "plugin-sqlite-database-integration";

fn data_runtime_dir() -> Result<PathBuf> {
    Ok(config::data_dir()?.join("runtime"))
}

pub fn sites_dir() -> Result<PathBuf> {
    Ok(config::data_dir()?.join("sites"))
}

pub fn site_dir(name: &str) -> Result<PathBuf> {
    if !valid_name(name) {
        bail!("invalid site name {name:?}");
    }
    Ok(sites_dir()?.join(name))
}

pub fn is_exported(name: &str) -> bool {
    site_dir(name).is_ok_and(|dir| dir.join("public/index.html").is_file())
}

pub fn frankenphp() -> Result<PathBuf> {
    Ok(data_runtime_dir()?.join(FRANKENPHP.file))
}

pub fn ensure_runtime(progress: &mut dyn FnMut(&str, u64, u64)) -> Result<()> {
    let dir = data_runtime_dir()?;
    fs::create_dir_all(&dir)?;
    let artifacts = [&FRANKENPHP, &WP_CLI, &WORDPRESS, &SQLITE_PLUGIN];
    let mut sizes = Vec::with_capacity(artifacts.len());
    for artifact in artifacts {
        let dest = dir.join(artifact.file);
        let size = if is_current(artifact, &dest)? {
            fs::metadata(&dest)?.len()
        } else {
            remote_size(artifact.url)?
        };
        sizes.push(size);
    }
    let total: u64 = sizes.iter().sum();
    let mut finished = 0u64;
    for (artifact, size) in artifacts.iter().zip(sizes) {
        let base = finished;
        ensure_artifact(artifact, &dir.join(artifact.file), &mut |done| {
            progress(artifact.file, base + done, total)
        })?;
        finished += size;
    }
    fs::write(dir.join("unzip.php"), UNZIP_SCRIPT)?;
    Ok(())
}

/// True when every runtime artifact is on disk. Downloads land in a `.part`
/// file and are renamed only after the hash checks out, so presence is enough.
fn runtime_present() -> bool {
    let Ok(dir) = data_runtime_dir() else {
        return false;
    };
    [
        FRANKENPHP.file,
        WP_CLI.file,
        WORDPRESS.file,
        SQLITE_PLUGIN.file,
        "unzip.php",
    ]
    .iter()
    .all(|file| dir.join(file).exists())
}

fn is_current(artifact: &Artifact, dest: &Path) -> Result<bool> {
    Ok(dest.exists() && sha256_of(dest)? == artifact.sha256)
}

fn remote_size(url: &str) -> Result<u64> {
    let response = ureq::head(url)
        .call()
        .with_context(|| format!("checking {url}"))?;
    response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .with_context(|| format!("no content-length for {url}"))
}

fn ensure_artifact(artifact: &Artifact, dest: &Path, progress: &mut dyn FnMut(u64)) -> Result<()> {
    if is_current(artifact, dest)? {
        progress(fs::metadata(dest)?.len());
        return Ok(());
    }

    let partial = PathBuf::from(format!("{}.part", dest.display()));
    let mut response = ureq::get(artifact.url)
        .call()
        .with_context(|| format!("downloading {}", artifact.url))?;
    let mut reader = response.body_mut().as_reader();
    let mut file = File::create(&partial)?;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut written = 0u64;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])?;
        written += read as u64;
        progress(written);
    }
    file.flush()?;
    drop(file);

    let actual = sha256_of(&partial)?;
    if actual != artifact.sha256 {
        let _ = fs::remove_file(&partial);
        bail!("checksum mismatch for {}: got {actual}", artifact.file);
    }
    fs::rename(&partial, dest)?;
    if artifact.file == FRANKENPHP.file {
        make_executable(dest)?;
    }
    Ok(())
}

pub fn sha256_of(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1 << 16];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

fn php(args: &[&str], cwd: &Path) -> Result<String> {
    let output = Command::new(frankenphp()?)
        .arg("php-cli")
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .output()
        .context("running embedded PHP")?;
    if !output.status.success() {
        bail!(
            "php {} failed: {}{}",
            args.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn wp(site: &Path, args: &[&str]) -> Result<String> {
    let runtime = data_runtime_dir()?;
    let phar = runtime.join(WP_CLI.file);
    let path_arg = format!("--path={}", site.display());
    let mut full = vec![
        phar.to_str().context("runtime path is not UTF-8")?,
        &path_arg,
    ];
    full.extend_from_slice(args);
    php(&full, site)
}

fn unzip(zip: &Path, dest: &Path) -> Result<()> {
    let runtime = data_runtime_dir()?;
    let script = runtime.join("unzip.php");
    let zip_s = zip.to_str().context("zip path is not UTF-8")?;
    let dest_s = dest.to_str().context("dest path is not UTF-8")?;
    php(
        &[
            script.to_str().context("script path is not UTF-8")?,
            zip_s,
            dest_s,
        ],
        &runtime,
    )?;
    Ok(())
}

pub struct SiteInfo {
    pub name: String,
    pub url: String,
    pub admin_user: String,
    pub admin_password: String,
    pub password_stored: bool,
}

pub fn create(name: &str, port: u16, project: &str) -> Result<SiteInfo> {
    if !valid_name(name) {
        bail!("site names use lowercase letters, digits, and dashes only");
    }
    let target = site_dir(name)?;
    if target.exists() {
        bail!("a site named {name} already exists");
    }
    if !runtime_present() {
        bail!("the runtime is not installed; download it from the dashboard first");
    }

    fs::create_dir_all(&target)?;
    match build_site(&target, name, port, project) {
        Ok(info) => Ok(info),
        Err(e) => {
            let _ = fs::remove_dir_all(&target);
            Err(e)
        }
    }
}

fn build_site(site: &Path, name: &str, port: u16, project: &str) -> Result<SiteInfo> {
    let runtime = data_runtime_dir()?;
    let extract = site.join(".extract");
    fs::create_dir_all(&extract)?;
    unzip(&runtime.join(WORDPRESS.file), &extract)?;
    copy_dir(&extract.join("wordpress"), site)?;
    fs::remove_dir_all(&extract)?;

    let plugins = site.join("wp-content/plugins");
    fs::create_dir_all(&plugins)?;
    unzip(&runtime.join(SQLITE_PLUGIN.file), &plugins)?;

    let plugin_path = plugins.join(PLUGIN_DIR);
    let db_copy = fs::read_to_string(plugin_path.join("db.copy"))?;
    let db_php = db_copy
        .replace(
            "{SQLITE_IMPLEMENTATION_FOLDER_PATH}",
            plugin_path.to_str().context("site path is not UTF-8")?,
        )
        .replace("{SQLITE_PLUGIN}", &format!("{PLUGIN_DIR}/load.php"));
    fs::write(site.join("wp-content/db.php"), db_php)?;

    let url = config::gateway_root(name);
    let admin_password = random_password()?;
    let admin_user = "admin".to_string();

    wp(
        site,
        &[
            "config",
            "create",
            "--dbname=wordpress",
            "--dbuser=unused",
            "--dbpass=unused",
            "--skip-check",
            "--quiet",
        ],
    )?;
    wp(
        site,
        &[
            "core",
            "install",
            &format!("--url={url}"),
            &format!("--title={project}"),
            &format!("--admin_user={admin_user}"),
            &format!("--admin_password={admin_password}"),
            &format!("--admin_email=admin@{name}.test"),
            "--skip-email",
        ],
    )?;
    wp(
        site,
        &["option", "update", "permalink_structure", "/%postname%/"],
    )?;
    wp(site, &["rewrite", "flush"])?;
    fs::write(site.join(".kandang-port"), port.to_string())?;
    fs::write(site.join(".kandang-project"), project)?;
    let password_stored = crate::credentials::store_site_password(name, &admin_password).is_ok();

    Ok(SiteInfo {
        name: name.to_string(),
        url,
        admin_user,
        admin_password,
        password_stored,
    })
}

pub fn port_of(name: &str) -> Result<u16> {
    let raw = fs::read_to_string(site_dir(name)?.join(".kandang-port"))
        .with_context(|| format!("no site named {name}"))?;
    raw.trim().parse().context("corrupt site port")
}

/// The human project name shown in the dashboard. Sites created before the
/// name was stored only have the slug, so fall back to title-casing it.
pub fn project_name(name: &str) -> String {
    site_dir(name)
        .ok()
        .and_then(|dir| fs::read_to_string(dir.join(".kandang-project")).ok())
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
        .unwrap_or_else(|| title_from_slug(name))
}

fn title_from_slug(slug: &str) -> String {
    slug.split('-')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            fs::create_dir_all(&target)?;
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

pub fn random_password() -> Result<String> {
    const ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut bytes = [0u8; 20];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes
        .iter()
        .map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char)
        .collect())
}

pub fn reset_admin_password(name: &str, password: &str) -> Result<()> {
    let site = site_dir(name)?;
    wp(
        &site,
        &[
            "user",
            "update",
            "admin",
            &format!("--user_pass={password}"),
            "--skip-email",
        ],
    )?;
    Ok(())
}

pub fn ensure_gateway_url(name: &str) -> Result<()> {
    let site = site_dir(name)?;
    let target = config::gateway_root(name);
    let current = last_line(&wp(&site, &["option", "get", "siteurl"])?);
    if current == target {
        return Ok(());
    }
    let port = port_of(name)?;
    let old = format!("http://127.0.0.1:{port}");
    let previous_gateway = format!("http://{name}.localhost:");
    if current.starts_with(&previous_gateway) {
        wp(
            &site,
            &[
                "search-replace",
                &current,
                &target,
                "--all-tables",
                "--skip-columns=guid",
                "--quiet",
            ],
        )?;
    } else if current == old || current.starts_with(&old) {
        wp(
            &site,
            &[
                "search-replace",
                &old,
                &target,
                "--all-tables",
                "--skip-columns=guid",
                "--quiet",
            ],
        )?;
    }
    wp(&site, &["option", "update", "home", &target, "--quiet"])?;
    wp(&site, &["option", "update", "siteurl", &target, "--quiet"])?;
    Ok(())
}

fn last_line(output: &str) -> String {
    output
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_fallback_titles_each_word() {
        assert_eq!(title_from_slug("project-pemda-01"), "Project Pemda 01");
        assert_eq!(title_from_slug("myproject"), "Myproject");
        assert_eq!(title_from_slug(""), "");
    }
}
