use std::fs;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::config::{self, PORT};
use crate::dashboard::{self, State};
use crate::{gateway, http};

pub fn run() -> Result<()> {
    let dir = config::runtime_dir()?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)?;
    let socket = config::socket_path()?;

    if UnixStream::connect(&socket).is_ok() {
        println!("kandang engine is already running");
        return Ok(());
    }
    let _ = fs::remove_file(&socket);

    let control = UnixListener::bind(&socket).context("binding control socket")?;
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
    control.set_nonblocking(true)?;

    let web = TcpListener::bind(("127.0.0.1", PORT))
        .with_context(|| format!("port {PORT} is already in use"))?;
    web.set_nonblocking(true)?;

    let gateway = TcpListener::bind(("127.0.0.1", config::GATEWAY_PORT))
        .with_context(|| format!("port {} is already in use", config::GATEWAY_PORT))?;
    thread::spawn(move || gateway::serve(gateway));

    let state = Arc::new(State::new()?);
    let result = serve(&control, &web, &state);
    state.stop_all();
    let _ = fs::remove_file(&socket);
    result
}

fn serve(control: &UnixListener, web: &TcpListener, state: &Arc<State>) -> Result<()> {
    loop {
        match control.accept() {
            Ok((stream, _)) => {
                if handle_control(stream, state)? {
                    return Ok(());
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.into()),
        }

        match web.accept() {
            Ok((stream, _)) => {
                let state = Arc::clone(state);
                thread::spawn(move || {
                    let _ = handle_http(stream, &state);
                });
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.into()),
        }

        thread::sleep(Duration::from_millis(20));
    }
}

fn handle_control(stream: UnixStream, state: &State) -> Result<bool> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;

    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    let command = line.trim();

    let (reply, stop) = match command.split_once(' ') {
        _ if command == "status" => ("running".to_string(), false),
        _ if command == "stop" => ("stopping".to_string(), true),
        Some(("site-start", name)) => (site_reply(dashboard::start_site(state, name)), false),
        Some(("site-stop", name)) => (
            site_reply(dashboard::stop_site(state, name).map(|_| false)),
            false,
        ),
        _ => ("error: unknown command".to_string(), false),
    };
    writeln!(&stream, "{reply}")?;
    Ok(stop)
}

fn site_reply(result: Result<bool>) -> String {
    match result {
        Ok(_) => "ok".to_string(),
        Err(e) => format!("error: {e:#}"),
    }
}

fn handle_http(stream: TcpStream, state: &State) -> Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let Some(request) = http::read_request(&stream)? else {
        return Ok(());
    };
    let response = dashboard::handle(state, &request);
    http::write_response(&stream, response)?;
    Ok(())
}
