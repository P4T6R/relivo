# SRS: central administration V1

Status: approved V1 scope. Read after the PRD; see implementation-status.md for
implemented behavior, test evidence, and remaining acceptance work.

## Container requirements (2026-11-11)

- Build this fork with locked dependencies; never substitute upstream binaries.
- Publish only HTTPS/HTTP and the registration/NAT ports. Keep management,
  authorization and relay listeners private and retain their loopback checks.
- Persist management data, peer identity and TLS state in separate named volumes.
- Use non-root processes, read-only roots, bounded logs and resources, readiness
  checks, private runtime secrets, and explicit administrator bootstrap.
- Verify HTTPS/WSS relay admission, denied/replayed access, service outage,
  recreation persistence and the configured public port surface in local Docker.

## Functional requirements

| ID | Requirement |
| --- | --- |
| F01 | Authenticate console users; support logout, expiry, account disabling, and session invalidation |
| F02 | Create/update users, assign admin/operator/viewer roles, and prevent loss of the last enabled administrator |
| F03 | Show approved devices, pending enrollments, administrator-assigned name, reported platform, last verified contact, and group assignments |
| F04 | Create groups and manage user/device membership with server-side validation |
| F05 | Grant view/connect access by group; deny by default and check both role capability and target scope |
| F06 | Enforce connect decisions in rendezvous and relay paths; console filtering is insufficient |
| F07 | Display login, administration, authorization, and supported connection events with time, actor, target, outcome, and source |
| F08 | Support the client account/device API subset required for this version, with fixtures from the checked-in client |
| F09 | Present accurate empty, loading, permission-denied, disconnected, and error states |
| F10 | Preserve licenses, provenance, and the root repository's absence of active upstream remotes |

## Security and reliability requirements

- Enforce permissions on every API operation, including detail reads, searches,
  exports, and writes. A caller cannot grant capabilities beyond its authority.
- Use a maintained password-hashing implementation, bounded login attempts,
  expiring sessions, CSRF protection for cookie-authenticated writes, and explicit
  proxy trust. Store no default administrator password in source.
- Treat device reports as untrusted until bound to approved device identity.
  An ID/UUID supplied in a request must not itself authorize enrollment or writes.
- Do not expose RustDesk database files or management service credentials to the
  browser. Keep internal authorization interfaces inaccessible from the public
  network and authenticate server-to-server requests.
- When central authorization is required, fail closed if its decision cannot be
  obtained. Reject expired, revoked, altered, or wrong-target credentials.
- State device freshness explicitly; an HTTP heartbeat is not proof of an active
  desktop session. Audit events must distinguish server observations from client
  reports. Logs are not claimed to be tamper-proof against the VPS administrator.
- Never record bearer tokens, private keys, or unattended-access passwords in
  activity records. Bound query sizes and define retention before deployment.
- Restarting the services must preserve users, groups, grants, and audit records.
  Document consistent backup/restore and test it before production use.

## Managed endpoint boundary

Central restrictions apply to enrolled, configured endpoints and the specified
server routes. Direct IP/LAN access and alternate rendezvous servers must be
disabled or brought under the same authorization mechanism before claiming
fleet-wide enforcement. A person controlling the endpoint OS can reconfigure it;
the console does not change that trust boundary.

Revoking a grant must prevent new sessions. Existing P2P sessions do not pass
through the relay; immediate termination requires verified endpoint support.
The interface must not claim an existing session was disconnected without
acknowledgement. Active-session termination is not an acceptance claim for V1.

## Branding requirements (2026-11-11)

Use Relivo Console on login, navigation, browser title and console documentation. Use Relivo Agent on native titles, launcher labels and application display metadata. Deliver Relivo-tagged Docker images and a new release archive. Preserve existing accounts, storage identifiers, protocol routes and upstream license/provenance notices.

## Shared VPS proxy requirements (2026-11-11)

- Preserve existing Nginx sites and public listeners. Add only the Relivo host.
- Terminate public TLS in host Nginx with a dedicated Certbot webroot certificate.
- Publish the container HTTP ingress only on host loopback 127.0.0.1:18083;
  continue publishing TCP 21115/21116 and UDP 21116 for device registration.
- Retain private authorization listeners and test both WebSocket upgrades through
  Nginx, secure login cookies, ticket denial and persistence after recreation.
- Preserve ACME renewal and validate Nginx before reloading it.
