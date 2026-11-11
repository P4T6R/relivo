# Relivo on the existing Ubuntu Nginx host

This is the target-specific procedure for root@72.62.178.54 and
relivo-desk.duckdns.org. The owner explicitly authorized this deployment on
2026-11-11, and it is now installed with HTTPS. See the
[deployment record](../../docs/enterprise/deployment-2026-11-11.md).
The fresh-install commands below are retained for reference; do not rerun
bootstrap or the initial-install checks against the existing deployment.

## Verified preflight, 2026-11-11

Ubuntu 24.04.3 LTS, x86_64, Docker 29.2.0, Compose 5.0.2, 7.8 GiB total RAM,
about 3.5 GiB available, 40 GiB free disk. Host Nginx 1.24 already owns 80/443
and serves multiple sites. Certbot is installed. No Relivo virtual host or
certificate exists. TCP 18083/21115/21116 and UDP 21116 are unoccupied.
These observations are a snapshot; recheck before applying.

Bundle 2026-11-11.3 contains the unchanged, tested application images tagged
2026-11-11.2 plus the Nginx integration. It does not include a native Agent
binary. Real desktop support still requires native builds and two-device tests.

## Network layout

Public HTTPS/WSS -> host Nginx -> 127.0.0.1:18083 -> Caddy HTTP port 8088 ->
existing private management/ID/relay listeners. Apply compose.nginx.yaml after
compose.yaml on every operation. Its !override replaces the public web port
bindings; an ordinary YAML list would append them and conflict with Nginx.
See [Compose merge semantics](https://docs.docker.com/reference/compose-file/merge/).

Only TCP 21115/21116 and UDP 21116 are directly published by Docker. Keep ports
21114 and 21117–21120 private. The HTTP ingress is bound to host loopback, and
Nginx overwrites client forwarding headers. Nginx explicitly forwards the
WebSocket upgrade headers and increases the idle timeout to one hour, following
its [WebSocket documentation](https://nginx.org/en/docs/http/websocket.html).

## Transfer and start a fresh installation

Run locally, then open the existing SSH master:

```sh
scp -o ControlPath=/tmp/relivo-deploy-control-20261111 \
  artifacts/relivo-2026-11-11.3.tar.gz \
  artifacts/relivo-2026-11-11.3.tar.gz.sha256 root@72.62.178.54:/root/
ssh -S /tmp/relivo-deploy-control-20261111 root@72.62.178.54
```

On the VPS, first verify that no Relivo installation or occupied target port
has appeared. Do not overwrite existing configuration, accounts or volumes.
The following commands initialize a new SQLite database and administrator:

```sh
set -euo pipefail
umask 077
cd /root
sha256sum -c relivo-2026-11-11.3.tar.gz.sha256
test ! -e /opt/relivo-2026-11-11.3
test ! -e /opt/rustdesk-control-config
test -z "$(docker volume ls --format '{{.Name}}' --filter name=rustdesk-control_)"
mkdir -p /opt
tar -xzf relivo-2026-11-11.3.tar.gz -C /opt
cd /opt/relivo-2026-11-11.3
sha256sum -c SHA256SUMS
docker load -i images.tar
python3 deployment/prepare.py --domain relivo-desk.duckdns.org \
  --tag 2026-11-11.2 --output /opt/rustdesk-control-config/.env
control_compose() {
  docker compose --env-file /opt/rustdesk-control-config/.env \
    -f deployment/compose.yaml -f deployment/compose.nginx.yaml "$@"
}
control_compose config --quiet
control_compose --profile setup run --rm -T bootstrap \
  < /opt/rustdesk-control-config/runtime/admin-password
control_compose up -d --no-build --wait --wait-timeout 180
curl --fail --show-error http://127.0.0.1:18083/healthz
```

Stop at a failed step. Bootstrap refuses any existing user database. The admin
password stays in the private runtime file; never paste it into shared output.
Do not use the workstation's .env: its runtime paths are workstation-specific.

## Add only the Relivo virtual host

The initial file serves certificate challenges without requiring a certificate
that has not yet been issued. Both filenames below must be unused:

```sh
test ! -e /etc/nginx/sites-available/relivo-desk.duckdns.org
test ! -L /etc/nginx/sites-available/relivo-desk.duckdns.org
test ! -e /etc/nginx/sites-enabled/relivo-desk.duckdns.org
test ! -L /etc/nginx/sites-enabled/relivo-desk.duckdns.org
install -d -m 0755 /var/www/relivo-acme
install -m 0644 deployment/nginx/relivo-acme.conf \
  /etc/nginx/sites-available/relivo-desk.duckdns.org
ln -s /etc/nginx/sites-available/relivo-desk.duckdns.org \
  /etc/nginx/sites-enabled/relivo-desk.duckdns.org
nginx -t
systemctl reload nginx
certbot certonly --webroot -w /var/www/relivo-acme \
  --cert-name relivo-desk.duckdns.org -d relivo-desk.duckdns.org
install -m 0644 deployment/nginx/relivo.conf \
  /etc/nginx/sites-available/relivo-desk.duckdns.org
if ! nginx -t; then
  install -m 0644 deployment/nginx/relivo-acme.conf \
    /etc/nginx/sites-available/relivo-desk.duckdns.org
  exit 1
fi
systemctl reload nginx
install -d -m 0755 /etc/letsencrypt/renewal-hooks/deploy
test ! -e /etc/letsencrypt/renewal-hooks/deploy/relivo-nginx
install -m 0755 deployment/nginx/renew-relivo.sh \
  /etc/letsencrypt/renewal-hooks/deploy/relivo-nginx
certbot renew --cert-name relivo-desk.duckdns.org --dry-run --run-deploy-hooks
systemctl is-active certbot.timer
```

Certbot can use the existing ACME account; handle any account prompt in the
owner's terminal. The webroot plugin does not rewrite unrelated Nginx sites.
The deploy hook validates and reloads Nginx only after this certificate renews.
Verify the host's existing scheduled renewal mechanism if certbot.timer is
inactive; do not create duplicate schedules. See the official
[Certbot renewal guide](https://eff-certbot.readthedocs.io/en/stable/using.html#renewing-certificates).

Confirm any Hostinger firewall allows TCP 80/443/21115/21116 and UDP 21116.
Docker-published ports may bypass UFW; the host's existing UFW policy alone does
not establish their reachability or filtering. Preserve existing SSH rules and
other applications. Do not expose additional internal ports.

## Verify, back up and roll back

From a separate machine, without bypassing certificate checks:

```sh
curl --fail --show-error https://relivo-desk.duckdns.org/healthz
curl --head http://relivo-desk.duckdns.org/
```

Expect healthy JSON and an HTTPS redirect. Open the console, sign in with the
privately retrieved administrator password, and verify its empty initial fleet.
Export the public server identity with:

```sh
control_compose cp hbbs:/data/id_ed25519.pub ./server-public-key.txt
```

Never export the private key to endpoints. Verify a real managed endpoint
session after the native build.

Use the base runbook's stopped-stack backup procedure with the two-file
control_compose function above. Also back up the Relivo virtual-host file,
webroot configuration, renewal hook and /etc/letsencrypt securely; TLS state
now lives on the host, not in the Docker TLS volume. A restoration exercise is
still required before treating those backups as a proven recovery process.

If first-install routing fails, disable only
/etc/nginx/sites-enabled/relivo-desk.duckdns.org, validate with nginx -t, and
reload Nginx. Stop the Relivo stack using control_compose stop if needed.
Retain its volumes, runtime credentials, certificate and release for diagnosis.
Never stop the shared Nginx or remove unrelated sites to free ports.

## Local acceptance

```sh
node deployment/test-stack.mjs 2026-11-11.2 --nginx
python3 deployment/package-release.py --tag 2026-11-11.2 --release 2026-11-11.3
```

The test uses an isolated Nginx 1.24 container as a stand-in for the host proxy,
a one-day local test certificate trusted explicitly by the test process,
localhost-only ports, and disposable Relivo volumes. It exercises the actual
HTTPS/WSS login and permission protocol, API outage denial, snapshot creation,
and recreation with preserved accounts, devices, audit and server identity.
It cannot validate public DNS, ACME issuance, provider filtering or native GUI
behavior; those remain deployment/endpoint checks.
