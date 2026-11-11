#!/bin/sh
# Install as a Certbot deploy hook; reload only after Relivo's certificate renews.
set -eu
if [ "${RENEWED_LINEAGE:-}" = /etc/letsencrypt/live/relivo-desk.duckdns.org ]; then
    /usr/sbin/nginx -t
    /usr/bin/systemctl reload nginx
fi
