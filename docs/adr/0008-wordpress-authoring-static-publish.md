# WordPress is an authoring tool; the public site is static files

Kandang runs WordPress locally only while a site is being edited. Publishing
crawls that running local site like a visitor and writes plain HTML, CSS,
JavaScript, and media files. The output is deployed with Cloudflare Pages Direct
Upload, with no Git repository.

Because the public site never runs PHP or a database, the production-parity
reasons in ADR-0005, ADR-0006, and ADR-0007 no longer apply. Those decisions
only govern the local authoring runtime, which uses one bundled PHP version and
SQLite. Serving PHP to visitors and the choice of FrankenPHP or nginx are no
longer decided by this project.

Incremental publishing (regenerating only the changed pages and the homepage) is
part of the design, not an add-on. Full export remains available.

**Considered options:** a live hosted WordPress (rejected: it leaves a PHP and
database attack surface on the internet), exporting by rendering templates
without a crawl (rejected: harder, and it misses plugin output), and a Git
repository as the deploy target (rejected: Direct Upload needs no Git and avoids
a second system to keep in sync).

**Consequences:** plugins and features that need a server (forms, search,
comments, WooCommerce, logins) are dropped or moved to third-party services.
Direct Upload projects cannot switch to Git later, and Wrangler uploads are
limited to 20,000 files and 25 MiB per file.
