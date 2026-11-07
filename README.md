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

Open the site's Publish tab. The Cloudflare Pages project follows the site name
(`Project Pemda 01` publishes to `project-pemda-01`), so sign in to Cloudflare
once, then choose Publish. The dialog previews the export before anything is deployed.

- The first publish creates the Cloudflare Pages project and uses
  `https://<project>.pages.dev` as the public address. To use your own domain,
  add it in Cloudflare first.
- Publishing starts the site if it is stopped, exports it, and leaves it
  running.
- Nothing reaches Cloudflare until you confirm in the review. Deploy uploads the
  export shown there; without Cloudflare, Write zip saves that same export.
- Nothing is deployed when the exported files are unchanged.

Credentials: Cloudflare sign-in opens your browser and stores the OAuth session in
the desktop keyring (`secret-tool`). Node.js and `wrangler` are not needed. Site
admin passwords stay in the keyring.

## Commands

| Command | Effect |
|---|---|
| `kandang start` / `stop` / `status` | Control the engine |
| `kandang open` | Open the dashboard, starting the engine if needed |
| `kandang site create <name>` | Create a WordPress site |
| `kandang site serve <name>` | Serve it in the foreground |
| `kandang site preview <name>` | Re-export the site and serve the export locally. A stopped site is started for the export and stopped again afterwards |
| `kandang export --source … --public … --out …` | Export a running site to a folder |
| `kandang publish <name> --public … --project …` | Export and deploy |
| `kandang doctor` | Check runtime, port, systemd, and tray support |
| `kandang autostart enable` / `disable` | Start the engine at login |
| `kandang runtime install` / `status` | Download and check the runtime (about 100 MB), or show whether it is installed. Refuses while the engine runs; use the dashboard then |

## Files

- `~/.local/share/kandang/runtime` — PHP, WP-CLI, WordPress, SQLite plugin. Each
  download is checked against a pinned SHA-256.
- `~/.local/share/kandang/sites/<name>` — the site, plus `public/` (last export)
  and `published.json` (deploy manifest).
- `$XDG_RUNTIME_DIR/kandang/control.sock` — control socket, mode 0600.
- `$XDG_RUNTIME_DIR/kandang/engine.log` — output of a detached engine.

**Runtime note.** The bundled FrankenPHP is upstream v1.13.4 packed with UPX
5.0.2 and hosted in this repository's releases
(`runtime-frankenphp-1.13.4-upx`), not the upstream release asset. It is about
52 MB instead of ~175 MB, at the cost of a slower PHP process start (~0.5 s
instead of ~70 ms). Serving and CLI behavior are unchanged, and existing
installs re-download it automatically because the pinned SHA-256 changed.

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
