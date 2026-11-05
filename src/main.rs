mod applet;
mod cloudflare;
mod config;
mod control;
mod credentials;
mod dashboard;
mod doctor;
mod engine;
mod export;
mod gateway;
mod http;
mod pages;
mod provision;
mod publish;
mod sites;
mod systemd;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "kandang", version, about = "Local WordPress engine controls")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the engine in the foreground (started by systemd or the applet)
    Engine,
    /// Run the status-area control applet
    Applet,
    /// Start the engine
    Start,
    /// Stop the engine
    Stop,
    /// Show whether the engine is running
    Status,
    /// Open the dashboard in the browser, starting the engine if needed
    Open,
    /// Check the environment for the applet and autostart
    Doctor,
    /// Manage starting the engine at login
    Autostart {
        #[command(subcommand)]
        action: Autostart,
    },
    /// Export the running local site to static files
    Export {
        /// Local site address, e.g. http://blog.test/
        #[arg(long)]
        source: String,
        /// Public address the output is served at, e.g. https://example.com
        #[arg(long)]
        public: String,
        /// Output folder (replaced only when the export succeeds)
        #[arg(long)]
        out: PathBuf,
    },
    /// Create and serve local WordPress sites
    Site {
        #[command(subcommand)]
        action: SiteAction,
    },
    /// Export a site; publishes to Cloudflare Pages when a token is connected, else writes a zip
    Publish {
        /// Site name
        name: String,
        /// Public address the site is served at, e.g. https://example.com (required for Cloudflare)
        #[arg(long)]
        public: Option<String>,
        /// Cloudflare Pages project name (defaults to the site name)
        #[arg(long)]
        project: Option<String>,
    },
    /// Print the version
    Version,
}

#[derive(Subcommand)]
enum SiteAction {
    /// Create a site with WordPress and SQLite
    Create {
        /// Site name (lowercase letters, digits, dashes)
        name: String,
        /// Local port the site is served on
        #[arg(long, default_value_t = 8090)]
        port: u16,
    },
    /// Serve a site in the foreground until Ctrl-C
    Serve {
        /// Site name
        name: String,
    },
    /// Preview the exported static files on a local port (no deploy)
    Preview {
        /// Site name
        name: String,
    },
}

#[derive(Subcommand)]
enum Autostart {
    /// Start the engine at login
    Enable,
    /// Stop starting the engine at login
    Disable,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Engine => engine::run(),
        Command::Applet => applet::run(),
        Command::Start => {
            control::ensure_running()?;
            println!("engine running at {}", config::dashboard_url());
            Ok(())
        }
        Command::Stop => {
            control::stop()?;
            println!("engine stopped");
            Ok(())
        }
        Command::Status => {
            if control::is_running() {
                println!("running at {}", config::dashboard_url());
            } else {
                println!("stopped");
            }
            Ok(())
        }
        Command::Open => control::open_dashboard(),
        Command::Doctor => doctor::run(),
        Command::Autostart { action } => match action {
            Autostart::Enable => systemd::enable(),
            Autostart::Disable => systemd::disable(),
        },
        Command::Export {
            source,
            public,
            out,
        } => {
            let report = export::export(&export::Options {
                source: url::Url::parse(&source)?,
                public_base: public,
                out: out.clone(),
                connect: None,
            })?;
            println!(
                "exported {} files ({} reused) to {}",
                report.files,
                report.reused,
                out.display()
            );
            for broken in &report.broken {
                println!("broken: {broken}");
            }
            Ok(())
        }
        Command::Site { action } => match action {
            SiteAction::Create { name, port } => {
                let info = provision::create(&name, port, &name)?;
                println!("site {} created", info.name);
                println!("  url:      {}", info.url);
                println!("  admin:    {} / {}", info.admin_user, info.admin_password);
                if info.password_stored {
                    println!(
                        "  password saved in the desktop keyring (see the Dashboard Overview)"
                    );
                }
                println!("  run:      kandang site serve {}", info.name);
                Ok(())
            }
            SiteAction::Preview { name } => {
                let (mut server, port) = publish::preview(&name)?;
                println!(
                    "previewing the static export at http://127.0.0.1:{port}/ (Ctrl-C to stop)"
                );
                let status = server.wait()?;
                if !status.success() {
                    anyhow::bail!("preview server exited with {status}");
                }
                Ok(())
            }
            SiteAction::Serve { name } => {
                let mut server = publish::serve(&name, true)?;
                println!(
                    "serving {name} at http://127.0.0.1:{}/ (Ctrl-C to stop)",
                    provision::port_of(&name)?
                );
                let status = server.wait()?;
                if !status.success() {
                    anyhow::bail!("site server exited with {status}");
                }
                Ok(())
            }
        },
        Command::Publish {
            name,
            public,
            project,
        } => {
            let target = publish::active_target();
            let public = match (target, public) {
                (publish::Target::Cloudflare, None) => {
                    anyhow::bail!("--public is required when Cloudflare is connected")
                }
                (publish::Target::Cloudflare, Some(public)) => public,
                (publish::Target::Zip, public) => {
                    public.unwrap_or_else(|| config::gateway_root(&name))
                }
            };
            let project = project.unwrap_or_else(|| name.clone());
            let mut server = publish::serve(&name, true)?;
            let exported = publish::export_site(&name, &public);
            publish::stop(&mut server);
            let files = exported?;
            let outcome = match target {
                publish::Target::Cloudflare => {
                    publish::deploy_if_changed(&name, &public, &project)?
                }
                publish::Target::Zip => publish::export_zip(&name)?,
            };
            println!("exported {files} files, {} changed", outcome.changed);
            match (outcome.target, outcome.deployed) {
                (publish::Target::Zip, _) => {
                    println!("wrote {}", outcome.location.unwrap_or_default())
                }
                (publish::Target::Cloudflare, true) => println!("published {name} to {public}"),
                (publish::Target::Cloudflare, false) => {
                    println!("no changes since the last publish; nothing deployed")
                }
            }
            Ok(())
        }
        Command::Version => {
            println!("kandang {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}
