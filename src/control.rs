use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use crate::config::{self, PORT, UNIT_NAME};
use crate::systemd;

fn send(command: &str) -> Result<String> {
    send_with_timeout(command, Duration::from_secs(5))
}

fn send_with_timeout(command: &str, timeout: Duration) -> Result<String> {
    let mut stream = UnixStream::connect(config::socket_path()?)?;
    stream.set_read_timeout(Some(timeout))?;
    writeln!(stream, "{command}")?;

    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply)?;
    Ok(reply.trim().to_string())
}

pub fn site_command(action: &str, name: &str) -> Result<()> {
    if !is_running() {
        ensure_running()?;
    }
    let reply = send_with_timeout(&format!("{action} {name}"), Duration::from_secs(60))?;
    if reply != "ok" {
        bail!("{reply}");
    }
    Ok(())
}

pub fn open_site(name: &str) -> Result<()> {
    if !crate::sites::is_listening(crate::provision::port_of(name)?) {
        site_command("site-start", name)?;
    }
    xdg_open(&config::gateway_url(name))?;
    Ok(())
}

pub fn is_running() -> bool {
    matches!(send("status"), Ok(reply) if reply == "running")
}

pub fn stop() -> Result<()> {
    if is_running() {
        send("stop")?;
    }
    Ok(())
}

pub fn ensure_running() -> Result<()> {
    if is_running() {
        return Ok(());
    }

    if systemd::unit_installed() {
        systemd::systemctl(&["start", UNIT_NAME])?;
    } else {
        spawn_detached()?;
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if is_running() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!(
        "engine did not start on port {PORT}; see {}",
        log_path()?.display()
    )
}

pub fn open_dashboard() -> Result<()> {
    ensure_running()?;
    xdg_open(&config::dashboard_url())?;
    Ok(())
}

fn xdg_open(url: &str) -> Result<()> {
    Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

fn log_path() -> Result<std::path::PathBuf> {
    Ok(config::runtime_dir()?.join("engine.log"))
}

fn spawn_detached() -> Result<()> {
    config::create_runtime_dir()?;
    let log = File::create(log_path()?)?;
    Command::new(config::executable()?)
        .arg("engine")
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .process_group(0)
        .spawn()?;
    Ok(())
}
