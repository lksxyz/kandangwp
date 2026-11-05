# Cloudflare publishing through the Wrangler CLI

Status: superseded by ADR 0017.

ADR 0014 replaced Wrangler with Kandang's own OAuth client and the Pages REST
API. That client is private: only members of the owning Cloudflare account can
authorize it, so Connect Cloudflare fails for everyone else. Making it public
needs domain verification and cannot be undone.

**Decision.**

- Sign-in runs `wrangler login`. Wrangler opens the browser, uses its own public
  OAuth client, listens on localhost:8976, and stores its session. Kandang never
  reads or stores the token. Disconnect runs `wrangler logout`.
- Connected means `wrangler whoami --json` exits zero. It exits non-zero when
  signed out.
- The Cloudflare plugin is a WASM component. It reaches Cloudflare only through
  the `kandang:publisher/wrangler.run` import. The host runs `wrangler` with a
  fixed allowlist:
  - `whoami --json`
  - `pages project list --json`
  - `pages deploy /site --project-name <slug>`, where the host replaces `/site`
    with the exported site directory.
  The host runs no shell and rejects every other argument list.
- The publish URL is the shortest `https://*.pages.dev` in `pages deploy` stdout.
- Node.js and `wrangler` must be on `PATH`. `kandang doctor` reports the sign-in
  state.
- Site admin passwords stay in the desktop keyring. The Cloudflare keyring
  entries from ADR 0011 and 0014 are no longer read or written.

**Alternatives considered.**

- **Make the OAuth client public.** Keeps the REST API, but needs a verified
  domain and is irreversible.
- **Reuse Wrangler's OAuth client over REST.** Same flow, but Kandang would hold
  Wrangler's client ID and scopes (Workers, D1, Queues), which ADR 0011 rejected.
- **Plugin in TypeScript (componentize-js).** Blocked. componentize-js 0.23 with
  jco 1.41 does not resolve the custom WIT import `kandang:publisher/wrangler` at
  wizer initialization, even with the import marked external in the Rolldown
  config. The plugin is written in Rust (`wit-bindgen`) for now.

**Consequences.**

- Wrangler and Node.js become runtime requirements again; ADR 0014 removed them.
- The plugin contract is `kandang:publisher@0.2.0`. It has no `http` import and
  no `/site` preopen.
- Unverified until a live sign-in: the JSON shape of `whoami --json` (the account
  id path `accounts[0].id`) and of `pages project list --json`. The deploy URL
  parse is also unverified.
