use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Result, bail};

use crate::config::{self, UNIT_NAME};

pub fn unit_installed() -> bool {
    config::unit_path()
        .map(|path| path.exists())
        .unwrap_or(false)
}

pub fn systemctl(args: &[&str]) -> Result<()> {
    let status = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()?;
    if !status.success() {
        bail!("systemctl --user {} failed", args.join(" "));
    }
    Ok(())
}

pub fn enable() -> Result<()> {
    let path = config::unit_path()?;
    fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    fs::write(&path, unit_text(&config::executable()?))?;
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", UNIT_NAME])?;
    println!("autostart enabled: engine starts at login");
    Ok(())
}

pub fn disable() -> Result<()> {
    if unit_installed() {
        systemctl(&["disable", "--now", UNIT_NAME])?;
        fs::remove_file(config::unit_path()?)?;
        systemctl(&["daemon-reload"])?;
    }
    println!("autostart disabled");
    Ok(())
}

fn unit_text(executable: &Path) -> String {
    format!(
        "[Unit]\n\
         Description=Kandang local WordPress engine\n\
         \n\
         [Service]\n\
         ExecStart=\"{}\" engine\n\
         Restart=on-failure\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        executable.display()
    )
}
