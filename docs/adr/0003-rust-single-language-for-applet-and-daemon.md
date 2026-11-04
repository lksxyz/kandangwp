# Rust, one language for Control Applet and Engine daemon

Both the **Control Applet** and the headless **Engine** are written in Rust,
shipped as one binary with subcommands. The applet uses the pure D-Bus SNI
crate `ksni`; the Engine is a small std-only service. The point is footprint,
not language purity.

**Considered options:** Tauri (rejected: a webview is redundant when the
Dashboard is deliberately browser-served), Electron (~200MB runtime; rejected
outright against the "very lightweight" goal), a single Go binary (rejected:
we still wanted native-feeling, framework-free status-area behavior across
Wayland-native hosts).

**Consequences:** all components, the AppImage payloads, and the systemd unit
files share a build toolchain; contribution requires Rust regardless of which
piece a contributor touches.
