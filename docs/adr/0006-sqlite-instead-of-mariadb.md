# SQLite per Site instead of a MariaDB server

Each Site stores its WordPress data in SQLite through the official SQLite
integration plugin, not in a MySQL or MariaDB server. A database server would be
the heaviest part of the AppImage and would need to run per Site or be shared
between them. SQLite needs no server, starts instantly, and keeps each Site's
data in its own folder.

This is a deliberate deviation from production hosting. Plugins that run raw
SQL specific to MySQL may behave differently here, so this trade-off must not be
"fixed" by swapping in a database server without revisiting this decision.
