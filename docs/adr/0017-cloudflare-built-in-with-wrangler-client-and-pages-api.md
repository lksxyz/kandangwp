# Cloudflare is built in: browser OAuth and the Pages REST API

Status: accepted. Supersedes ADR 0016 and the plugin host in ADR 0015.

ADR 0016 published through the `wrangler` CLI inside a WASM plugin. That needed
Node.js and Wrangler on `PATH`, a plugin artifact built from a sibling repository,
and wasmtime in the host binary. The plugin was not installed on any machine, so
Cloudflare publishing could not run.

**Decision.**

- Cloudflare is part of Kandang. There is no plugin, no WASM host, and no
  `wrangler` process. Wasmtime and the `kandangwp-plugin` contract are removed.
- Sign-in is OAuth 2.0 authorization code with PKCE, in the browser, the same flow
  as `wrangler login`:
  - Client: Wrangler's public client `54d11594-84e4-41aa-b438-e81b8fa78ee7`.
  - Scopes: `account:read pages:write offline_access`.
  - Redirect: `http://localhost:8976/oauth/callback`, listening on 127.0.0.1. The
    state is checked and the code is exchanged at `dash.cloudflare.com/oauth2/token`.
- The session (access token and refresh token) is one JSON item in the desktop
  keyring (`service=kandang key=cloudflare-session`). It is never written to a file.
  The access token is refreshed 60 seconds before it expires. A refresh the server
  rejects clears the session. Disconnect revokes the refresh token at
  `dash.cloudflare.com/oauth2/revoke`, then clears the keyring item.
- Publishing uses the Pages REST API and the Direct Upload flow. It follows
  Wrangler 3.114.17's calls:
  1. list projects, or create the project with production branch `main`;
  2. `upload-token`;
  3. `pages/assets/check-missing` with the upload JWT;
  4. `pages/assets/upload` in batches of at most 40 MiB and 2000 files;
  5. `pages/assets/upsert-hashes`;
  6. a multipart deployment with a `manifest` field and a `branch` field.
- A file's hash is BLAKE3 over its base64 contents plus its extension, truncated
  to 32 hex characters. The golden value is checked against an independent BLAKE3
  implementation.
- Connected means a session exists in the keyring. Publish uses Cloudflare when
  connected and writes a zip otherwise.

**Alternatives considered.**

- **Kandang's own OAuth client (ADR 0014).** It is private to the owning account,
  so anyone else's sign-in fails. Making it public needs domain verification and
  cannot be undone.
- **Keep `wrangler` as a subprocess.** Node.js stays a runtime dependency, the CLI
  output is parsed, and the sign-in still ends with a plain-text session file.
- **Keep the WASM plugin.** The host still embeds wasmtime and the plugin is a
  second build artifact. Nothing in the plugin needs isolation from the host.

**Consequences.**

- Kandang uses an identity that belongs to Wrangler. ADR 0011 and ADR 0014 rejected
  this. Cloudflare can change or revoke the client, and the consent screen may name
  Wrangler.
- Sign-in needs `secret-tool` (libsecret), because the session lives in the keyring.
- Unverified until a live sign-in: whether this client accepts the reduced scope
  set; the shapes of the `accounts` and Pages responses, which were matched to
  Wrangler's code by reading and not by a call; and the upload JWT's lifetime. A
  401 in the middle of a long upload is not retried with a new JWT.
- The API token entry from ADR 0011 (`service=kandang key=cloudflare`) is no longer
  read. It stays in the keyring until removed with `secret-tool clear`.
