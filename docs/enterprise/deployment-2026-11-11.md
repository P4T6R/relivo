# Relivo deployment record — 2026-11-11

The owner explicitly authorized applying the prepared deployment, starting the
containers, initializing the administrator, and activating the domain with HTTPS.
This authorization supersedes the earlier owner-only restriction for these
operations. Deployment was completed through the owner's authenticated SSH
master on Ubuntu VPS 72.62.178.54.

## Installed

- Public console: https://relivo-desk.duckdns.org/
- Release bundle: 2026-11-11.3, installed at /opt/relivo-2026-11-11.3.
- Application images: relivo/management, relivo/server, relivo/edge, each tagged
  2026-11-11.2. Archive and per-file checksums verified before startup.
- Compose project: rustdesk-control, using compose.yaml plus compose.nginx.yaml.
- Four healthy services: edge, management, hbbs and hbbr. All run as UID/GID
  10001 with read-only roots and dropped capabilities; no restarts at verification.
- Private configuration: /opt/rustdesk-control-config/.env and its runtime/
  directory. Administrator username: admin. The generated password remains in
  runtime/admin-password and in the owner's ignored, private local handover file
  .local/relivo-production/admin-password. It is not embedded in this record.
- Public server identity: /opt/rustdesk-control-config/server-public-key.txt;
  a private local handover copy is in .local/relivo-production/.
- Host Nginx routes the new domain to 127.0.0.1:18083. The existing 20 Nginx
  configuration files and original enabled-site symlinks were unchanged.
- Pre-change Nginx backup: /root/relivo-deploy-20261111/nginx-before.tar.gz.

## Verification

| Check | Result |
| --- | --- |
| Public certificate | Valid trusted Let's Encrypt certificate for the exact hostname, initially expiring 2027-02-09 |
| HTTP | Redirects to HTTPS with status 308 |
| Health and assets | Public health endpoint reports ok; Relivo Console HTML and JavaScript load |
| Browser | Public login page renders as Relivo Console in the in-app browser |
| Administrator | Login succeeds through public HTTPS; cookie is Secure, HttpOnly and SameSite=Strict |
| Initial data | One enabled administrator; zero devices, groups and grants; activity records available |
| Access boundaries | Unauthenticated API rejected; private authorization routes unavailable publicly; incorrect CSRF rejected; logout invalidates the session |
| WebSocket | Trusted public WSS upgrades succeed at /ws/id and /ws/relay |
| Registration | TCP 21115/21116 reachable and UDP 21116 returns the expected registration challenge to a nonexistent probe identity; no device created |
| Private ports | TCP 18083, 21114 and 21117–21120 unreachable from the external workstation |
| Renewal | Certbot timer active; scoped simulated renewal and deploy hooks succeed |

The existing Nginx owns public 80/443; Docker publishes only the required device
ports and the loopback HTTP ingress. No firewall rules were changed. Reachability
was tested from the workstation; this is not an inventory of provider firewall
rules or a capacity/bandwidth test.

## Remaining acceptance and operations

The console and backend are deployed. The modified native Relivo Agent has not
been fully compiled, and no real two-machine desktop session has been tested.
Public WSS upgrades do not prove desktop capture, input, consent or peer encryption.
The stronger allowed/denied/replayed/revoked relay protocol checks ran locally
before deployment, not against production with synthetic users or an API outage.

Follow deployment/nginx/README.md for maintenance. Back up the stopped Docker
volumes and private configuration, plus the host's certificate and renewal files,
before upgrades. A production data backup/restore exercise and off-host backup
schedule have not been performed or installed by this deployment.

The release archive remains the immutable pre-deployment bundle. This record
describes the resulting deployment without rewriting its manifest or checksums.
