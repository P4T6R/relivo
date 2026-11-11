# Docker deployment: Relivo

Product names: **Relivo Console** for administration and **Relivo Agent** for
the installed application. Release `2026-11-11.2` contains `relivo/*` images.
The Compose project and private configuration paths retain their existing
identifiers to preserve volumes and credentials. See `docs/branding.md`.

**For the target VPS use [the existing-Nginx procedure](nginx/README.md).**
Bundle `2026-11-11.3` adds this integration while retaining application image
tag `2026-11-11.2`. The standalone procedure below applies only to a host with
free public web ports. Do not run it unchanged on this VPS.

Target: the owner's Ubuntu VPS at `72.62.178.54`. The owner explicitly authorized
application of the prepared deployment. Bundle `2026-11-11.3` is now installed
behind the existing Nginx with HTTPS; see the
[deployment record](../docs/enterprise/deployment-2026-11-11.md).

Confirmed domain: **relivo-desk.duckdns.org**. Public endpoints are
`https://relivo-desk.duckdns.org` and
`wss://relivo-desk.duckdns.org/ws/relay`.

Public checks on 2026-11-11 resolved the A record to `72.62.178.54`, with no AAAA
answer from the checked resolver. HTTP returned `404` with an
`nginx/1.24.0 (Ubuntu)` server header. HTTPS failed hostname verification
(`curl: (60) SSL: no alternative certificate subject name matches target host name`).
Subsequent authorized SSH inspection confirmed that host Nginx owns both public
web ports and serves other applications. Ubuntu 24.04.3, Docker 29.2.0 and
Compose 5.0.2 are installed, with about 3.5 GiB RAM available and 40 GiB free disk.
The target-specific Compose override publishes HTTP only on 127.0.0.1:18083;
host Nginx handles TLS. Local acceptance and public deployment checks passed.
The dedicated Relivo certificate is installed, and its simulated renewal passed.
Existing Nginx sites and their configuration were preserved.

This release contains the management API, web console, modified rendezvous and
relay servers, and HTTPS proxy. The modified native client still requires a full
build and a two-endpoint desktop acceptance test. Backend protocol tests do not
prove desktop capture, input control or endpoint consent behavior.

## Components and exposure

| Service | Purpose | Persistent volume |
| --- | --- | --- |
| `edge` | Caddy HTTPS, WSS forwarding and shared network namespace | `tls_data`, `tls_config` |
| `management` | Console, authenticated API, policy and activity database | `management_data` |
| `hbbs` | Device registration and managed rendezvous | `peer_data` |
| `hbbr` | Authorized relay pairing and transport | `peer_data` |

All processes run as UID/GID 10001 with a read-only root filesystem. Only Caddy
receives `NET_BIND_SERVICE`. Secrets are mounted from a private host directory;
they are absent from images. Default memory limits total 1,728 MiB; these are
limits, not a measured workload capacity estimate. Relay bandwidth and session
capacity must be measured with the actual endpoints.

`management`, `hbbs` and `hbbr` use `network_mode: service:edge`. This keeps the
policy API on literal loopback, as required by the server authorization code.
Docker documents this mode in its
[Compose service reference](https://docs.docker.com/reference/compose-file/services/#network_mode).
All four services therefore share a network trust boundary. Replace them as a
whole stack during upgrades; do not recreate only the namespace-owning proxy.

| Public port | Protocol | Purpose |
| --- | --- | --- |
| 80 | TCP | HTTPS redirect and certificate validation |
| 443 | TCP | Console, API, managed rendezvous and relay over TLS |
| 21115 | TCP | NAT test |
| 21116 | TCP and UDP | Device registration/heartbeat |

Ports 21114 and 21117–21120 are never published. Private authorization routes
are not exposed through the public router. Managed relay traffic uses `/ws/id`
and `/ws/relay`. The original server update check is disabled in this stack.

## Before deploying

The selected DNS name is `relivo-desk.duckdns.org`; its A record was verified
against `72.62.178.54`. Recheck before deployment. Do not add an AAAA record
unless this deployment is configured and reachable over IPv6.
Use a direct DNS record for the Relivo host. Confirm that the VPS has free
ports 80, 443, 21115 and 21116; adapt the architecture before proceeding if
another reverse proxy already owns them.

The owner can run this read-only preflight from the source checkout:

```sh
ssh root@72.62.178.54 'bash -s' < deployment/preflight.sh
```

To let the assistant reuse an authenticated connection, the owner can instead
open a dedicated local SSH master and enter the password in their terminal:

```sh
ssh -M -S /tmp/relivo-deploy-control-20261111 -o ControlPersist=4h -o ServerAliveInterval=30 -o ServerAliveCountMax=3 -fN root@72.62.178.54
ssh -S /tmp/relivo-deploy-control-20261111 -O check root@72.62.178.54
```

The second command must report `Master running`. A `ControlSocket ... already
exists, disabling multiplexing` message means the new connection cannot be
reused through that socket. Do not delete another project's socket or stop its
SSH process to resolve this; use a fresh, dedicated path. The previously supplied
`/tmp/sycad-deploy-control-15` had no local listener at inspection time.

Check the reported Ubuntu version, available RAM/disk, `x86_64` architecture,
Docker Engine, Compose plugin and occupied ports. The exported images are
`linux/amd64`; other architectures require a rebuild. If Docker is absent,
follow the [official Ubuntu installation guide](https://docs.docker.com/engine/install/ubuntu/).
Do not remove an existing Docker installation or disrupt other services blindly.

Allow only the published ports needed above through the provider firewall and
the host's Docker-aware firewall policy. Docker-published ports can bypass UFW
rules; see the installation guide's firewall limitations. Preserve SSH access.
Caddy's public certificate flow requires correct DNS, reachable ports 80/443
and persistent writable certificate storage, as described in
[Automatic HTTPS](https://caddyserver.com/docs/automatic-https).

## Transfer and start the tested release

These commands are for the owner. The release archive includes images,
corresponding backend source, checksums and this runbook. It contains no account,
production secret or database. Keep the source available to users of the
modified AGPL software and preserve its license notices.

From the local checkout:

```sh
scp artifacts/relivo-2026-11-11.2.tar.gz root@72.62.178.54:/root/
ssh root@72.62.178.54
```

On the VPS, after the preflight succeeds and the existing proxy is accounted for:

```sh
set -euo pipefail
umask 077
mkdir -p /opt
tar -xzf /root/relivo-2026-11-11.2.tar.gz -C /opt
cd /opt/relivo-2026-11-11.2
sha256sum -c SHA256SUMS
docker load -i images.tar
control_domain=relivo-desk.duckdns.org
python3 deployment/prepare.py --domain "$control_domain" --tag 2026-11-11.2 --output /opt/rustdesk-control-config/.env
control_compose() {
  docker compose --env-file /opt/rustdesk-control-config/.env -f deployment/compose.yaml "$@"
}
control_compose config --quiet
control_compose --profile setup run --rm -T bootstrap < /opt/rustdesk-control-config/runtime/admin-password
control_compose up -d --no-build --wait --wait-timeout 180
control_compose ps
curl --fail --show-error "https://$control_domain/healthz"
```

Stop at the first failed command and inspect that failure before proceeding.
`up --wait` checks process health; the external `curl` also checks the actual
public certificate and routing. Do not use `curl -k` to mask certificate errors.

The login is `admin`; retrieve the password privately from
`/opt/rustdesk-control-config/runtime/admin-password`, then store it in the
owner's password manager. Bootstrap deliberately fails once any user exists.
Do not rerun it on an existing database. No default password is baked in.
The private runtime directory is mode 0700. The internal key file is 0444
inside it so Docker's explicitly mounted non-root process can read the file;
other host users cannot traverse the parent directory.

Export only the public server identity for endpoint configuration:

```sh
control_compose cp hbbs:/data/id_ed25519.pub ./server-public-key.txt
```

Use the endpoint configuration contract in `management/README.md`. This backend
requires the modified managed native client; the earlier official binary
preview does not validate those modifications. Finish native build and
two-endpoint acceptance before using the system for real support sessions.

## Back up before upgrades

Keep the configuration directory outside versioned releases. Volume names use
the stable Compose project name `rustdesk-control`, independent of the release
directory. Never run `down --volumes` on production data.

For a consistent snapshot, schedule a short outage and stop the entire stack.
The maintenance container has read-only access to the volumes and no network.
Run from the installed release, with `control_compose` defined as above:

```sh
umask 077
control_backup="/opt/rustdesk-control-backups/$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$control_backup"
control_compose stop
control_compose --profile maintenance run --rm -T snapshot > "$control_backup/volumes.tar.gz"
tar -tzf "$control_backup/volumes.tar.gz" > "$control_backup/contents.txt"
cp -a /opt/rustdesk-control-config "$control_backup/config"
cp manifest.json "$control_backup/manifest.json"
control_compose up -d --no-build --wait --wait-timeout 180
```

Verify the backup command succeeded and the archive includes the SQLite
database, peer private/public keys and TLS storage. Keep a protected copy off
the VPS. Choose retention and periodically exercise restoration before relying
on the backup. No destructive retention job is installed by this release.

For recovery, use a separate, empty Compose project and restore the four archive
directories to its corresponding volumes while all services are stopped. Use
UID/GID 10001, restore the matching private configuration, and start the exact
image versions from the backup manifest. Verify HTTPS, login, inventory, audit,
server identity and an authorized endpoint session before switching traffic.
Do not overlay an archive onto a live SQLite database or merge two server keys.

For upgrades, first make and verify the snapshot, load the new images, then
update `IMAGE_TAG` in the stable `.env`. Run `control_compose down` without `-v`,
switch to the new release's Compose file, and recreate the whole stack with
`up -d --no-build --wait`. Keep the old images and source. Rollback across a
schema change requires the matching backup and image version; an image-only
downgrade is not assumed to be safe.

## Rebuild and verify locally

The image tags are explicit; base images use immutable digests. Rust builds use
one job to bound build pressure. Runtime packages are resolved by Debian at
build time, so rebuilding later is not claimed to be bit-for-bit reproducible.
The server's SQLx macros validate against an empty schema created inside the
build stage from `compile-schema.sql`; no workstation database or `.env` is
copied into the build. Keep that schema aligned with server database changes.

```sh
python3 deployment/prepare.py --test
docker compose --env-file deployment/.env -f deployment/compose.yaml build
node deployment/test-stack.mjs 2026-11-11.2
python3 deployment/package-release.py --tag 2026-11-11.2
```

The test creates a random `rustdesk-test-*` project, binds only localhost test
ports, explicitly trusts its local CA in the test process, and checks actual
HTTPS/WSS protocol behavior, denied/replayed/revoked tickets, API outage denial,
snapshot creation and persistence after whole-stack recreation. It removes
only its own disposable containers and volumes. It does not touch the existing
preview database or the VPS. Node 22 or newer with built-in WebSocket is needed.

The packager refuses to overwrite an existing release directory. If preparing
changed application images, choose a new tag, build and test that tag, then
package it. For deployment-only changes, use `--release` to version the bundle
separately from `--tag`, and test the new configuration with the retained images.
`SHA256SUMS` checks file integrity; it is not a third-party signature or audit.
