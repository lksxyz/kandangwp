# Cloudflare sign-in is a browser session, with an optional scoped token

Publishing needs a Cloudflare credential. Kandang offers two paths and stores
neither in a site file, the browser, or a log.

**Default: browser sign-in, like `wrangler login`.** The Dashboard starts
`wrangler login --browser=false`, shows the Cloudflare authorization link in a
dialog (and the engine exposes it so the page updates on its own), and waits
until Wrangler saves the OAuth session. Wrangler stores that session in
`~/.config/.wrangler/config/default.toml` (mode 0600). Kandang detects it and
uses it for deploys; it never reads or prints the token. Disconnecting runs
`wrangler logout`, so Wrangler is signed out too.

**Optional: an API token in the desktop keyring.** For least privilege, the
Publish tab also accepts an API token scoped to Account → Cloudflare Pages →
Edit, limited to one account. It is stored through `secret-tool` in the Secret
Service (`service=kandang key=cloudflare`), never in plain text, and takes
precedence over the OAuth session when both exist.

Alternatives considered:

- **Only `wrangler login`.** Simplest, and familiar to Wrangler users, but its
  OAuth scopes include Workers, D1, Queues, and more, and the session file is a
  plain-text credential on disk (0600). It stays the default for convenience,
  not the most restrictive option.
- **Only a pasted API token.** Least privilege and keyring-backed, but the user
  asked for a Wrangler-like "open the page and approve" flow instead of
  creating and pasting a secret by hand.
- **Building a Kandang OAuth client.** Requires every user or the project to
  register a Cloudflare OAuth client, and still ends with a token on disk.
  Reusing Wrangler's client avoids that while keeping the same browser step.
- **A token field in `settings.json` or the dashboard.** Rejected: plain text
  and exposed to the browser.

The sign-in dialog has no token input on the default path. The browser link is
opened in a new tab, and the dialog polls the engine until the session appears.
Cancelling or closing the dialog kills the whole `npx` → Node process group, so
port 8976 is released; an engine shutdown does the same.

Consequences: the keyring must be unlocked to use an API token, and `secret-tool`
must be installed; without it, browser sign-in still works. Anyone who can run
as the user can publish, which matches the desktop trust level. The browser
session can do more than publish; the API token path cannot change DNS,
Workers, or account settings.
