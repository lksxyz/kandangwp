# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users
Freelancers and small agencies who build WordPress sites for clients and publish them as fast, secure static sites. They work on their own Linux desktop and switch between building in WordPress and checking results often.

## Product Purpose
Kandang runs WordPress locally for authoring only. When a site is ready, it exports static files and publishes them, so the public site has no PHP, database, or WordPress login to attack. Success means a client's site can be edited locally and published in minutes, with nothing exposed on the internet except static files.

## Positioning
A local-first authoring tool that publishes WordPress as plain static files, with a browser dashboard and a status-area applet on Linux rather than a bloated desktop app.

## Operating Context
Used from a Linux desktop, mostly as a background tool. The dashboard is open when managing sites, creating a site, publishing, reading logs, or moving a site to the trash. Publishing goes to Cloudflare Pages.

## Capabilities and Constraints
- Create, start, stop, preview, publish, and trash local WordPress sites (SQLite, bundled PHP).
- Static export with incremental page reuse; deploy only when something changed.
- Dashboard at localhost, sites at `*.localhost` through a local gateway.
- Terminology: Site, Domain, Export, Publish, Public Address, Trash (see GLOSSARY.md).
- Undecided: pricing and hosting targets beyond Cloudflare Pages.

## Product Principles
- Local authoring, public static output: the public site never runs server code.
- Nothing is deleted without a second confirmation.
- Publishing is explicit and reviewable before anything reaches the internet.
- The desktop surface stays small: one applet, one browser dashboard.

## Accessibility & Inclusion
WCAG 2.2 AA for the dashboard: keyboard operation, visible focus, labelled controls, announced status changes, and sufficient contrast.
