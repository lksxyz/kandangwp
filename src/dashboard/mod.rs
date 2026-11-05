use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::config::{self, PORT};
use crate::http::{Request, Response};
use crate::{credentials, provision, publish, sites};

const PAGE: &str = include_str!("dashboard.html");
const STYLE: &str = include_str!("dashboard.css");
const SCRIPT: &str = include_str!("dashboard.js");
const MARK: &str = include_str!("../../assets/brand/kandang-mark.svg");
const FONT_REGULAR: &[u8] = include_bytes!("../../assets/font/OpenRunde-Regular.woff2");
const FONT_SEMIBOLD: &[u8] = include_bytes!("../../assets/font/OpenRunde-Semibold.woff2");
const FONT_BOLD: &[u8] = include_bytes!("../../assets/font/OpenRunde-Bold.woff2");
const LOG_LINES: usize = 200;

pub struct State {
    token: Arc<String>,
    servers: Arc<Mutex<HashMap<String, Child>>>,
    publish: Arc<Mutex<HashMap<String, PublishJob>>>,
}

impl State {
    /// A cheap handle sharing the same locks; used to move a clone of the
    /// state into the publish thread.
    fn mirror(state: &State) -> State {
        State {
            token: Arc::clone(&state.token),
            servers: Arc::clone(&state.servers),
            publish: Arc::clone(&state.publish),
        }
    }
}

/// One in-flight publish per site. Phases mirror what the publish thread
/// does, so the UI can show where the work currently is. A finished job stays
/// behind so the dashboard can report the outcome and the deployed address.
#[derive(Clone, Serialize)]
struct PublishJob {
    phase: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    public: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    files: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    changed: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deployed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    finished_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<String>,
}

impl PublishJob {
    fn running(phase: &'static str) -> Self {
        Self {
            phase,
            error: None,
            project: None,
            public: None,
            files: None,
            changed: None,
            deployed: None,
            finished_at: None,
            location: None,
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Terminal jobs stay readable so the dashboard can report the outcome of the
/// last publish even after the thread is gone.
fn done_job(outcome: &publish::Outcome, project: String, public: String) -> PublishJob {
    PublishJob {
        phase: "done",
        project: Some(project),
        public: Some(public),
        files: Some(outcome.files),
        changed: Some(outcome.changed),
        deployed: Some(outcome.deployed),
        finished_at: Some(now_secs()),
        location: outcome.location.clone(),
        ..PublishJob::running("done")
    }
}

fn failed_job(error: String) -> PublishJob {
    PublishJob {
        phase: "error",
        error: Some(error),
        finished_at: Some(now_secs()),
        ..PublishJob::running("error")
    }
}

impl State {
    pub fn new() -> Result<Self> {
        reap_stale_servers();
        let mut bytes = [0u8; 32];
        File::open("/dev/urandom")?.read_exact(&mut bytes)?;
        let token = bytes.iter().map(|b| format!("{b:02x}")).collect();
        Ok(Self {
            token: Arc::new(token),
            servers: Arc::new(Mutex::new(HashMap::new())),
            publish: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn stop_all(&self) {
        let mut servers = self.servers.lock().unwrap_or_else(|e| e.into_inner());
        for (_, mut child) in servers.drain() {
            publish::stop(&mut child);
        }
        save_pids(&servers);
    }
}

fn pid_file() -> Result<PathBuf> {
    Ok(config::runtime_dir()?.join("servers.json"))
}

fn save_pids(servers: &HashMap<String, Child>) {
    let pids: HashMap<&String, u32> = servers.iter().map(|(name, c)| (name, c.id())).collect();
    if let (Ok(path), Ok(text)) = (pid_file(), serde_json::to_string(&pids)) {
        let _ = fs::write(path, text);
    }
}

fn reap_stale_servers() {
    let Ok(path) = pid_file() else { return };
    let Ok(text) = fs::read_to_string(&path) else {
        return;
    };
    let pids: HashMap<String, u32> = serde_json::from_str(&text).unwrap_or_default();
    for (name, pid) in pids {
        let Ok(root) = provision::site_dir(&name) else {
            continue;
        };
        let cmdline = fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
        let needle = root.to_string_lossy().into_owned();
        if String::from_utf8_lossy(&cmdline).contains(&needle) {
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }
    }
    let _ = fs::remove_file(path);
}

pub fn handle(state: &State, request: &Request) -> Response {
    if !allowed_host(request) {
        return Response::text(403, "forbidden host".into());
    }

    let segments: Vec<&str> = request.path.trim_matches('/').split('/').collect();
    let result = match (request.method.as_str(), segments.as_slice()) {
        ("GET", [""]) => return Response::html(PAGE.replace("__TOKEN__", &state.token)),
        ("GET", ["dashboard.css"]) => return Response::css(STYLE),
        ("GET", ["dashboard.js"]) => return Response::js(SCRIPT),
        ("GET", ["favicon.svg"]) => return Response::svg(MARK.replace("#ff5e1f", "#1a1a18")),
        ("GET", ["fonts", "OpenRunde-Regular.woff2"]) => return Response::font(FONT_REGULAR),
        ("GET", ["fonts", "OpenRunde-Semibold.woff2"]) => return Response::font(FONT_SEMIBOLD),
        ("GET", ["fonts", "OpenRunde-Bold.woff2"]) => return Response::font(FONT_BOLD),
        ("GET", ["api", "sites"]) => sites::list().map(|s| json!(s)),
        ("GET", ["api", "trash"]) => sites::list_trash().map(|t| json!(t)),
        ("GET", ["api", "cloudflare"]) => Ok(cloudflare_status()),
        ("GET", ["api", "cloudflare", "projects"]) => {
            publish::list_projects().map(|p| json!({ "projects": p }))
        }
        ("GET", ["api", "status"]) => Ok(json!({
            "dashboard_port": PORT,
            "gateway_port": config::GATEWAY_PORT,
        })),
        ("GET", ["api", "sites", name]) => site_detail(name),
        ("GET", ["api", "sites", name, "publish", "status"]) => {
            let jobs = state.publish.lock().unwrap_or_else(|e| e.into_inner());
            match jobs.get(*name) {
                Some(job) => Ok(json!(job)),
                None => Ok(json!({ "phase": "idle" })),
            }
        }
        ("GET", ["api", "sites", name, "logs"]) => {
            return match read_logs(name) {
                Ok(text) => Response::text(200, text),
                Err(e) => Response::text(400, format!("{e:#}")),
            };
        }
        ("POST", _) => {
            if request.header("x-kandang-token") != Some(state.token.as_str()) {
                return Response::text(403, "missing or wrong token".into());
            }
            if request.header("content-type") != Some("application/json") {
                return Response::text(400, "expected application/json".into());
            }
            match parse_body(request) {
                Ok(body) => post(state, &segments, &body),
                Err(e) => Err(e),
            }
        }
        _ => return Response::text(404, "not found".into()),
    };

    match result {
        Ok(value) => Response::json(200, &value),
        Err(e) => Response::json(400, &json!({ "error": format!("{e:#}") })),
    }
}

fn allowed_host(request: &Request) -> bool {
    let allowed = [format!("localhost:{PORT}"), format!("127.0.0.1:{PORT}")];
    request
        .header("host")
        .is_some_and(|host| allowed.iter().any(|a| a == host))
}

fn parse_body(request: &Request) -> Result<Value> {
    if request.body.is_empty() {
        return Ok(json!({}));
    }
    Ok(serde_json::from_slice(&request.body)?)
}

fn post(state: &State, segments: &[&str], body: &Value) -> Result<Value> {
    match segments {
        ["api", "sites"] => {
            #[derive(Deserialize)]
            struct Create {
                name: String,
                project: Option<String>,
                port: Option<u16>,
            }
            let create: Create = serde_json::from_value(body.clone())?;
            let port = match create.port {
                Some(port) => port,
                None => sites::next_port()?,
            };
            let project = create
                .project
                .as_deref()
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .unwrap_or(create.name.as_str())
                .to_string();
            let info = provision::create(&create.name, port, &project)?;
            Ok(json!({
                "name": info.name,
                "url": info.url,
                "admin_user": info.admin_user,
                "admin_password": info.admin_password,
                "password_stored": info.password_stored,
            }))
        }
        ["api", "sites", name, "admin-password"] => {
            provision::port_of(name)?;
            let password = credentials::load_site_password(name)?
                .context("the admin password is not in the desktop keyring; reset it instead")?;
            Ok(json!({ "user": "admin", "password": password }))
        }
        ["api", "sites", name, "admin-password", "reset"] => {
            provision::port_of(name)?;
            let password = provision::random_password()?;
            provision::reset_admin_password(name, &password)?;
            let stored = credentials::store_site_password(name, &password).is_ok();
            Ok(json!({ "user": "admin", "password": password, "stored": stored }))
        }
        ["api", "sites", name, "start"] => {
            start(state, name)?;
            Ok(json!({ "running": true }))
        }
        ["api", "sites", name, "stop"] => {
            stop(state, name)?;
            Ok(json!({ "running": false }))
        }
        ["api", "cloudflare", "connect"] => {
            crate::cloudflare::start()?;
            Ok(json!({ "started": true }))
        }
        ["api", "cloudflare", "disconnect"] => {
            crate::cloudflare::disconnect()?;
            Ok(json!({ "connected": false }))
        }
        ["api", "sites", name, "publish"] => {
            provision::port_of(name)?;
            let target = publish::active_target();
            let dry_run = body
                .get("dry_run")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if dry_run && target == publish::Target::Zip {
                bail!("a dry run previews Cloudflare publishes only");
            }
            let (project, public) = match target {
                publish::Target::Cloudflare => cloudflare_destination(name, body)?,
                publish::Target::Zip => (String::new(), zip_destination(name, body)),
            };

            // A finished job stays readable for the dashboard's result strip;
            // only a running one blocks the next attempt.
            if publish_running(state, name) {
                bail!("publish is already running for {name}");
            }
            if !dry_run {
                // Claim the job before the export runs, so the dashboard's
                // status poll reports the publish as soon as it is accepted.
                state
                    .publish
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(name.to_string(), PublishJob::running("starting"));
            }
            // Exporting needs the site server. Starting it is part of the
            // publish and the site stays up afterwards, so the authoring
            // window the user is working in does not disappear.
            let setup = (|| -> Result<()> {
                start(state, name)?;
                publish::export_site(name, &public).map(|_| ())
            })();
            if let Err(e) = setup {
                if !dry_run {
                    state
                        .publish
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(name.to_string(), failed_job(format!("{e:#}")));
                }
                return Err(e);
            }

            if dry_run {
                let plan = publish::plan(name, &public, &project)?;
                return Ok(json!({
                    "dry_run": true,
                    "files": plan.files,
                    "changed": plan.changed,
                    "published": public,
                    "project": project,
                }));
            }
            let state = State::mirror(state);
            let publish_jobs = Arc::clone(&state.publish);
            let job_name = name.to_string();
            let track_name = job_name.clone();
            let job_project = project.clone();
            let job_public = public.clone();
            let join = thread::spawn(move || {
                let set_phase = |p: &'static str| {
                    let mut jobs = publish_jobs.lock().unwrap_or_else(|e| e.into_inner());
                    jobs.insert(job_name.clone(), PublishJob::running(p));
                };
                let outcome = (|| {
                    set_phase("starting");
                    start(&state, &job_name)?;
                    set_phase("exporting");
                    publish::export_site(&job_name, &job_public)?;
                    match target {
                        publish::Target::Cloudflare => {
                            set_phase("deploying");
                            publish::deploy_if_changed(&job_name, &job_public, &job_project)
                        }
                        publish::Target::Zip => {
                            set_phase("zipping");
                            publish::export_zip(&job_name)
                        }
                    }
                })();
                let mut jobs = publish_jobs.lock().unwrap_or_else(|e| e.into_inner());
                jobs.insert(
                    job_name.clone(),
                    match &outcome {
                        Ok(outcome) => done_job(outcome, job_project, job_public),
                        Err(e) => failed_job(format!("{e:#}")),
                    },
                );
                outcome.map(|_| ())
            });
            track_join(&track_name, join);
            Ok(json!({
                "started": true,
                "target": target,
                "project": project,
                "published": public,
            }))
        }
        ["api", "sites", name, "trash"] => {
            stop(state, name)?;
            sites::trash(name)?;
            Ok(json!({ "trashed": name }))
        }
        ["api", "trash", id, "restore"] => {
            let name = sites::restore(id)?;
            Ok(json!({ "restored": name }))
        }
        ["api", "trash", id, "purge"] => {
            sites::purge(id, &string_field(body, "confirm")?)?;
            Ok(json!({ "purged": id }))
        }
        _ => bail!("unknown action"),
    }
}

fn cloudflare_status() -> Value {
    json!({ "connected": crate::cloudflare::connected() })
}

/// The Cloudflare destination for a site. The project follows the site slug;
/// the public address is the one Cloudflare assigned unless a custom domain is set.
fn cloudflare_destination(name: &str, body: &Value) -> Result<(String, String)> {
    let saved = publish::read_settings(name)?;
    let project = name.to_string();
    let mut public = body
        .get("public")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let guessed = format!("https://{project}.pages.dev");
    if public.is_empty() || public == guessed {
        // A guess from before the project existed would 404 forever.
        public = publish::default_public(&project);
    }
    if public != saved.public {
        publish::write_settings(
            name,
            &publish::Settings {
                public: public.clone(),
                project: project.clone(),
            },
        )?;
    }
    Ok((project, public))
}

/// The zip target has no hosting address; links use the given public address,
/// or the local gateway address when none is given.
fn zip_destination(name: &str, body: &Value) -> String {
    body.get("public")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|public| !public.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| config::gateway_root(name))
}

fn site_detail(name: &str) -> Result<Value> {
    let port = provision::port_of(name)?;
    let settings = publish::read_settings(name)?;
    let exported = provision::site_dir(name)?
        .join("public/index.html")
        .is_file();
    Ok(json!({
        "name": name,
        "port": port,
        "url": config::gateway_root(name),
        "preview_url": exported.then(|| format!("{}/", config::preview_url(name))),
        "running": sites::is_listening(port),
        "exported": exported,
        "cloudflare_connected": crate::cloudflare::connected(),
        "cloudflare_account_id": publish::account_id(),
        "settings": settings,
        "title": provision::project_name(name),
        "deployed": publish::deployed_target(name),
    }))
}

fn string_field(body: &Value, key: &str) -> Result<String> {
    body.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .with_context(|| format!("missing field {key}"))
}

pub fn start_site(state: &State, name: &str) -> Result<bool> {
    start(state, name)
}

pub fn stop_site(state: &State, name: &str) -> Result<()> {
    stop(state, name)
}

fn start(state: &State, name: &str) -> Result<bool> {
    let port = provision::port_of(name)?;
    let mut servers = state.servers.lock().unwrap_or_else(|e| e.into_inner());
    if sites::is_listening(port) {
        return Ok(false);
    }
    let child = publish::serve(name, false)?;
    servers.insert(name.to_string(), child);
    save_pids(&servers);
    Ok(true)
}

fn stop(state: &State, name: &str) -> Result<()> {
    let port = provision::port_of(name)?;
    let mut servers = state.servers.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(mut child) = servers.remove(name) {
        publish::stop(&mut child);
    }
    save_pids(&servers);
    if sites::is_listening(port) {
        bail!("{name} is running outside Kandang and cannot be stopped here");
    }
    Ok(())
}

fn publish_running(state: &State, name: &str) -> bool {
    state
        .publish
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(name)
        .is_some_and(|job| job.phase != "done" && job.phase != "error")
}

fn track_join(_name: &str, join: JoinHandle<Result<()>>) {
    // The thread reports its own result; the handle is only reaped so the
    // OS does not keep zombie bookkeeping around after long sessions.
    std::thread::spawn(move || {
        let _ = join.join();
    });
}

fn read_logs(name: &str) -> Result<String> {
    let path = publish::log_path(name)?;
    let text = fs::read_to_string(&path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(LOG_LINES);
    Ok(lines[start..].join("\n"))
}
