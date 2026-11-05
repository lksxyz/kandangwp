use std::env;
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;

use anyhow::Result;

use crate::config::{self, PORT};
use crate::{control, systemd};

pub fn run() -> Result<()> {
    check(
        "XDG_RUNTIME_DIR is set",
        env::var_os("XDG_RUNTIME_DIR").is_some(),
    );

    let running = control::is_running();
    if running {
        check(
            &format!("engine running at {}", config::dashboard_url()),
            true,
        );
    } else {
        let port_free = TcpListener::bind(("127.0.0.1", PORT)).is_ok();
        check(&format!("engine stopped; port {PORT} is free"), port_free);
    }

    check(
        "systemd --user available",
        Command::new("systemctl")
            .args(["--user", "--version"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false),
    );
    check("autostart unit installed", systemd::unit_installed());

    check(
        "StatusNotifier host on session bus (tray can show)",
        Command::new("busctl")
            .args(["--user", "status", "org.kde.StatusNotifierWatcher"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false),
    );

    check(
        "secret-tool available (for site admin passwords)",
        crate::credentials::available(),
    );
    let connected = crate::cloudflare::connected();
    check("Cloudflare signed in", connected);
    if !connected {
        println!("[info] without a Cloudflare sign-in, publish writes a zip of the export");
    }

    let icons = icons_dir();
    let icons_installed = icons.join("scalable/status/kandang-symbolic.svg").is_file()
        && icons.join("scalable/apps/kandang.svg").is_file();
    check("Kandang icons installed", icons_installed);
    if !icons_installed {
        println!(
            "[info] run ./scripts/install-icons.sh so the tray and launchers can find the icons"
        );
    }

    if Path::new("/usr/share/omarchy").exists() {
        println!("[info] Omarchy detected: its bar hosts SNI tray items (Quickshell SystemTray)");
    }
    Ok(())
}

fn icons_dir() -> std::path::PathBuf {
    let data = env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| Path::new(&home).join(".local/share")))
        .unwrap_or_default();
    data.join("icons/hicolor")
}

fn check(label: &str, ok: bool) {
    println!("[{}] {label}", if ok { "ok" } else { "!!" });
}
