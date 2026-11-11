# Modeling and acceptance: central administration V1

Status: V1 acceptance model. Test results and outstanding acceptance conditions
are recorded in implementation-status.md; this table alone does not claim a pass.

## Container lifecycle acceptance (2026-11-11)

Prepare private secrets -> build/load release images -> bootstrap once -> start
healthy services -> validate HTTPS/WSS and policy -> stop/backup before upgrade.
Container recreation must retain accounts, grants, audit and server keys. A
second bootstrap must refuse existing accounts. An unavailable management API
must block new relays. Tests use an isolated Compose project, loopback ports and
an explicitly trusted test CA; production requires valid public TLS and native
two-device acceptance. Remote commands are supplied for owner execution.

## Logical data model

| Entity | Required relationships and invariants |
| --- | --- |
| User | Unique normalized account name; password hash; role; enabled flag; session deletion on relevant account changes |
| UserSession | User, hashed opaque credential, expiry, revocation; never expose stored hashes |
| Device | RustDesk ID as stable primary key, verified UUID/public-key identity binding, approval state, last verified contact |
| Group | Unique name within the installation; administrator-managed |
| UserGroup | Unique user/group membership |
| DeviceGroup | Unique device/group membership |
| Grant | Group principal, target device group, view/connect capability; unique assignment |
| AuditEvent | UTC time, actor, action, target, result, provenance, correlation ID; redacted metadata |
| ConnectionAttempt | Authenticated requester, target, policy decision, expiry, bounded relay authorization state |

A session may connect only when the user is enabled, the session is current, the
role permits connection, the device is approved, and a matching grant exists.
Administrators have explicit global administration authority; remote access still
uses an authenticated decision and endpoint authentication. Viewers cannot gain
connect permission merely by joining a group that grants it to operators.

## State transitions

Device: discovered/pending -> identity verified and approved -> active -> disabled.
Reporting alone cannot approve a device. A disabled identity must not silently
re-enroll as a trusted device under a changed display name.

User: enabled -> disabled. Disabling invalidates active login credentials and
new connection attempts. Removing the last enabled administrator is rejected.

Connection: request -> authenticate -> authorize -> negotiate -> endpoint accepts
or rejects -> connected -> ended. Record negotiation, endpoint acceptance, and
session completion separately; an allow decision is not a successful connection.

## Required acceptance evidence

| Test | Expected observation |
| --- | --- |
| Fresh setup and restart | Owner-created admin can sign in; state survives restart |
| Login lifecycle | Wrong password denied; bounded attempts; logout and expiry invalidate sessions |
| Disabled account | Existing credentials cannot perform API operations or create new remote sessions |
| Direct API requests | Viewer/operator cannot bypass UI restrictions with crafted requests |
| Two disjoint device groups | Lists, detail reads, searches, and writes respect scope |
| Directly entered remote ID | Out-of-scope session denied by the connection path, not just hidden in UI |
| Forged device report | Another device's ID/UUID cannot overwrite inventory or policy state |
| Relay and rendezvous bypass | Missing, expired, wrong-target, replayed, and revoked authorization rejected |
| Authorization outage | New managed sessions fail closed with an understandable client error |
| Native client compatibility | Real checked-in client version can sign in and use the implemented inventory flows |
| Managed endpoint routing | Direct IP/LAN and alternate-server routes cannot bypass the claimed policy boundary |
| Audit | Allowed/denied decisions and admin writes are attributable without secrets; client reports labeled |
| Persistence recovery | Consistent backup restores accounts, groups, grants, audit, and identity bindings |
| Browser workflows | Empty/loading/error states work; keyboard navigation and narrow screens remain usable |

Unit and integration tests alone do not prove native remote-control behavior.
The final authorization tests require two authorized endpoint environments and
real connection attempts. Record untested operating systems and connection modes.

## Completion boundary

The research/specification package is complete when its links and source claims
are checked. V1 itself is complete only after implementation and the applicable
acceptance tests above pass. Neither documentation nor an interface preview
constitutes implementation of the requested enterprise features.

## Naming acceptance (2026-11-11)

Verify the browser title, login and authenticated console branding; rebuild the web assets; run the Docker acceptance against the new image names, including storage persistence and connection authorization. Check native display metadata and review Windows title routing together. Native compilation and endpoint acceptance remain pending; an unchanged prebuilt upstream preview is not evidence of the new native name.

## Existing-proxy deployment states (2026-11-11)

Inspected -> local Nginx/Compose acceptance -> owner authorizes or executes writes
-> release loaded -> private configuration and initial administrator created
-> stack healthy -> ACME-only host installed -> certificate issued -> HTTPS host
validated and reloaded -> public TLS/health/login verified. On failure stop at
the failed phase; restore only the Relivo virtual host, retaining all data.
A successful backend pilot is not two-device desktop acceptance.
