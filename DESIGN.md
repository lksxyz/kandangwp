# DESIGN.md — Kandang dashboard

<!-- impeccable:design-schema 1 -->

## Platform

web — dashboard served by the Rust daemon (`src/dashboard/`); self-hosted OpenRunde woff2; no build step.

## Users

Freelancers and small agencies building WordPress sites locally on a Linux desktop; some middle-aged or older eyes.

## Design World

**Direction.** Sinyal Modern (user-pinned after rejecting Buku Log / Stempel Amber as "kurang modern", 2025-11-04) — a modern professional tool: near-white ground, white surfaces, one electric-blue accent, dense and calm.

**Color.** Blue #1570ef is the single accent: primary buttons, active nav, focus rings, links. Green #067647 carries success/live signals only. Red #d92d20 is destructive. No second accent.

| Token | Value | Carries |
|---|---|---|
| `--bg` | `#fafbfc` | Page background |
| `--surface` | `#ffffff` | Cards, panels, sidebar |
| `--ink` | `#101828` | Text |
| `--muted` | `#475467` | Secondary text (≥4.5:1 on white) |
| `--accent` | `#1570ef` | `.btn.primary`, active nav, focus, selection |
| `--accent-hover` | `#1158c4` | Hover text color (AA on #f8faff) |
| `--green` | `#067647` | Running dot, success text |
| `--red` | `#d92d20` | Destructive only |
| `--line` | `#e4e7ec` | 1px hairlines |
| `--radius-card` | `12px` | Cards, dialogs |

Contrast: white on #1570ef ≈ 4.7:1 (AA); #1158c4 on #f8faff ≈ 5.9:1.

**Type.** OpenRunde 400/600/700 as the only UI face. 14px body (`text-sm`) and 40px controls; compact in-row actions 32px. Scale: 12 micro (`text-xs`) / 14 body / 15 h3 / 18 h2 / 24 h1. System mono only for addresses, figures, and ports.

**Structure.** Fixed sidebar on white with soft-primary active nav item and pill badge counts, engine status foot. Content: h1 + primary action top-right, white panels radius 12px with hairline borders — border or shadow, never both. Statusbar at the page foot.

**Preline-informed components** (ported manually to CSS tokens, no Tailwind):
- Tabs: underline strip, 14px labels, 16px Hugeicons, active accent text + 2px underline.
- Breadcrumb: `ol` with chevron separators, 14px, current page semibold.
- Badges (`.status`): rounded-md, 12px/500, 6px dot.
- Inputs: 1px border, 8px radius, focus border accent + 4px ring.
- Empty states: solid hairline card with 44px circular primary mark.
- Dialogs: borderless, 12px radius, shadow only, 24px padding.
- Sidebar nav: 38px rows, gray-soft hover, soft-primary active item.

**Signature move.** The running order — one dense list where a state change restyles a row in place and stamps its time; the grid never reflows. Running = green dot; Stopped/Trashed = bare muted text, no mark.

**Buttons.** One base (40px, 16px inline padding, 8px gap, 14px/500, radius 8; `.sm` 32px for rows) and exactly six variants:

| Variant | Role |
|---|---|
| `solid` | The single primary action on a screen |
| `outline` | Neutral secondary; destructive rows with `.danger` |
| `ghost` | Tertiary inline action (refresh, reveal) |
| `soft` | In-place selected/toggle state |
| `white` | Dialog cancel, row actions |
| `link` | Inline navigation-style action |

Destructive actions keep one of the six variants and add the `.danger` tone: `solid danger` only for irreversible confirms, `outline danger` for reversible or row-level actions. Icons come from Hugeicons (24px grid, 1.5 stroke, rendered 16px) and only where they speed scanning; text-only otherwise.

**Refusals.** No hollow state marks, no scrollbar track on tablists, no gradient text, no floating ghost cards (border+shadow together), no kicker/eyebrow labels, no second accent.

## History

- 2026-11-05: Type scale stepped down to the sm/base set: 14px body, 12px meta, 24px h1; dropped the 13px meta step and the 30px h1.
- 2026-11-05: Dashboard restyled on Preline patterns (manual port, no Tailwind): white sidebar with soft-primary active nav and badge counts, borderless page heads, rounded-md status badges, solid empty states, ringed inputs, shadow-only dialogs.
- 2026-11-04: Buttons rebuilt on the six-variant recipe (Solid/Outline/Ghost/Soft/White/Link) with a `.danger` tone and Hugeicons icons; retired `primary`, `danger`, and `danger-solid` classes.
- 2025-11-04: Sinyal Modern committed; Buku Log amber world retired after user rejection. Hover links raised to #1158c4 to clear the 4.5:1 detector advisory.
- 2025-11-04: Type scale tightened (15px body, 30px h1, 40px controls, 32px row buttons) after user feedback that the dashboard read oversized; overview facts grouped into Site / Publishing, trash rows fixed to keep destructive actions visible.
