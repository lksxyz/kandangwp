# Dashboard actions use a per-run token on the local HTTP port

The browser Dashboard needs to start, stop, publish, trash, and purge sites, so
it can't use the Unix control socket from ADR-0004. Those actions go over the
Dashboard's HTTP port instead, which amends ADR-0004 for the Dashboard only. The
control socket still handles status and stop for the applet and CLI.

Four checks protect the mutating routes:

- **Host header:** only `localhost:10042` and `127.0.0.1:10042` are accepted,
  which blocks DNS rebinding.
- **Per-run token:** a random 256-bit token is embedded in the served page and
  required in an `X-Kandang-Token` header on every POST. A page from another
  origin can't read it, so it can't send a valid request.
- **JSON only:** POST bodies must be `application/json`. A plain cross-site form
  can't send that without a CORS preflight, which the engine doesn't allow.
- **Typed confirmation:** purging a trashed site requires its name, so a stray
  click can't delete data.

Considered options: a separate authenticated API with user accounts (rejected:
the Dashboard is single-user and local), and keeping mutating routes on the
socket with the Dashboard calling through the applet (rejected: adds a second
process to every action).

Consequence: any local process that can read the engine's memory or the served
page can act as the user. That matches the trust level of the desktop session.
