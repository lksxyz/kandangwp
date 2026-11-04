# One bundled PHP version for all sites

Kandang ships exactly one tested PHP version inside the AppImage, and every Site
runs on it. Per-site PHP versions were considered and rejected for the MVP: they
need downloads, verification, and a cache, which adds moving parts to a tool
meant to stay lightweight. Choosing one version keeps the AppImage self-contained
and makes the PHP every Site runs on a single tested target.

Consequence: a future reader will wonder why sites can't pick a PHP version. That
is a deliberate MVP limit, and revisiting it means revisiting this decision.
