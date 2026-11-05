# Replace Wrangler with Cloudflare OAuth and the REST API

Status: superseded by ADR 0016. Kept as history.

Publishing shelled out to `npx wrangler@3.114.17` for four things: OAuth sign-in
and sign-out, listing Pages projects (parsed from debug output), creating a
project, and deploying a directory. That needed Node.js at runtime, pinned an
unrelated CLI, and parsed stdout.

**Decision.** Kandang no longer runs Wrangler.

- **Sign-in:** Connect Cloudflare opens the browser to
  `https://dash.cloudflare.com/oauth2/auth` with Kandang's own OAuth client
  (`src/oauth.rs`, Authorization Code + PKCE S256, no client secret). Cloudflare
  redirects to `http://localhost:8976/oauth/callback`, where the engine
  verifies `state`, exchanges the code at `/oauth2/token`, and stores the access
  token and refresh token in the keyring. The access token is refreshed
  shortly before it expires.
- **Client:** created once by the Kandang maintainer under **Manage Account →
  OAuth clients**. Its client ID is a public identifier and lives in the source.
  The client is private: only members of the owning account can authorize it.
  Authorizing from other accounts requires making it public, which needs domain
  verification and cannot be undone.
- **Publishing:** the Cloudflare plugin (ADR 0015) calls the Pages REST API
  directly: list and create projects, then the Direct Upload flow used by Wrangler
  (upload token, hash check, bulk upload, upsert hashes, multipart deployment).
  The host injects the access token into requests to `api.cloudflare.com`; the
  plugin never reads it.

Users who signed in with the Wrangler session must connect again once.

Alternatives considered:

- **Keep `wrangler login`.** Requires Node.js, and the Wrangler session is a
  plain-text credential shared with the user's other tools.
- **Reuse Wrangler's client ID.** Rejected: it identifies Wrangler, not Kandang,
  and Cloudflare can change it without notice.
- **Paste an API token.** Still supported as a manual path in ADR 0011 history, but
  the browser redirect was the requested flow.
- **Dynamic client registration.** Not available: `/oauth2/register` and
  `/client/v4/oauth/register` return 404.

Open items, verified in code but not yet against a live account:

- The `scope` parameter is omitted, so Cloudflare grants the scopes registered on
  the client. The client needs account read and Cloudflare Pages edit. Dashboard
  scope names are listed in the API token permissions reference; the OAuth scope
  IDs are dot-delimited (`account.read`), and the Pages ID is still to be read
  from `GET /oauth/scopes`.
- The token and authorize endpoints are the ones Wrangler uses. They are assumed
  to apply to third-party clients too; the first live sign-in confirms this.

Consequences: no Node.js dependency. The Direct Upload protocol is now our
responsibility; it follows Wrangler 3.114's implementation, so a Cloudflare
change needs a plugin update. Multi-account tokens use the first account the
token can see.
