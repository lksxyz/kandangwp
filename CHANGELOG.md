# Changelog

All notable changes to Kandang. Versions follow `Cargo.toml`.

## Unreleased

### Fixed
- Publishing a site for the first time no longer fails with "nothing exported
  for <site> yet". The export creates the public folder itself.

### Changed
- Publish shows a preview of the export in the review dialog before anything
  is deployed. Deploy (or Write zip, without Cloudflare) sends the export you
  reviewed, without exporting again.
- Preview addresses are gone from the sites list, the Overview, and the Publish
  tab. `kandang site preview` re-exports first, and stops the site again if it
  had to start it.

## 0.1.1 — 2026-11-06

### Added
- Runtime setup page. The engine no longer downloads anything on start. The
  dashboard shows a "Download runtime" button; the download starts only when
  you press it, with a live percentage and the file being fetched.
- Dashboard actions (new site, start, publish, password reset, restore, purge)
  stay disabled until the runtime is ready.
- `kandang runtime install` and `kandang runtime status` for installing the
  runtime from the terminal. Install prints per-file progress and refuses while
  the engine is running, so the dashboard and the CLI never write the same files.
- Byte-level download progress for FrankenPHP, WordPress, WP-CLI, and the
  SQLite plugin. Each file is still checked against its pinned SHA-256.
- `scripts/build-appimage.sh` builds the AppImage reproducibly.
- Release workflow: a `v*` tag builds the AppImage and attaches it to the
  GitHub release.
- ADR 0018: runtime artifacts are downloaded at provisioning, not bundled.

### Changed
- FrankenPHP runtime is upstream v1.13.4 packed with UPX 5.0.2, hosted in this
  repository's releases. About 52 MB instead of ~175 MB; a PHP process starts
  in about 0.5 s instead of about 70 ms.
- `kandang site create` and the dashboard refuse to create a site until the
  runtime is installed, instead of downloading silently in the middle.
- ADR 0005 is superseded by ADR 0018.

### Fixed
- Dashboard no longer offers actions that would fail without the runtime.
- Cloudflare Pages wording: the Cloudflare-side name is "Pages project"; the
  site's readable name stays "Project name".
- Research doc link keys resolve; `xapp-sn` points at the watcher source.
- Removed the stale Node.js requirement from the README.

## 0.1.0 — 2026-11-05

Initial public release: local WordPress authoring, static export, and
publishing to Cloudflare Pages or a zip file, with a browser dashboard and a
tray applet on Linux.
