# Linux system tray support (as of October 2026)

Research for an app that wants to show a tray icon/menu ("tray applet") across many Linux
desktop environments and window managers.

**TL;DR:** Ship a **StatusNotifierItem / AppIndicator (SNI)** implementation. That is the only
protocol with broad, native, out-of-the-box support across virtually every mainstream DE and
bar (Plasma, XFCE 4.16+, MATE 1.18+, LXQt, Budgie, Cinnamon via the XApp bridge, swaybar,
waybar). **GNOME is the lone holdout**: it needs a third-party extension (preinstalled and
auto-enabled on Ubuntu, required everywhere else). **XEmbed** is the legacy X11-only tray and
works only as a *compatibility fallback* (bridged via `xembedsniproxy` or kept in the DE's own
legacy "notification area" applet); it is dead on compositors that have no tray host of any kind
(Hyprland, dwm by default, etc.).

---

## 1. The SNI/freedesktop protocol

The **StatusNotifierItem (SNI)** spec ("AppIndicator" / "KStatusNotifierItem") is a pure
**D-Bus** session-bus protocol with three actors: the app registers a `StatusNotifierItem`, a
`StatusNotifierWatcher` tracks all items, and any `StatusNotifierHost` (a panel/bar) renders
them. It was explicitly written **as a replacement for the legacy X11 system-tray
specification** ([spec intro][sni-intro], [design][sni-basic]). Menus ride along via
**DBusMenu**.

App-side D-Bus names to know:

- item interface: `org.kde.StatusNotifierItem`
- watcher: `org.kde.StatusNotifierWatcher` (many hosts also accept `org.freedesktop.StatusNotifierWatcher` — e.g. sway registers both, [tray.c][sway-tray]))

Because everything is D-Bus, **SNI works identically on X11 and Wayland** — the only
requirement is that *some* panel/bar in the session implements the SNI *host* side. On
Wayland-only compositors there is no tray "in the compositor"; the tray lives in the bar/panel
You choose (or nowhere).

[sni-intro]: https://www.notmart.org/misc/statusnotifieritem/ (author's copy of the spec intro; canonical: https://www.freedesktop.org/wiki/Specifications/StatusNotifierItem/ — served bot-hostile, hence the author mirror)
[sni-basic]: https://www.notmart.org/misc/statusnotifieritem/basic-design.html

## 2. Support matrix

| DE / WM | Native tray? | Protocol | Notes |
|---|---|---|---|
| **KDE Plasma** (6.x) | ✅ Yes | SNI (+ XEmbed fallback via `xembed-sni-proxy`) | System Tray applet in `plasma-workspace` is a full SNI host (`applets/systemtray/statusnotifieritemhost.cpp`). Legacy X11/XEmbed tray icons are bridged by `plasma-xembedsniproxy`. Works on X11 and Wayland. [1][plasma-sni] |
| **GNOME Shell** (45–51) | ❌ **No native support** | requires extension | No StatusNotifier code anywhere in `gnome-shell` main; legacy XEmbed tray code only compiles with XWayland (`if have_xwayland: subdir('tray')`). Needs the **"AppIndicator and KStatusNotifierItem Support"** extension (3.2 M downloads, still built for GNOME 45 → 51). [2][gnome-ext][gnome-meson][gnome-tree] |
| — Ubuntu (GNOME session) | ✅ out of the box (via extension) | SNI | Ubuntu preinstalls `gnome-shell-extension-appindicator` (on the 24.04.3 desktop image) and its own maintained fork `ubuntu-appindicators@ubuntu.com` is **auto-enabled** by the Ubuntu session mode (`enabledExtensions` in distro gnome-shell packaging). Also ships `libayatana-appindicator3`. [3][ubuntu-manifest][ubuntu-mode] |
| **XFCE** (4.16+) | ✅ Yes | SNI + XEmbed (same plugin) | `xfce4-statusnotifier-plugin` was merged into `xfce4-panel` `plugins/systray` in Aug 2020 (shipped in 4.16): `sn-*` backend = SNI, `systray-manager` = legacy XEmbed. [4][xfce-commit][xfce-tree] |
| **Cinnamon** | ✅ Yes (XEmbed native; SNI via bundled bridge) | XEmbed + XApp/SNI | Ships a legacy XEmbed "System Tray" applet and the "XApp Status Applet". AppIndicator/SNI apps appear because Mint's **xapps** ships `xapp-sn-watcher`, a daemon that claims `org.kde.StatusNotifierWatcher` and mirrors SNI items into `XAppStatusIcon`s. [5][cin-applets][xapp-sn] |
| **MATE** (1.18+) | ✅ Yes | SNI (+ legacy XEmbed applet) | mate-panel 1.18.0 (2018) added StatusNotifier support to the notification area; a separate legacy "Notification Area" (XEmbed) applet also exists. [6][mate-news] |
| **LXQt** | ✅ Yes | SNI (+ XEmbed plugin) | `lxqt-panel` builds the **"Status Notifier"** (SNI) plugin by default (`STATUSNOTIFIER_PLUGIN Yes`), and the legacy X11 **"System Tray"** XEmbed plugin is also enabled by default. [7][lxqt-cmake] |
| **Budgie** (10.8+) | ✅ Yes | SNI + DBusMenu | Current `budgie-desktop` has a tray applet speaking `org.kde.StatusNotifierItem` with full DBusMenu support (Vala: `src/panel/applets/tray/`), and its own StatusNotifierWatcher. Runs on X11 or Wayland (labwc). [8][budgie] |
| **Hyprland** | ❌ None | — | No built-in bar and no tray; the wiki recommends **waybar** (which has an SNI `tray` module) or a third-party bar (e.g. hyprpanel). [9][hypr-bars] |
| **Sway** | ⚠️ Bar-based: **swaybar has a native SNI tray** | SNI (+ XEmbed via `xembedsniproxy`) | Not compositor-integrated; the default bar implements a D-Bus tray watching both `org.freedesktop` and `org.kde` watchers (since 2017 commit "Implement Tray Icons"). Arch notes swaybar's tray is incomplete and recommends waybar. [10][sway-tray][sway-commit][arch-sway] |
| **i3** (i3bar) | ✅ Yes (bar) | **XEmbed only** | i3bar has a tray (`tray_output` config) but it's the legacy X11 `_NET_SYSTEM_TRAY`/XEmbed protocol — X11 only. [11][i3] |
| **awesome** | ✅ Yes | **XEmbed only** | `wibox.widget.systray` (XEmbed tray). awesome is X11-only ("window manager for X"). [12][awesome-readme][awesome-api] |
| **dwm** | ❌ by default; ✅ with patch | **XEmbed only** | No tray in vanilla dwm; the community `systray` patch (X11/XEmbed) is maintained for 6.x releases. [13][dwm-patch] |
| **Qtile** (WM) | ✅ X11 bar widget only | **XEmbed only** | `libqtile/widget/systray.py` implements the XEmbed tray; Wayland backend exists but the systray widget is X11-only. [14][qtile-systray][qtile-wayland] |

Sources:
[1] https://github.com/KDE/plasma-workspace/tree/master/applets/systemtray ·
    https://github.com/KDE/plasma-workspace/tree/master/xembed-sni-proxy
[2] https://extensions.gnome.org/extension/615/appindicator-support/ (EGO API `extension-info?pk=615` shows shell-version support up to GNOME 51) ·
    https://github.com/GNOME/gnome-shell main tree (no SNI code) ·
    https://github.com/GNOME/gnome-shell/blob/main/src/meson.build (`have_xwayland` guard on the legacy tray) ·
    https://wiki.archlinux.org/title/GNOME ("AppIndicators/Top bar icons": install appindicator extension)
[3] https://cdimage.ubuntu.com/ubuntu/releases/24.04.3/release/ubuntu-24.04.3-desktop-arm64.manifest ·
    https://git.launchpad.net/ubuntu/+source/gnome-shell/tree/debian/ubuntu-session-mods/ubuntu.json (h=ubuntu/devel)
[4] https://docs.xfce.org/xfce/xfce4-panel/systray ·
    https://gitlab.xfce.org/xfce/xfce4-panel/-/tree/master/plugins/systray ·
    commit "Merge xfce4-statusnotifier-plugin to the panel" (gitlab API, 2020-08-20)
[5] https://github.com/linuxmint/cinnamon/tree/master/files/usr/share/cinnamon/applets/systray@cinnamon.org ·
    https://github.com/linuxmint/cinnamon/tree/master/files/usr/share/cinnamon/applets/xapp-status@cinnamon.org
[6] https://github.com/mate-desktop/mate-panel/blob/master/NEWS ("Notification area: add StatusNotifier support", mate-panel 1.18.0)
[7] https://github.com/lxqt/lxqt-panel/blob/master/CMakeLists.txt ·
    https://github.com/lxqt/lxqt-panel/tree/master/plugin-statusnotifier
[8] https://github.com/BuddiesOfBudgie/budgie-desktop/blob/main/src/panel/applets/tray/TrayItem.vala ·
    https://github.com/BuddiesOfBudgie/budgie-desktop/blob/main/src/daemon/statusnotifier.vala
[9] https://wiki.hyprland.org/Useful-Utilities/Status-Bars/
[10] https://github.com/swaywm/sway/blob/master/swaybar/tray/tray.c ·
     https://github.com/swaywm/sway/blob/master/sway/sway-bar.5.scd (TRAY section) ·
     commit 843ad38b "Implement Tray Icons" ·
     https://wiki.archlinux.org/title/Sway ("swaybar does not fully support tray icons. Consider an alternative like waybar")
[11] https://i3wm.org/docs/userguide.html (`tray_output`)
[12] https://awesomewm.org/doc/api/classes/wibox.widget.systray.html ·
     https://github.com/awesomeWM/awesome/blob/master/docs/01-readme.md ("window manager for X")
[13] https://dwm.suckless.org/patches/systray/
[14] https://github.com/qtile/qtile/blob/master/libqtile/widget/systray.py (X11/XEmbed; `from libqtile.backend.x11 import window`) ·
     https://github.com/qtile/qtile/blob/master/docs/manual/wayland_status.rst

## 3. Wayland vs X11

- **SNI works on Wayland sessions.** It's D-Bus only; there is no X dependency and no
  compositor support needed. What matters is whether a **panel/bar implements the SNI host**
  (Plasma, Budgie, XFCE 4.16+, swaybar, waybar-tray do; GNOME via extension; i3bar/awesome/dwm
  do not).
- **XEmbed is strictly X11.** The legacy tray is an X selection on the root window
  (`_NET_SYSTEM_TRAY_Sx`) with X windows docked into the panel ([XEmbed spec][xembed]). On a
  Wayland-only compositor there is no XEmbed host at all; the only XEmbed icons that survive
  are ones bridged by desktop code running against XWayland:
  - KDE, sway and LXQt use/ship **`xembedsniproxy`** — it grabs the X tray selection inside
    XWayland and republishes XEmbed icons as SNI items ([plasma tree][plasma-sni],
    [sway commit message][sway-commit]).
  - GNOME's legacy tray (XEmbed via `na-tray-manager`) only compiles when building with
    XWayland — and the Shell UI doesn't expose it without the extension.
  - GTK3's `GtkStatusIcon` (the classic XEmbed tray icon API) is **removed in GTK4**, so
    modern native apps have nowhere to publish XEmbed icons from; they must go through
    libappindicator/libayatana-appindicator or their own SNI client.
- Practical rule for an app in 2026: **speak SNI (+ DBusMenu), optionally fall back to libayatana-appindicator, and treat XEmbed as an X11-only compatibility path.**

[xembed]: https://specifications.freedesktop.org/xembed/latest/

## 4. Getting a tray in bar-only setups

- **Hyprland / wlroots compositors:** no in-compositor tray. Start a bar with an SNI tray
  module — **waybar** `tray` module (marked "still in beta" in its man page but widely used),
  eww/AGS, or DE-like shells (hyprpanel). NetworkManager applets should run with
  `nm-applet --indicator` (SNI mode) on Wayland ([waybar tray man page][waybar-tray],
  [Hyprland wiki status-bars][hypr-bars]).
- **Sway:** `swaybar` ships a working SNI tray (configure via `tray_*` bar directives in
  `sway-bar(5)`); Arch still recommends waybar for full tray support.
- **i3 / awesome / dwm:** XEmbed-only (X11). On these, SNI items won't render natively, but a
  bridging watcher (`plasma-xembedsniproxy` run inside XWayland, or a small compatibility
  daemon) can convert SNI → XEmbed. XEmbed items render fine (unless you run them under
  a non-X server).
- **GUI toolkits:** Electron and Tauri both publish SNI/appindicator icons directly, so they
  work on all of the above without extra daemons.

[waybar-tray]: https://github.com/Alexays/Waybar/blob/master/man/waybar-tray.5.scd

## 5. Headline issues for tray apps on Wayland

- **GNOME without the extension = invisible icon.** Electron's Linux tray docs say it uses
  SNI "by default" and falls back to `GtkStatusIcon` when no SNI host exists — GtkStatusIcon
  is deprecated/removed in modern stacks, so effectively the icon just disappears on vanilla
  GNOME. On Ubuntu it works by default (extension preinstalled/enabled); outside Ubuntu,
  this is the #1 support ticket for tray apps. ([Electron docs][electron-tray])
- **Electron id-collision bug:** multiple Electron apps historically created AppIndicator
  entries with the same id ("chrome"), which made them collide/overwrite each other
  (electron issue #40936, closed; waybar even documents custom icon overrides to work around
  Electron icon bugs). ([electron/40936][electron-issue], [waybar-tray][waybar-tray])
- **Click semantics are unspecified in SNI.** Electron notes the `click` event can map to
  left-click in some environments and differently elsewhere; Tauri documents that
  **tray click events are simply unsupported on Linux** (on left-click: some hosts won't
  emit; you generally only get a right-click context menu either way).
  ([Electron docs][electron-tray], [Tauri system-tray docs][tauri-tray])
- **Tauri:** tray via the `tray-icon` crate: crate defaults to **libappindicator (GTK3)** and
  offers a **`ksni`** feature that is a pure D-Bus StatusNotifierItem backend (no GTK deps,
  better for Wayland-only setups; when both are enabled, ksni wins). Both need an SNI host
  present in the session. ([tray-icon README][trayicon])
- **ksni (Rust):** works anywhere an SNI host exists; same GNOME caveat. ([docs.rs/ksni][ksni])
- **GNOME extension quirks:** menu/tooltip fidelity varies; legacy XEmbed icons via the
  extension only work under X11/XWayland.

[electron-tray]: https://github.com/electron/electron/blob/main/docs/api/tray.md ("Platform considerations: Linux")
[electron-issue]: https://github.com/electron/electron/issues/40936
[tauri-tray]: https://v2.tauri.app/develop/system-tray/
[trayicon]: https://github.com/tauri-apps/tray-icon/blob/dev/README.md
[ksni]: https://docs.rs/ksni

## 6. Late-breaking changes (GNOME 45–51 era) — verified

- **GNOME Shell still has no built-in StatusNotifier support** as of main (checked 2026-10:
  no SNI/appindicator code in `GNOME/gnome-shell`).
- The **AppIndicator extension remains the sanctioned path** and is **actively maintained
  through GNOME 51** (EGO `shell_version_map` keys: …, 48, 49, 50, 51).
- **Ubuntu** continues to ship + auto-enable its distro fork
  (`ubuntu-appindicators@ubuntu.com`) via the Ubuntu session mode in gnome-shell packaging
  (ubuntu/devel through noble), and the desktop images install
  `gnome-shell-extension-appindicator`.
- No sway/Hyprland/etc. compositor has gained an integrated tray; tray bars remain the solution.

## Appendix: spec details worth knowing when implementing SNI

- Register an item: own `org.kde.StatusNotifierItem` at a unique bus name + path
  (`/StatusNotifierItem` by convention; LibreOffice/Ayatana use
  `/org/ayatana/NotificationItem/<id>` — hosts must scan both path conventions).
- Watcher handoff: an app first queries for an existing `org.kde.StatusNotifierWatcher`
  / `org.freedesktop.StatusNotifierWatcher`; if absent, it may itself register a watcher
  (swaybar, Budgie and `xapp-sn-watcher` all do exactly this dual-watcher dance).
- Item → menu: `GetMenu` returns a DBusMenu object path; hosts render that menu (Budgie's
  tray applet is a good minimal Vala example).
- Icon fallbacks: `IconName` → theme lookup (hosts default to `hicolor`), then
  `IconPixmap` (ARGB32 serialized); attention/palette overlays use
  `AttentionIconName`/`OverlayIconName`.

[Sway's tray.c][sway-tray] and [Budgie's statusnotifier.vala][budgie] are compact reference
implementations of the watcher/host and item/client roles respectively.
