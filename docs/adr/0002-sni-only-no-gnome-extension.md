# Standard SNI only — no custom GNOME Shell extension

Kandang integrates with the desktop status area exclusively through the
freedesktop **StatusNotifierItem/AppIndicator** protocol (plus its companion
DBusMenu). Every DE/WM with a native tray host — Plasma, XFCE 4.16+, MATE 1.18+,
LXQt, Budgie, and Cinnamon via `xapp-sn-watcher` — shows it with no extra code,
on both X11 and Wayland (the protocol is pure D-Bus). GNOME has no native SNI
host; there we rely on already-installed general tray support (Ubuntu
preinstalls the AppIndicator extension) and document that installation, rather
than building and maintaining a Kandang-specific GNOME Shell extension.
Trayless setups (vanilla GNOME, bare Hyprland/i3/dwm) are first-class users of
the CLI instead.

**Considered options:** a bespoke GNOME Shell extension for true
quick-settings-style integration on GNOME (rejected: perpetual maintenance
chase against GNOME Shell version churn, for a minority exercise; the
AppIndicator path already delivers a working tray item).

**Consequences:** on GNOME the app reads as "a tray
icon" rather than a native volume/wifi-style control; Wayland tiling users
without a tray-capable bar (e.g. Hyprland without waybar's tray module) see
nothing and use `kandang` CLI commands instead — that is the supported path,
not a failure state.
