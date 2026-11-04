use std::thread;
use std::time::Duration;

use anyhow::{Result, bail};
use ksni::blocking::TrayMethods;
use ksni::menu::{MenuItem, StandardItem, SubMenu};
use ksni::{ToolTip, Tray};

use crate::control;
use crate::sites::{self, SiteSummary};

#[derive(Debug, Clone, PartialEq)]
struct Snapshot {
    running: bool,
    sites: Vec<SiteSummary>,
}

#[derive(Debug)]
struct KandangTray {
    snapshot: Snapshot,
}

impl KandangTray {
    fn running(&self) -> bool {
        self.snapshot.running
    }
}

impl Tray for KandangTray {
    fn id(&self) -> String {
        "kandang".into()
    }

    fn title(&self) -> String {
        "Kandang".into()
    }

    fn icon_name(&self) -> String {
        if self.running() {
            "kandang-symbolic".into()
        } else {
            "kandang-stopped-symbolic".into()
        }
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "Kandang".into(),
            description: if self.running() {
                format!("Engine running at {}", crate::config::dashboard_url())
            } else {
                "Engine stopped".into()
            },
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        thread::spawn(|| report(control::open_dashboard()));
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let running = self.running();
        vec![
            StandardItem {
                label: if running {
                    "Engine: running"
                } else {
                    "Engine: stopped"
                }
                .into(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: if running {
                    "Stop Engine"
                } else {
                    "Start Engine"
                }
                .into(),
                activate: Box::new(move |_: &mut Self| {
                    thread::spawn(move || {
                        report(if running {
                            control::stop()
                        } else {
                            control::ensure_running()
                        })
                    });
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Open Dashboard".into(),
                activate: Box::new(|_: &mut Self| {
                    thread::spawn(|| report(control::open_dashboard()));
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            self.sites_menu(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit Applet".into(),
                activate: Box::new(|_: &mut Self| std::process::exit(0)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

impl KandangTray {
    fn sites_menu(&self) -> MenuItem<Self> {
        let mut items: Vec<MenuItem<Self>> = Vec::new();
        for site in &self.snapshot.sites {
            let name = site.name.clone();
            let open_name = name.clone();
            let toggle_name = name.clone();
            let toggle_running = site.running;
            items.push(
                SubMenu {
                    label: format!(
                        "{} ({})",
                        site.name,
                        if site.running { "running" } else { "stopped" }
                    ),
                    submenu: vec![
                        StandardItem {
                            label: "Open in browser".into(),
                            activate: Box::new(move |_: &mut Self| {
                                let name = open_name.clone();
                                thread::spawn(move || report(control::open_site(&name)));
                            }),
                            ..Default::default()
                        }
                        .into(),
                        StandardItem {
                            label: if toggle_running { "Stop" } else { "Start" }.into(),
                            activate: Box::new(move |_: &mut Self| {
                                let name = toggle_name.clone();
                                thread::spawn(move || {
                                    report(control::site_command(
                                        if toggle_running {
                                            "site-stop"
                                        } else {
                                            "site-start"
                                        },
                                        &name,
                                    ))
                                });
                            }),
                            ..Default::default()
                        }
                        .into(),
                    ],
                    ..Default::default()
                }
                .into(),
            );
        }
        if items.is_empty() {
            items.push(
                StandardItem {
                    label: "No sites yet".into(),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            );
        }
        SubMenu {
            label: "Sites".into(),
            submenu: items,
            ..Default::default()
        }
        .into()
    }
}

fn snapshot() -> Snapshot {
    Snapshot {
        running: control::is_running(),
        sites: sites::list().unwrap_or_default(),
    }
}

pub fn run() -> Result<()> {
    let handle = KandangTray {
        snapshot: snapshot(),
    }
    .spawn()
    .map_err(|e| {
        anyhow::anyhow!(
            "no StatusNotifier host on this session ({e}); use the `kandang` CLI instead"
        )
    })?;

    let mut last: Option<Snapshot> = None;
    loop {
        if handle.is_closed() {
            bail!("tray host went away");
        }
        let current = snapshot();
        if last.as_ref() != Some(&current) {
            handle.update(|tray| tray.snapshot = current.clone());
            last = Some(current);
        }
        thread::sleep(Duration::from_secs(1));
    }
}

fn report(result: Result<()>) {
    if let Err(e) = result {
        eprintln!("kandang: {e:#}");
    }
}
