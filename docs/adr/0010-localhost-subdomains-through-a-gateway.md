# Sites use `<name>.localhost` through a local gateway, not `.test`

Each Site is reached at `http://<name>.localhost:18080`. The engine runs a small
gateway on port 18080 that routes each request by its hostname to that Site's
own server. Chromium and Firefox resolve `*.localhost` to loopback without any
configuration, and so does systemd-resolved on the target machines, so no root
access or DNS setup is needed.

The `.test` domain was the first choice, but it needs a DNS server that answers
for `.test`, which means root access and a systemd-resolved or dnsmasq setup on
every machine. Kandang keeps `.test` as a possible later opt-in and doesn't
require it.

Considered options: binding port 80 for clean URLs (rejected: needs root or a
capability grant on every upgrade), `.test` through systemd-resolved (rejected
for the default: root setup on each machine), and per-site ports only (rejected:
WordPress links break whenever a port changes).

Consequence: WordPress links are written as `http://<name>.localhost:18080`, so
changing the gateway port later means re-running the site's URL update. Exporting
connects to the Site's own port with the gateway hostname as the `Host` header.
That keeps WordPress from redirecting, and the CLI can export without the engine
running.
