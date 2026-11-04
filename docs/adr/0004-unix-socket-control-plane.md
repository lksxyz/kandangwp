# Unix-socket control plane; the public HTTP port serves the Dashboard only

The Applet and CLI command the Engine over a locked Unix domain socket under
the user's runtime directory (`$XDG_RUNTIME_DIR`, mode 0600). The Engine's
fixed public `localhost:PORT` HTTP surface serves **only** the Dashboard —
start/stop and future mutating control verbs never ride it.

Why not just expose `POST /api/stop` on the public port: browsers can be made
to send unintended requests to TCP `localhost` from ordinary web pages (form
posts and fetch attempts against the user's own machine, a documented local
abuse pattern), whereas a 0600 Unix socket is untouchable from a browser
context. Engine stop/start must never be reachable by anything the user didn't
run on purpose.

**Considered options:** same-port HTTP control API (rejected: one surface, but
protection depends on bolt-on CSRF tokens for exactly the verbs that must be
intrinsically safe), D-Bus control (rejected: extra machinery in Rust and a
GNOME-leaned mechanism for a DE-agnostic product).

**Consequences:** the socket owns the authoritative Running/Stopped state
(`status` and `stop` are socket commands; "start" launches the Engine process
through systemd or as a detached process, then waits for the socket to answer);
Dashboard pages that need to display or trigger engine state go through the
socket-backed control channel rather than the public port; the fixed port is
therefore a display/serving concern only — a collision there degrades the
Dashboard without ever making the desktop controls ambiguous.
