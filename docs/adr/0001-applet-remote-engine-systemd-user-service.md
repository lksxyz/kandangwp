# Control Applet is a remote control; the Engine is a systemd user service

Kandang is split into (1) a headless **Engine** daemon that does all
the work and (2) a disposable **Control Applet** that merely shows state in the
desktop status area and sends Start/Stop. The Engine is registered as a
`systemd --user` service (with a direct-process fallback for non-systemd
distros), so the applet can crash, be closed, or be absent entirely without any
local WordPress site stopping.

**Considered options:** applet-as-owner (one process; quitting the applet kills
sites — rejected: accidental exits destroy running sessions), applet
supervising a child daemon (rejected: keeps the daemon's life tied to the
applet's).

**Consequences:** two binaries instead of one; state (Running/Stopped) must be
observable by the applet through the control plane, not inferred from its own
lifecycle; the AppImage installer therefore installs a user systemd unit rather
than launching a foreground app.
