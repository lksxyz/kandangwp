# FrankenPHP serves PHP instead of nginx with php-fpm

Each Site is served by FrankenPHP, a single bundled binary that runs PHP itself.
The alternative was nginx with php-fpm, the standard production setup, which
means two processes to start, configure, and supervise for every Site. Caddy with
php-fpm was also considered and has the same two-process cost.

FrankenPHP keeps the process count to one per Site and fits Kandang's
single-bundled-PHP decision. The trade-off is that the web-server and PHP stack
differs from the common nginx setup, so behaviour that depends on nginx-specific
configuration will not carry over directly.
