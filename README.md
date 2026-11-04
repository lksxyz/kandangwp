# Kandang

Local WordPress development, controlled from your desktop status area. The
Dashboard lives in your browser; the desktop only starts and stops the engine.

- **Engine** — headless service serving the Dashboard at `http://localhost:10042/`
- **Control Applet** — status-area item (SNI) to start/stop the engine and open the Dashboard
- **Sites** — local WordPress (SQLite, bundled PHP) for authoring only
- **Publish** — exports a site to static files and deploys them to Cloudflare Pages
- **CLI** — `kandang start|stop|status|open|doctor|autostart|site|export|publish|version`

See [GLOSSARY.md](GLOSSARY.md) and [docs/adr/](docs/adr/) for the design, and
[assets/brand/](assets/brand/) for the identity.

## Dashboard

Open `http://localhost:10042/` (or `kandang open`). From there you can create,
start, stop, publish, and view logs for sites, and move them to the Trash.
Purging a trashed site asks you to type its name.

The Dashboard's actions use a per-run token and a Host check. See
[ADR-0009](docs/adr/0009-dashboard-actions-over-token-protected-http.md).

Site servers started from the Dashboard stop when the engine stops. Sites
started with `kandang site serve` stop when that command exits.

## Static publishing

```sh
kandang site create blog                  # downloads and verifies the runtime once
kandang start                             # engine + gateway, needed for *.localhost
# author at http://blog.localhost:18080/wp-admin
kandang export --source http://blog.localhost:18080/ --public https://example.com --out ./public
kandang publish blog --public https://example.com --project my-blog
```

The Dashboard publish flow asks only for a Cloudflare Pages project name.
Kandang creates the project on the first publish and uses
`https://<project>.pages.dev` as the default public address. To use a custom
domain, add it in Cloudflare, then enter it under **Custom domain (optional)**
and publish again. You do not need to create the project or the default URL in
the Cloudflare dashboard. See
[ADR-0013](docs/adr/0013-publish-by-project-name-with-a-default-pages-dev-url.md).

Each site has a random WordPress admin password. Kandang saves it in the
desktop keyring when the system provides one, so **Overview → Show password**
can reveal it later. If the entry is missing, **Reset password** generates a
new one with WP-CLI and saves it again. See
[ADR-0012](docs/adr/0012-store-site-admin-passwords-in-the-keyring.md).

Requirements for `publish`:
- Node.js 22 or newer on `PATH`. Wrangler 3.114.17 is pinned, so older Node
  releases fail on its engine check.
- A Cloudflare sign-in. In the Dashboard's Publish tab, choose **Sign in with
  Cloudflare**: the browser opens Cloudflare's approval page, and the Dashboard
  updates by itself when the session is saved. This is the same OAuth session
  Wrangler uses. For a narrower credential, choose **Use an API token** and
  paste one scoped to Account → Cloudflare Pages → Edit. Kandang stores the
  token in your desktop keyring, never in a site file, the browser, or a log.
  See [ADR-0011](docs/adr/0011-cloudflare-sign-in-and-credential-storage.md).

The runtime (FrankenPHP 1.13.4, WP-CLI 2.12.0, WordPress 7.1.4, and the SQLite
plugin 3.1.2) lives in `~/.local/share/kandang/runtime`, and each download is
checked against a pinned SHA-256.

Export behavior:
- Pages are exported as `path/index.html`. Assets referenced from HTML or CSS
  (including `?ver=` URLs) are copied.
- Search, feed, author archive, `.php`, and users-sitemap URLs are skipped.
- WordPress discovery tags (RSS, RSD, REST API) are removed from the output.
- A failed export leaves the previous output in place.

The API token path needs an unlocked Secret Service provider (GNOME Keyring,
KWallet, or another libsecret backend). Browser sign-in has no such requirement.
`kandang doctor` reports how Cloudflare is connected. **Disconnect Cloudflare**
in the Publish tab removes the API token and signs Wrangler out.

Publishing only deploys when something changed. Each publish stores a hash
manifest in the site folder, and an unchanged site is reported as "nothing
deployed". Cloudflare also dedupes uploads by hash.

Publishing needs the site server for the export, so **Review and publish**
starts a stopped site first and leaves it running afterwards. You do not have
to start the site before publishing.

Sites are reached at `http://<name>.localhost:18080`, served by the engine's
gateway. See [ADR-0010](docs/adr/0010-localhost-subdomains-through-a-gateway.md).

## Build

```sh
cargo build --release
./scripts/install-icons.sh    # tray and launcher icons, once per user
```

## Omarchy (Hyprland)

Omarchy's status bar is Quickshell, which hosts SNI tray items, so the applet
shows up in the bar's tray with no extra setup.

```sh
cargo install --path .        # or copy target/release/kandang into ~/.local/bin
./scripts/install-icons.sh    # already done above if you followed Build
kandang doctor
kandang applet &              # or add it to Hyprland autostart (below)
```

To launch the applet with Hyprland, add to `~/.config/hypr/autostart.lua`:

```lua
o.launch_on_start("kandang applet")
```

## Commands

| Command | Effect |
|---------|--------|
| `kandang start` | Start the engine (via systemd if autostart is enabled, else a detached process) |
| `kandang stop` | Stop the engine; the port closes and the process exits |
| `kandang status` | Print `running` or `stopped` |
| `kandang open` | Start the engine if needed, then open the Dashboard |
| `kandang autostart enable` | Install a systemd user unit that starts the engine at login |
| `kandang autostart disable` | Remove the unit |
| `kandang doctor` | Check runtime dir, port, systemd, and SNI host |

## Notes

- The control socket lives at `$XDG_RUNTIME_DIR/kandang/control.sock` (mode 0600).
  The public HTTP port serves the Dashboard only, per
  [ADR-0004](docs/adr/0004-unix-socket-control-plane.md).
- Detached engine logs go to `$XDG_RUNTIME_DIR/kandang/engine.log`.
- On GNOME without the AppIndicator extension, and on bare tiling WMs without a
  tray bar, the applet has nowhere to appear; use the CLI there
  ([ADR-0002](docs/adr/0002-sni-only-no-gnome-extension.md)).
