# Relivo Console

The Rust management API and React web administration interface for Relivo.
Relivo Agent is the native application, derived from the locally owned RustDesk fork.
The browser manages devices, people, groups, access rules, and activity. Remote
screens continue to use the native client.

## Local console

```sh
bash scripts/start-management.sh
```

Run from the workspace root. The command builds both components, initializes a
**new local database only**, and listens at `http://127.0.0.1:21114`. It generates
a local administrator named `admin`; the random password is in
`.local/management/admin-password`. Credentials and data are ignored by Git.
The internal API uses `127.0.0.1:21120` and a separate secret file.

This does not start the ID/relay servers, install a service, or deploy to a VPS.
An empty fleet is intentional: no demonstration devices are mixed with real data.

Dependencies: a supported Rust toolchain, Node 22.12+ or an appropriate newer
release, a C compiler for bundled SQLite, and npm. `scripts/cargo-local.sh` also
recognizes this workspace's locally extracted toolchain/development packages.

## Implemented behavior

- Argon2 password hashes, expiring/revocable browser and native sessions,
  HttpOnly cookies, same-origin/CSRF checks, and bounded sign-in attempts.
- Administrator, operator, and viewer roles. Administrator-only account changes
  and last-administrator protection. Role/password/disable changes revoke login
  sessions.
- Device enrollment credentials shown once; UUID/public-key pinning on the
  first authenticated report; manual approval and credential rotation.
- Groups containing people/devices; explicit view or connect grants between
  groups. A connect grant includes visibility; viewers cannot connect.
- Scoped inventory in both web and native APIs. Pagination and search.
- Persistent administration, login, authorization, relay-pairing, and supported
  client-report events. Client reports are identified as reports; a paired relay
  is not evidence that desktop login succeeded.
- Short-lived relay tickets, bound to a device, attempt, role, and credential.
  Each of the two roles can be admitted once. Permission and credential state are
  checked again at admission and before pairing.

The legacy native address book is a read-only view of central inventory, limited
to the first 100 visible devices; the paginated native group/device API and the
console should be used for larger fleets. Editing it in the native client returns
an explicit error. SSO, LDAP, 2FA, session recording, browser remote control, and
standalone file/terminal/tunnel sessions are outside V1.

## Managed connection mode

V1 uses the relay for every managed session. This deliberately trades P2P
bandwidth savings for a single enforceable connection route. Two one-time tickets
admit the authenticated operator and the approved receiving device. Standard
endpoint consent/password authentication and peer encryption still apply.

This requires **building the modified native client** on both computers and
building the modified hbbs/hbbr. The official binary used for the earlier UI
preview does not contain this implementation. Ordinary upstream clients are
rejected by a managed server; they cannot silently fall back to unrestricted
P2P. Existing unmanaged mode remains available when management is not configured.

Client configuration in the active `RustDesk2.toml` `[options]` section:

```toml
management-required = 'Y'
management-device-token = 'ENROLLMENT_CREDENTIAL_FOR_THIS_DEVICE'
api-server = 'https://relivo-desk.duckdns.org'
custom-rendezvous-server = 'relivo-desk.duckdns.org:21116'
relay-server = 'wss://relivo-desk.duckdns.org/ws/relay'
key = 'YOUR_SERVER_PUBLIC_KEY'
allow-websocket = 'N'
direct-server = 'N'
```

Keep the global WebSocket setting off: the receiving device uses the existing
UDP registration path. Managed controllers explicitly use validated WSS for
authorization and relay regardless of that global setting. On a controller-only
machine, omit the device credential and sign in with a workspace account. A
receiving machine needs its own enrollment credential and approval. Do not copy
credentials between devices.

Restart the relevant client/service after configuring it. The first device
report pins its UUID and public key; verify the public key on the actual device
before approving it in the console. Changing those identity fields requires
credential rotation and re-approval. Direct-IP/LAN/alternate-server incoming
paths are rejected by the modified client while managed mode is enabled.

The operating-system administrator can still change configuration or replace the
binary. This is an endpoint administration boundary, not tamper resistance.
Revocation blocks new sessions; V1 does not forcibly terminate existing sessions.

## Production preparation (owner-run)

1. Build and test the API, console, server, and native client. See the acceptance
   status in `../docs/enterprise/implementation-status.md` before deployment.
2. Set a real domain with valid HTTPS/WSS certificates. Use the example Caddy
   configuration with your actual host name. Keep backend listeners private.
3. Create a private data directory and generate an internal secret of at least
   32 random bytes. Set the variables shown in `.env.example`; protect secret
   files and databases with restrictive permissions.
4. Bootstrap once, using stdin rather than a command-line password:
   `rustdesk-management init-admin admin < private-password-file`.
5. Run management, hbbs, and hbbr as supervised, unprivileged services. The two
   servers must use the same management URL/key. Run hbbr with `--bind 127.0.0.1`.
   Managed hbbs automatically binds its WebSocket listener to loopback; its UDP
   registration listener must remain reachable by devices.
6. Expose HTTPS/WSS 443 and the required registration/NAT TCP 21115–21116 and UDP
   21116. Keep 21114, 21117–21120 private. These are planned defaults, not a
   firewall change performed by the assistant.
7. Run the two-device allowed/denied acceptance scenario before inviting users.
   Test revocation and service failure as well as successful connections.

For initial operation, retain activity records and budget disk space accordingly.
No automatic destructive audit retention job is enabled. Set an explicit
retention policy before production; archiving/removal is an owner-run database
maintenance operation. Administrative logs may contain user names and device IDs.

## Backup and recovery

Stop management and both connection services before a simple filesystem backup.
Copy the complete management data directory (including any SQLite WAL/SHM files),
the hbbs peer database and server key pair, the internal secret, and service
configuration. Keep backups private. Restore all corresponding files while the
services are stopped, preserve permissions, then test login, grants, device
identity, and a denied connection before reopening access.

Schema version 1 initializes only an empty database. Unknown/populated schemas
are refused, not overwritten or automatically migrated. No existing production
database has been changed by this implementation.

## Verification commands

```sh
bash scripts/cargo-local.sh test --locked --manifest-path management/Cargo.toml
npm --prefix management/web ci
npm --prefix management/web run build
npm --prefix management/web audit --omit=dev
```

All six capability families are implemented; the status document distinguishes
automated API tests, server integration checks, browser checks, and real native
desktop validation. Do not equate API tests with a proven remote desktop session.

New source in this directory is provided under AGPL-3.0-only, matching the
project's existing open-source foundation. Original RustDesk notices remain in
their source trees. The upstream license text is in `../rustdesk-server/LICENSE`.
