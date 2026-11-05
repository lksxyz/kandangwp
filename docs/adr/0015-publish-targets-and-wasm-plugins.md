# Publish targets, with WASM plugins for hosts

Status: accepted for the publish-target choice. The WASM plugin host is removed by ADR 0017.

Publishing is a target choice. The default output is a zip of the exported
static site. Hosting services are plugins.

**Decision.**

- A stored Cloudflare API token makes Cloudflare the active integration.
  Without one, `publish` writes `<site>/export.zip` containing every exported
  file at its relative path (HTML, CSS, JS, images, fonts). Target selection is
  `publish::active_target()`.
- Hosting targets are WebAssembly components built to `wasm32-wasip2`. The WIT
  contract is `kandangwp-plugin/wit/publisher.wit`, package
  `kandang:publisher@0.1.0`.
- The host loads `<data dir>/plugins/<id>.wasm`, built and installed from
  `kandangwp-plugin/`. The Cloudflare plugin is the first.
- Plugin capabilities:
  - **Filesystem:** the export is preopened read-only at `/site` via WASI.
  - **Network:** only the `http.send` import. The host fixes the origin
    (`https://api.cloudflare.com` for Cloudflare, path prefix `/client/v4/`),
    rejects other methods, and injects the stored credential when the request
    has no `authorization` header. Plugins never see the credential.
  - **Memory:** capped at 1 GiB per instance.

**Why not WASI-HTTP for networking.** WASI-HTTP would let the host intercept
requests, but injecting a credential means implementing the boxed-future
`send_request` hook over wasmtime's body types, which changes across releases.
A single host import is smaller, testable, and enforces the same rules.
Filesystem stays on WASI.

**Open for later.** A third-party plugin is not sandboxed against long loops:
there is no epoch deadline yet. Add one before loading plugins that are not
shipped with Kandang. Plugin signatures are also not verified yet.

Alternatives considered:

- **In-process `Target` trait.** Simpler, but every target would ship inside the
  binary and be compiled and reviewed with it.
- **Subprocess plugins.** Would need a stable CLI protocol and per-OS builds;
  WASM gives one artifact per plugin.

Consequences: the host embeds wasmtime, which adds a sizeable build dependency.
Plugin changes need a rebuild and reinstall of the `.wasm` file; the host and
plugin contract are versioned through the WIT package version.
