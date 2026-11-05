# Security Policy

## Reporting a vulnerability

Please report security issues privately. Do not open a public issue or pull
request.

Use GitHub's private vulnerability reporting:
<https://github.com/lksxyz/kandangwp/security/advisories/new>

Include:

- The affected version or commit.
- Steps to reproduce.
- The impact you expect, for example credential exposure or access from
  another local user.

You should get an acknowledgement within 7 days. Fixes are released as soon as
they are verified, and the advisory credits you unless you ask otherwise.

## Scope

Areas with the highest impact:

- Cloudflare sign-in tokens and the desktop keyring (`src/cloudflare.rs`,
  `src/credentials.rs`).
- The dashboard's token-protected HTTP API (`src/dashboard/`).
- The `*.localhost` gateway and the preview server (`src/gateway.rs`).
- The unix control socket (`src/control.rs`).

Kandang runs on your own machine and binds to `127.0.0.1`. Reports about the
published static output should say which site and which file are involved.
