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

**Overview ledger.** The site Overview is one card split by a full-height hairline into two ledgers, Authoring (site address, admin sign-in) and Publishing (public address, deployed address, project). Each row is a 132px muted label column and a 14px value column, hairline between rows; addresses and the admin user are mono in ink. A missing value stays a muted phrase, and where the fix lives on another tab it carries one accent text action that switches to that tab. Site status appears once, in the page head, never repeated in the card.

**Publish flow.** The review step is a wide shadow-only dialog. It exports the site first, then shows an embedded preview of that export above the ledger — changed files, project, destination. The preview is never shown as an address. With nothing changed it says so and hides the primary action rather than disabling it, so the only way out is Cancel; with changes the primary action is **Deploy** (or **Write zip** without Cloudflare), and confirming sends the export that was reviewed.

The strip above the content carries the run: three named phases while it works, then one of two settled states. Done is a green `Published` badge beside the change count (`4 of 12 files changed`, or `Everything was already up to date`) with **Open live site** and Dismiss; failure swaps in the error text with **View log** and Dismiss. Both settled states are terminal — nothing auto-advances. The strip hides after 20 seconds, and is not shown again on a return more than two minutes later; the durable record lives in the Publish tab's **Last publish** block (deployed address) and in the Overview's Publishing ledger.

**Preline-informed components** (ported manually to CSS tokens, no Tailwind):
- Tabs: segmented bar on a gray-50 track (1px hairline, 10px radius, 3px inset) with one 34px chip per section; the selected chip is white with a hairline and a deep-blue label, hover is gray-soft. Danger zone is red throughout — red label, red-soft hover, red-soft selected chip with a red hairline.
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

- 2026-11-05: The Publish tab's project field is now a disabled value that follows the site name (slug); the "Save settings" button is gone, and the destination is always derived from the project, so changing projects can never leave a stale pages.dev URL behind.
- 2026-11-05: The new-site dialog asks for a project name with capitals, digits, and spaces, shows the derived site name under the field, and creates the slug (`Project Pemda 01` → `project-pemda-01`).
- 2026-11-05: The project name field accepts capitals, digits, and spaces; Kandang slugifies it (`Pemda Project 1` → `pemda-project-1`) and shows the generated Cloudflare name under the field.
- 2026-11-05: Tab strip rebuilt from bare underlines into a segmented bar so the four site sections read as tabs at a glance; Danger zone carries red text, red hover, and a red-soft selected chip. Phones drop the tab icons to keep all four visible.
- 2026-11-05: Publish and review start the site when it is stopped and leave it running, so the authoring window never disappears mid-publish; the review dialog says so.
- 2026-11-07: Publish review exports first and shows a preview of the export before Deploy. Preview addresses removed from the sites list, the Overview, and Last publish.
- 2026-11-05: Publish flow redesigned end to end: the review dialog became a ledger with a state-aware primary action, the progress strip gained settled states with **Open live site** / **View log** and an explicit Dismiss, and the Publish tab gained a **Last publish** block. Dashboard publishes now write the deploy manifest, so the deployed address and the changed-file count are real afterwards.
- 2026-11-05: Site Overview rebuilt as a two-door ledger: one card split by a hairline into Authoring and Publishing, label/value rows replacing the auto-fit fact tiles. Dropped the Status row there (the page head badge already carries it), gave the missing preview and public address a text action into the Publish tab, and switched `dl.facts` to the same ledger on the Engine page.
- 2026-11-05: Type scale stepped down to the sm/base set: 14px body, 12px meta, 24px h1; dropped the 13px meta step and the 30px h1.
- 2026-11-05: Dashboard restyled on Preline patterns (manual port, no Tailwind): white sidebar with soft-primary active nav and badge counts, borderless page heads, rounded-md status badges, solid empty states, ringed inputs, shadow-only dialogs.
- 2026-11-04: Buttons rebuilt on the six-variant recipe (Solid/Outline/Ghost/Soft/White/Link) with a `.danger` tone and Hugeicons icons; retired `primary`, `danger`, and `danger-solid` classes.
- 2025-11-04: Sinyal Modern committed; Buku Log amber world retired after user rejection. Hover links raised to #1158c4 to clear the 4.5:1 detector advisory.
- 2025-11-04: Type scale tightened (15px body, 30px h1, 40px controls, 32px row buttons) after user feedback that the dashboard read oversized; overview facts grouped into Site / Publishing, trash rows fixed to keep destructive actions visible.
