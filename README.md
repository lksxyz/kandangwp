# Kandang

Kandang runs WordPress on your own machine for writing, then publishes the
result as plain static files. The public site is HTML and assets only: no PHP,
no database, no WordPress login exposed.

It is a Linux desktop tool. A small status-area applet starts and stops a
background engine, and the engine serves a dashboard at
`http://localhost:10042/`.

## Requirements

- Linux with a tray host (the applet uses StatusNotifierItem)
- Rust, to build it
- Node.js 22 or newer, if you publish to Cloudflare Pages
- A Cloudflare account, if you publish

## Build

```sh
cargo build --release
./scripts/install-icons.sh    # tray icon and launcher entry, once
```

Then run `target/release/kandang applet` and use the tray item, or skip the
applet and drive the CLI.

## First run

```sh
kandang start     # engine and the *.localhost gateway
kandang open      # dashboard in your browser
```

Create a site from the dashboard. The dialog asks for a project name; capitals
and spaces are fine, and the site name is derived from it:

```
Project Pemda 01  ->  project-pemda-01  ->  http://project-pemda-01.localhost:18080
```

Author at that address. The admin user is `admin`; the password is generated
once and saved in your desktop keyring, so Overview → Show password can show it
again later.

## Publishing

Open the site's Publish tab. Type a project name (`Pemda Project 1` becomes
`pemda-project-1`), sign in to Cloudflare once, then choose Review and publish.

- The first publish creates the Cloudflare Pages project and uses
  `https://<project>.pages.dev` as the public address. To use your own domain,
  add it in Cloudflare first.
- Publishing starts the site if it is stopped, exports it, and leaves it
  running.
- Nothing is deployed when the exported files are unchanged.

Credentials: Cloudflare's browser sign-in (the same OAuth session Wrangler uses)
or an API token with Account → Cloudflare Pages → Edit. Both stay in the
keyring.

## Commands

| Command | Effect |
|---|---|
| `kandang start` / `stop` / `status` | Control the engine |
| `kandang open` | Open the dashboard, starting the engine if needed |
| `kandang site create <name>` | Create a WordPress site |
| `kandang site serve <name>` | Serve it in the foreground |
| `kandang site preview <name>` | Preview the last static export |
| `kandang export --source … --public … --out …` | Export a running site to a folder |
| `kandang publish <name> --public … --project …` | Export and deploy |
| `kandang doctor` | Check runtime, port, systemd, and tray support |
| `kandang autostart enable` / `disable` | Start the engine at login |

## Files

- `~/.local/share/kandang/runtime` — PHP, WP-CLI, WordPress, SQLite plugin. Each
  download is checked against a pinned SHA-256.
- `~/.local/share/kandang/sites/<name>` — the site, plus `public/` (last export)
  and `published.json` (deploy manifest).
- `$XDG_RUNTIME_DIR/kandang/control.sock` — control socket, mode 0600.
- `$XDG_RUNTIME_DIR/kandang/engine.log` — output of a detached engine.

## Notes

- Sites started from the dashboard stop when the engine stops.
- Without a tray host (GNOME without AppIndicator, or a bare tiling WM with no
  bar) the applet has nowhere to appear. Use the CLI there.
- The dashboard needs the browser only; nothing is installed system-wide.

## License

MIT, see [LICENSE](LICENSE).

## More

- [GLOSSARY.md](GLOSSARY.md) — the words this project uses.
- [DESIGN.md](DESIGN.md) — dashboard design rules.
- [docs/adr/](docs/adr/) — why things are built this way.
