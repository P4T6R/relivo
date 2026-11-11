# Central administration implementation status

Date: 2026-11-10. Scope: the six capabilities selected by the owner.

Deployment update, 2026-11-11: the owner explicitly authorized production
installation. Relivo Console and its Docker backend are live at
https://relivo-desk.duckdns.org/ with verified HTTPS, administrator login,
public WSS upgrades and registration ports. Certificate renewal simulation
passed. See [the deployment record](deployment-2026-11-11.md).
Native compilation and two-machine desktop acceptance remain outstanding.

## Delivered

| Capability | Implemented behavior |
| --- | --- |
| Web console | Responsive React interface, authenticated sessions, dashboard, searchable/paginated lists, dialogs, loading/error/empty states |
| Devices | Manual inventory entry, unique enrollment credential, authenticated identity report, pinned UUID/public key, administrator approval, disable and credential rotation |
| Users | Create/update accounts, admin/operator/viewer roles, disable, session invalidation and last-admin protection |
| Groups | Validated user/device membership, group creation/update/removal |
| Access rights | Explicit view/connect grants, scoped APIs, managed rendezvous authorization, one-use tickets for both relay legs, policy recheck before pairing |
| Activity | Persistent login/admin/authorization/relay events and authenticated, deduplicated client reports; sensitive payloads excluded |

The management service owns a separate SQLite database. Its private API is
authenticated and loopback-only. Managed connections use the relay; upstream
unmanaged behavior is kept behind the absence of management configuration.
The native client hooks reject direct-IP/LAN/alternate-server incoming paths in
managed mode and retain ordinary endpoint consent/password authentication and
peer encryption. Those native hooks have **not** yet passed a full native build
and a two-device desktop test.

## Verification performed

### Existing Nginx target integration, 2026-11-11

Authorized read-only SSH inspection confirmed the target's Docker installation,
capacity and shared host Nginx. No production services or files were changed.
The new Compose override and dedicated Nginx host passed local Docker acceptance
using the unchanged 2026-11-11.2 application images: trusted HTTPS, console
assets, secure login, real hbbs/hbbr WSS protocol, denied/replayed/revoked tickets,
API outage denial, snapshot creation and full-stack recreation with preserved
accounts, devices, audit and server identity. Evidence is in ignored
`.local/relivo-nginx-test.log` and `.local/docker-test-nSCW7Z/`.

Bundle 2026-11-11.3 adds deployment configuration only. Public certificate,
production startup and endpoint reachability checks subsequently passed during
the authorized deployment. Native build and two-endpoint desktop acceptance
remain outstanding. See [the target runbook](../../deployment/nginx/README.md).

### Application verification

| Check | Result and limits |
| --- | --- |
| Management build | Passed with the local Rust 1.99.1 toolchain and locked dependencies |
| API integration suite | 10 passed: authentication/CSRF/persistence, device identity, role/scope enforcement, relay tickets, revocation before pairing, replay/expiry, secret redaction, native JSON shapes, throttling, stopped-database backup/restore, unhealthy status when the database is unavailable |
| Management Clippy | Passed for all targets with `-D warnings` |
| Web build | TypeScript and Vite production build passed |
| Web dependency audit | `npm audit --omit=dev`: zero reported vulnerabilities at verification time |
| Server build | Modified hbbs/hbbr debug binaries compiled; existing upstream warnings remain |
| Server regression suite | 13 hbbs/library tests and 14 hbbr tests passed; executable entrypoints have no tests |
| Real-process integration | `management/tests/protocol-smoke.mjs` starts isolated local API/hbbs/hbbr processes and verifies allowed/denied rendezvous, legacy path rejection, cryptographic verification of signed device identity, token-free UDP notification, bidirectional relay payload, invalid/replayed/revoked tickets, audit, and fail-closed behavior on API shutdown |
| Browser | Login, session restoration after reload, empty fleet, create/remove a test-only empty group, activity search, populated journal-to-users navigation, role-rule form with no groups, Escape-to-close dialog, and 390px layout checked in the actual browser |
| Native source | Edited Rust files parse; a full dependency/build attempt was started but stopped during dependency preparation after confirming missing graphical prerequisites. This is **not** a successful native compile |

The browser check found a stale-row rendering error when changing between table
types. Navigation now clears the old rows in the same render as the page change;
the failing journal-to-users path and users-to-groups path were replayed
successfully after the fix.

The relay integration exchanges test payloads. It does not simulate or claim
desktop capture, input control, the peer-encryption handshake, native account UI,
or endpoint consent. These checks are implementation verification, not an
independent security review or production acceptance.

## Remaining acceptance before production

1. Build the modified native client for the actual endpoint operating systems.
   On this Linux host, Flutter and Dart are absent, and `pkg-config --modversion
   gtk+-3.0` reports that GTK development metadata is missing. Prepare the full
   platform toolchain documented by the client project. The earlier official
   binary preview cannot validate modified source.
2. Use two authorized endpoints to verify account login, scoped inventory,
   enrollment/approval, allowed and denied desktop sessions, encrypted peer
   authentication, consent/password behavior, revocation, reconnects, and direct
   route rejection. Windows/macOS/mobile behavior has not been exercised.
3. Use the confirmed domain `relivo-desk.duckdns.org`, check host resources and occupied ports, deploy
   the Docker release with public HTTPS/WSS certificates, validate the external
   firewall and choose backup retention. The owner's VPS has not been inspected
   or modified; production acceptance remains owner-run.

No production SSH, system package installation, production migration, firewall
change, service installation, deployment, or Git push was performed. Production
commands remain owner-run under the supplied MISHKAN instructions.

## Explicit V1 limits

- No SSO, LDAP, 2FA, session recording, browser-based remote desktop, or immediate
  termination of existing sessions. A grant revocation blocks new sessions.
- The legacy address book is read-only and limited to the first 100 visible
  devices; larger inventories use the paginated device/group APIs or console.
- Device names are administrator-assigned; platform and contact time come from
  authenticated device reports. A recent report is not proof of desktop access.
- There is no audit deletion endpoint or automatic destructive retention job.
- An administrator of an endpoint's OS can change its configuration or binary.

## Docker verification (2026-11-11)

Release `2026-11-11.1` builds three `linux/amd64` images from the modified source:
`rustdesk-control/management`, `rustdesk-control/server` and
`rustdesk-control/edge`. Compose runs four application services. See
[`deployment/README.md`](../../deployment/README.md) for the owner-run VPS
procedure, backup, upgrade and recovery steps.

The complete local Docker acceptance test passed with Docker Engine 29.7.2,
Compose 5.5.0 and Node 22.23.2:

- Release-mode builds of the API, web assets, hbbs and hbbr.
- All four services healthy, UID/GID 10001, read-only root filesystems,
  dropped capabilities and only the five configured port/protocol bindings.
- HTTPS console/assets/API and WSS protocol exchange with certificate
  validation using the test CA. TLS verification was never disabled.
- Internal authorization POST routes inaccessible on the public router.
- First-admin bootstrap and refusal to repeat bootstrap on an existing database.
- Signed peer identity, two-leg relay pairing, bidirectional payload, invalid
  and replayed ticket rejection, revoked access and management-outage denial.
- Read-only volume snapshot while the application was stopped.
- Complete container/namespace recreation retaining the same CA and server
  identity, administrator login, both accounts, enrolled device and relay audit.
- Secure session-cookie flag on the HTTPS login response.

Test evidence is local and ignored by Git: `.local/docker-tests.log` and
`.local/docker-test-E9pAXs/`. The disposable test project and its volumes were
removed by the test. The existing preview and other Docker projects were left
in place. Generated configuration permissions and non-destructive credential
regeneration were also checked. The API suite passes 10 tests and Clippy passes
with warnings denied.

The first container build exposed the upstream SQLx dependency on a local
`.env` and compile-time SQLite file. Docker now creates a fresh, empty schema
inside the build stage. Neither workstation databases nor secret files enter
the image context. A stale keep-alive socket in the restart test was fixed by
opening a fresh TLS connection for each check; the full scenario then passed.

Public ACME issuance, real VPS networking/load, full native compilation and an
actual two-device desktop session remain unverified. Local protocol payloads
do not establish complete remote-support readiness. No production action was
performed. The subsequently confirmed domain is `relivo-desk.duckdns.org`;
owner-run host preflight and existing proxy integration are still required.

### Confirmed VPS domain (2026-11-11)

The owner supplied `relivo-desk.duckdns.org`. A live DNS check at approximately
09:28 UTC returned `72.62.178.54`; the checked resolver returned no AAAA or CAA
answer. HTTP returned 404 with the `nginx/1.24.0 (Ubuntu)` header. HTTPS hostname
verification failed because the certificate did not cover the supplied domain.
No TLS validation bypass was used. The existing listener must be identified
through owner-run host preflight before assigning the Compose web ports.

The public origin, WSS relay and native client configuration examples now use
the selected domain. A private local preparation exists at
`.local/relivo-target/.env`; Compose validation passed without starting services.
Its local secret paths are not portable: generate private configuration on the
VPS with the documented `prepare.py` command. The tested images and historical
release archive are unchanged; domain selection is runtime configuration.

## Reproduce

### Relivo naming release (2026-11-11)

The owner approved Relivo, Relivo Console and Relivo Agent. Release
`2026-11-11.2` uses `relivo/management`, `relivo/server` and `relivo/edge` images;
the existing Compose storage namespace and native technical identifiers remain
stable. Historical `2026-11-11.1` artifacts are retained.

The production web build passed, and the browser rendered the updated Relivo
Console title and login screen. The full Docker acceptance passed again with
the new image names, including HTTPS/WSS, permission denial, outage denial,
snapshot and persistence. Evidence: `.local/relivo-docker-tests.log` and
`.local/docker-test-Wu2RTh/`. Native display metadata parses; edited Rust files
parse. A complete native build and two-device test remain outstanding.
See [branding and regression surface](../branding.md) for changed source paths.

### Local build commands

From the workspace root, with supported Rust and Node installed:

```sh
bash scripts/cargo-local.sh test --locked --manifest-path management/Cargo.toml
bash scripts/cargo-local.sh clippy --locked --manifest-path management/Cargo.toml --all-targets -- -D warnings
npm --prefix management/web ci
npm --prefix management/web run build
npm --prefix management/web audit --omit=dev
bash scripts/cargo-local.sh build --locked --manifest-path management/Cargo.toml
bash scripts/cargo-local.sh test --locked --manifest-path rustdesk-server/Cargo.toml --lib --bins
bash scripts/cargo-local.sh build --locked --manifest-path rustdesk-server/Cargo.toml --bins
node management/tests/protocol-smoke.mjs
bash scripts/start-management.sh
```

The protocol test uses loopback ports 21414, 21420, 22115–22119 and fresh private
data under `.local/protocol-test-*`. Its processes are stopped when the test
finishes. It does not use the preview console's database or real endpoints.
The local console remains at `http://127.0.0.1:21114`; account `admin`, generated
password in `.local/management/admin-password`. Never commit that directory.
