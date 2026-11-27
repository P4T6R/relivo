# Relivo deployment record — 2026-11-27 upgrade

The owner asked for the production server to be redeployed from branch
`test/windows-fixes` (commits edd1f15, eea8597, 6d8cb30). The upgrade was applied
through the owner's authenticated SSH master on the Ubuntu VPS 72.62.178.54,
following "Back up before upgrades" in deployment/README.md. It replaces the
installation described in [deployment-2026-11-11.md](deployment-2026-11-11.md);
DNS, host Nginx, the certificate and its renewal were not changed.

## Installed

- Release bundle: 2026-11-27.1, installed at /opt/relivo-2026-11-27.1.
- Application images: relivo/management, relivo/server, relivo/edge, each tagged
  2026-11-27.1, built from 6d8cb30. Archive and per-file checksums verified on
  the VPS before loading.
- Compose project: unchanged, rustdesk-control, using compose.yaml plus
  compose.nginx.yaml from the new release directory. The four volumes
  (management_data, peer_data, tls_data, tls_config) were kept; no `down -v`.
- Private configuration: unchanged /opt/rustdesk-control-config; only
  `IMAGE_TAG` changed from 2026-11-11.2 to 2026-11-27.1.
- Management database: upgraded in place from schema v1 to v2 at startup, with the
  automatic pre-upgrade copy `/data/management.sqlite3.pre-v2-1795811239.bak`
  inside the management volume.
- The previous release directory /opt/relivo-2026-11-11.3 and its images remain.

## Backup taken before the change

/opt/rustdesk-control-backups/20261127T202632Z-before-2026-11-27.1 (root only):
stopped-stack volume snapshot (management and peer SQLite databases, server key
pair, TLS storage), a copy of the private configuration and the previous release
manifest. It stays on the VPS; no off-host copy was made.

## Verification

| Check | Result |
| --- | --- |
| Services | edge, management, hbbs and hbbr healthy on 2026-11-27.1, no restarts |
| Server identity | hbbs public key fingerprint identical to server-public-key.txt |
| Public HTTPS | /healthz ok; HTTP redirects with 308; new console bundle served |
| Address book API | /api/ab/* returns 404 (legacy address book instead of HTTP 405) |
| WebSocket | Public WSS upgrades succeed at /ws/id and /ws/relay |
| Registration ports | TCP 21115/21116 reachable from the workstation |
| Other host projects | All other Compose projects still running, untouched |

Before release, `node deployment/test-stack.mjs 2026-11-27.1 --nginx` and the
protocol smoke test passed locally. The console login and the device inventory
were not checked after the upgrade (the administrator password is not handled by
the assistant); the v1 -> v2 upgrade message shows the existing database was used.

## Notes for the next upgrade

- `docker compose run -T snapshot` reads standard input. When the procedure is
  piped into `ssh ... 'bash -s'`, it consumes the rest of the script and the run
  stops after the snapshot with the stack stopped. Copy the script to the VPS and
  run it as a file, or give each compose command `</dev/null`.
- Rollback across this schema change needs the backup above and the 2026-11-11.2
  images together; restoring the old images alone is not supported.
- The native Windows agent paths of this branch still need a real Windows run.

## Second upgrade the same day: 2026-11-27.2

Requested by the owner after the [security review](security-review-2026-11-27.md).
Images 2026-11-27.2 were built from 16a13f2, passed
`node deployment/test-stack.mjs 2026-11-27.2 --nginx`, and were installed at
/opt/relivo-2026-11-27.2 with the same procedure (script copied to the VPS and run
with standard input closed). Schema unchanged (v2).

- Backup before the change:
  /opt/rustdesk-control-backups/20261127T224131Z-before-2026-11-27.2
  (stopped-stack snapshot, configuration copy, 2026-11-27.1 manifest).
- `IMAGE_TAG` changed from 2026-11-27.1 to 2026-11-27.2; volumes, configuration,
  DNS, Nginx and certificate unchanged. /opt/relivo-2026-11-27.1 and its images
  remain for rollback (same schema, so its images alone can be restarted).
- Verified: four services healthy with no restarts; `MANAGEMENT_REQUIRED=Y` in
  hbbs; server key fingerprint unchanged; public /healthz ok, HTTP 308 redirect,
  /api/ab/* 404, WSS upgrades on /ws/id and /ws/relay; TCP 21115/21116 reachable
  and 18083, 21114, 21117–21120 closed; other host projects still running.
- Removed the leftover /root/relivo-03-finish.sh from the first upgrade.
