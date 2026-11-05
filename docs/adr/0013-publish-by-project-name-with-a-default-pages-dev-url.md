# Publish by project name, with a default pages.dev URL

The Publish tab used to require both a public address and a project name. New
users had to invent a URL and open the Cloudflare dashboard to create the
project before the first publish.

**Decision: the project name is the only required input.** Kandang derives
`https://<project>.pages.dev` when the custom domain is empty. The first
publish creates the project through the Pages REST API when it does not exist,
then deploys (see ADR 0014).
The **Custom domain (optional)** field stays for users who add a domain in
Cloudflare themselves and want the exported links to use it.

The export rewrites links against the effective public address, so changing
the custom domain means publishing again. The stored project is unchanged.

Alternatives considered:

- **Keep the two fields mandatory.** Familiar to Wrangler users, but it asks
  the user to know a URL before the project exists and pushes them into the
  Cloudflare dashboard.
- **Create the project in a separate step.** More clicks for no benefit; the
  deploy already knows how to create it, and the first publish is the natural
  point.
- **Default the URL only in the UI.** The engine must derive the same value for
  export and deploy, so the rule lives in the backend and the UI mirrors it.

Consequences: the default URL depends only on the project name. A custom
domain needs a second publish. Signing in to Cloudflare is still required;
Kandang cannot create a project without an authenticated account.
