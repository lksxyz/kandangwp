//! Cloudflare Pages through its REST API (ADR 0017). Uses the Direct Upload
//! flow that Wrangler uses: hash every file, upload only the hashes Pages does
//! not have, mark them uploaded, then create a deployment from a manifest.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::cloudflare::{self, Auth};
use crate::publish;

const PRODUCTION_BRANCH: &str = "main";
const LIST_PAGE_SIZE: usize = 10;
const MAX_ASSET_BYTES: u64 = 25 * 1024 * 1024;
const MAX_ASSET_COUNT: usize = 20_000;
const MAX_BATCH_BYTES: u64 = 40 * 1024 * 1024;
const MAX_BATCH_FILES: usize = 2_000;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Project {
    pub name: String,
    pub subdomain: String,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub production_branch: String,
}

pub fn list_projects(account: &str) -> Result<Vec<Project>> {
    let mut projects = Vec::new();
    for page in 1.. {
        let path =
            format!("/accounts/{account}/pages/projects?per_page={LIST_PAGE_SIZE}&page={page}");
        let batch: Vec<Project> = serde_json::from_value(cloudflare::get(Auth::Session, &path)?)
            .context("reading the Pages project list")?;
        let last = batch.len() < LIST_PAGE_SIZE;
        projects.extend(batch);
        if last {
            return Ok(projects);
        }
    }
    unreachable!("the page loop returns once a page is short")
}

fn create_project(account: &str, name: &str) -> Result<Project> {
    let body = json!({
        "name": name,
        "production_branch": PRODUCTION_BRANCH,
        "deployment_configs": { "production": {}, "preview": {} },
    });
    let created = cloudflare::post(
        Auth::Session,
        &format!("/accounts/{account}/pages/projects"),
        "application/json",
        body.to_string().as_bytes(),
    )?;
    serde_json::from_value(created).context("reading the created Pages project")
}

/// Publishes `dir` to the Pages project `name`, creating the project on first
/// use. Returns the project's public `*.pages.dev` address.
pub fn deploy(name: &str, dir: &Path) -> Result<String> {
    ensure_name(name)?;
    let account = cloudflare::account_id()?;
    let project = match list_projects(&account)?
        .into_iter()
        .find(|project| project.name == name)
    {
        Some(project) => project,
        None => create_project(&account, name)?,
    };
    let assets = collect_assets(dir)?;
    let jwt = upload_token(&account, name)?;
    upload_missing(&jwt, &assets)?;
    create_deployment(&account, name, &project.production_branch, &assets)?;
    Ok(format!("https://{}", project.subdomain))
}

fn ensure_name(name: &str) -> Result<()> {
    let valid = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !valid {
        bail!("Cloudflare Pages project names use lowercase letters, digits, and dashes: {name:?}");
    }
    Ok(())
}

struct Asset {
    path: String,
    file: PathBuf,
    hash: String,
    size: u64,
    content_type: String,
}

fn collect_assets(dir: &Path) -> Result<Vec<Asset>> {
    let files = publish::files_under(dir)?;
    if files.len() > MAX_ASSET_COUNT {
        bail!(
            "Pages takes at most {MAX_ASSET_COUNT} files per deployment; this site has {}",
            files.len()
        );
    }
    files
        .into_iter()
        .map(|(relative, file)| -> Result<Asset> {
            let size = fs::metadata(&file)?.len();
            if size > MAX_ASSET_BYTES {
                bail!("{relative} is {size} bytes; Pages takes files up to 25 MiB");
            }
            let hash = asset_hash(&fs::read(&file)?, &file);
            Ok(Asset {
                path: format!("/{relative}"),
                content_type: content_type(&file),
                file,
                hash,
                size,
            })
        })
        .collect()
}

/// Pages' content hash: BLAKE3 over the base64 of the file followed by its
/// extension, truncated to 32 hex characters. Must match what Pages computes.
fn asset_hash(bytes: &[u8], path: &Path) -> String {
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let input = format!("{}{extension}", STANDARD.encode(bytes));
    blake3::hash(input.as_bytes()).to_hex().as_str()[..32].to_string()
}

fn content_type(path: &Path) -> String {
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();
    if mime.starts_with("text/") && !mime.contains("charset") {
        format!("{mime}; charset=utf-8")
    } else {
        mime
    }
}

fn upload_token(account: &str, project: &str) -> Result<String> {
    let result = cloudflare::get(
        Auth::Session,
        &format!("/accounts/{account}/pages/projects/{project}/upload-token"),
    )?;
    result["jwt"]
        .as_str()
        .map(str::to_string)
        .context("Pages returned no upload token")
}

fn upload_missing(jwt: &str, assets: &[Asset]) -> Result<()> {
    let hashes: BTreeSet<&str> = assets.iter().map(|a| a.hash.as_str()).collect();
    let missing: HashSet<String> = serde_json::from_value::<Vec<String>>(cloudflare::post(
        Auth::Bearer(jwt),
        "/pages/assets/check-missing",
        "application/json",
        &serde_json::to_vec(&json!({ "hashes": hashes }))?,
    )?)
    .context("reading the missing-asset list")?
    .into_iter()
    .collect();

    let mut seen = HashSet::new();
    let mut pending: Vec<&Asset> = assets
        .iter()
        .filter(|a| missing.contains(&a.hash) && seen.insert(a.hash.as_str()))
        .collect();
    pending.sort_by_key(|a| std::cmp::Reverse(a.size));
    let sizes: Vec<u64> = pending.iter().map(|a| a.size).collect();

    for batch in plan_batches(&sizes) {
        let payload = batch
            .iter()
            .map(|&index| upload_entry(pending[index]))
            .collect::<Result<Vec<Value>>>()?;
        cloudflare::post(
            Auth::Bearer(jwt),
            "/pages/assets/upload",
            "application/json",
            &serde_json::to_vec(&payload)?,
        )?;
    }

    cloudflare::post(
        Auth::Bearer(jwt),
        "/pages/assets/upsert-hashes",
        "application/json",
        &serde_json::to_vec(&json!({ "hashes": hashes }))?,
    )?;
    Ok(())
}

fn upload_entry(asset: &Asset) -> Result<Value> {
    let bytes = fs::read(&asset.file)?;
    Ok(json!({
        "key": asset.hash,
        "value": STANDARD.encode(bytes),
        "metadata": { "contentType": asset.content_type },
        "base64": true,
    }))
}

/// Splits sizes (largest first) into upload batches, returning indexes into
/// `sizes`. A batch stays within the byte and file-count limits of one request.
fn plan_batches(sizes: &[u64]) -> Vec<Vec<usize>> {
    let mut batches = Vec::new();
    let mut current = Vec::new();
    let mut bytes = 0;
    for (index, &size) in sizes.iter().enumerate() {
        let full = bytes + size > MAX_BATCH_BYTES || current.len() >= MAX_BATCH_FILES;
        if full && !current.is_empty() {
            batches.push(std::mem::take(&mut current));
            bytes = 0;
        }
        current.push(index);
        bytes += size;
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

fn create_deployment(account: &str, project: &str, branch: &str, assets: &[Asset]) -> Result<()> {
    let manifest: BTreeMap<&str, &str> = assets
        .iter()
        .map(|a| (a.path.as_str(), a.hash.as_str()))
        .collect();
    let boundary = format!("kandang-{}", cloudflare::random_urlsafe(12)?);
    let mut body = Vec::new();
    push_field(
        &mut body,
        &boundary,
        "manifest",
        &serde_json::to_string(&manifest)?,
    );
    if !branch.is_empty() {
        push_field(&mut body, &boundary, "branch", branch);
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    cloudflare::post(
        Auth::Session,
        &format!("/accounts/{account}/pages/projects/{project}/deployments"),
        &format!("multipart/form-data; boundary={boundary}"),
        &body,
    )?;
    Ok(())
}

fn push_field(body: &mut Vec<u8>, boundary: &str, name: &str, value: &str) {
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
        )
        .as_bytes(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batches_split_on_the_byte_limit() {
        let mib = 1024 * 1024;
        assert_eq!(
            plan_batches(&[30 * mib, 15 * mib, mib]),
            vec![vec![0], vec![1, 2]]
        );
    }

    #[test]
    fn batches_split_on_the_file_count_limit() {
        let sizes = vec![1; MAX_BATCH_FILES + 1];
        let batches = plan_batches(&sizes);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), MAX_BATCH_FILES);
        assert_eq!(batches[1], vec![MAX_BATCH_FILES]);
    }

    #[test]
    fn text_assets_declare_utf8() {
        assert_eq!(
            content_type(Path::new("index.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(content_type(Path::new("logo.png")), "image/png");
    }

    #[test]
    fn asset_hash_matches_pages_blake3_of_base64_and_extension() {
        assert_eq!(
            asset_hash(b"hello", Path::new("index.html")),
            "a2b82584e50075886b08927390f2f573"
        );
    }

    #[test]
    fn project_names_are_lowercase_slugs() {
        assert!(ensure_name("my-site-2").is_ok());
        assert!(ensure_name("My Site").is_err());
        assert!(ensure_name("").is_err());
    }
}
