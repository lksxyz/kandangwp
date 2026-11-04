# Kandang identity

The mark is a pen: three pickets with pointed tops and two rails, drawn in
white on the dashboard accent. It reads as a fence at a glance and as a
enclosure for sites that are kept local until published.

The identity is calm and precise: one colour, geometric construction, no
gradients or shadows. It matches the Dashboard's dense, light, single-accent
visual language (see [DESIGN.md](../../DESIGN.md)).

## Files

| File | Use |
|------|-----|
| `kandang-mark.svg` | App icon, favicon, Dashboard sidebar. 64 px canvas, mark from 24 px up |
| `kandang-mark-small.svg` | 16 px app icon only; the rails are dropped so the fence stays legible |
| `kandang-logo.svg` | Horizontal lockup: mark plus outlined "Kandang" wordmark (Noto Sans Medium, no font dependency) |
| `kandang-symbolic.svg` | Tray icon, engine running (monochrome, theme-coloured) |
| `kandang-stopped-symbolic.svg` | Tray icon, engine stopped (40% opacity) |
| `favicon.ico` | 16, 32, and 48 px favicon |
| `png/kandang-<size>.png` | 16–512 px app icon raster |
| `tray/kandang-*-symbolic-<size>.png` | 16–32 px tray rasters |

## Colour

- Accent `#2f5fd0` on white. White on accent is 5.7:1, so the mark meets AA
  at any size.
- Ink `#15181d` for the wordmark.
- The tray icons are black with transparency; the host tints them.

## Clear space and minimum size

- Clear space: one picket width (3 units on the 64-unit canvas) on all sides.
- Minimum size: 16 px for the small mark, 24 px for the full mark, 96 px wide
  for the lockup.

## Do not

- Recolour the mark or put it on a non-neutral background.
- Rotate, outline, or add effects.
- Set the wordmark in a live font; use the outlined lockup so it renders the
  same everywhere.

## Installing icons

The tray and launchers look icons up by name, so install them into the user's
icon theme once:

```sh
./scripts/install-icons.sh
kandang doctor   # checks that the icons are installed
```

The script writes to `${XDG_DATA_HOME:-~/.local/share}/icons/hicolor`. The
Dashboard embeds its favicon and sidebar mark, so it needs no install step.
