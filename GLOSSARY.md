# Glossary — Kandang

Canonical vocabulary for **Kandang** (depot/holding pens for local WordPress
sites). Terms are domain-level only — no
implementation details. Implementation choices live in `docs/adr/`.

---

## Control Applet

The desktop affordance for the product: a small, always-available control that
lives in the desktop environment's **status area** (wherever volume, network,
and brightness controls live for that DE/WM). Parity of feeling: "like the wifi
toggle".

It offers exactly: **turn the Engine on/off** and **open the Dashboard**.

The DE's native mechanism varies (some call it a system tray, some use a
quick-settings panel, tiling WMs show it inside a status bar). "Tray icon" is a
colloquial word for it on tray-capable DEs, not the canonical term.

**Not** the Engine. It can disappear without the Engine stopping.

## Engine

The headless local service that does all the work: serves the Dashboard and
every WordPress-development capability on `localhost`. It has no visible UI of
its own other than the Dashboard in a browser.

Has exactly two user-visible states: **Running** and **Stopped**.

## Dashboard

The browser-based UI served by the Engine. It is the product's entire visual
surface; there is no native (widget-based) settings or management UI. Opened
from the Control Applet or by typing the local URL.

## Site

A Site has a **Project Name**, the readable name entered at creation (e.g.
`Project Pemda 01`), and a name slug derived from it that names its directory
and Domain (e.g. `project-pemda-01`). The Dashboard breadcrumb and page titles
show the Project Name; the slug identifies the Site to the engine. A Site also
has its own files and database.

A Site is either **Running** (reachable at its Domain) or **Stopped**.

## Domain

The address a Site is reached at in the browser, e.g. `mysite.localhost`. Domains
are on the `.localhost` top-level domain, which browsers send to the local machine
without configuration. The engine's gateway routes each Domain to its Site.

## Export

Producing a static copy of a Site's pages, assets, and media from its running
local version. Export never changes the local Site.

## Preview

The view of a Site's latest Export, shown only inside the Publish review before
anything is deployed. Preview is never a link in the Dashboard or CLI, and it
is not a Domain.

## Publish

Exporting a Site and making the result live at its Public Address.

## Pages Project

The Cloudflare Pages project a Site publishes to. Kandang creates it on the Site's first Publish and names it after the Site slug (e.g. `project-pemda-01`), so its default `*.pages.dev` address follows the Site name. It is separate from the Project Name, which is only the readable label entered at creation.

## Public Address

The address visitors use for a published Site, e.g. `https://example.com`. It
differs from the local Domain, and Publish rewrites links to it.

## Trash

Where a deleted Site goes first. A Site in the Trash can be restored until it is
purged. Deleting a Site never removes its data immediately.

## Status Area

The desktop-specific region where system-level controls (volume, network,
power) live on a given DE/WM: e.g. the system tray / notification area on
Plasma, the shell panel on GNOME, the bar's tray zone on tiling WMs. The place
where the Control Applet appears.

User base note: users on a DE/WM without a Status Area provider (no tray bar,
no extension) use the CLI instead of the Control Applet.
