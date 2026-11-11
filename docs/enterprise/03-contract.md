# Contract: central administration V1

Status: implemented V1 boundaries. See implementation-status.md for validation
limits, especially the native desktop build and two-device acceptance.

## Container operating contract (2026-11-11)

Compose manages `edge`, `management`, `hbbs`, and `hbbr`. A separate bootstrap
profile initializes an empty management database from a password on stdin.
Runtime secrets are generated outside image layers and mounted read-only.
The public origin/domain is explicit; the target SSH address is not a TLS name.
The owner selected `relivo-desk.duckdns.org` on 2026-11-11. Use
`https://relivo-desk.duckdns.org` as the public origin and
`wss://relivo-desk.duckdns.org/ws/relay` for the relay. Existing public web
listeners must be accounted for before the dedicated-proxy Compose deployment.
Published defaults: TCP 80/443/21115/21116 and UDP 21116. No backend/API port is
published. Images carry a release tag; upgrades preserve volumes and require a
stopped, consistent backup. No implicit schema upgrades or volume deletion.

## Web application API

Use a versioned administration namespace, `/management/v1`, distinct from the
native client's existing `/api` namespace. Keep the browser and administration
API on the same origin.

| Resource | Operations | Authorization |
| --- | --- | --- |
| Session | Login, inspect, logout | Login is rate limited; other operations require a valid session |
| Users | List, create, update, disable | Administrator |
| Devices | List/detail; approve, update, disable | Scoped read; administrator writes |
| Groups | List, create, update, membership changes | Scoped read; administrator writes |
| Grants | List, add, revoke | Administrator |
| Audit | Paginated, filtered read | Administrator in V1 |

Represent device IDs as strings. Use UTC timestamps and bounded pagination.
Validation failures return structured errors; unauthenticated and unauthorized
requests do not disclose out-of-scope device details. Frontend route visibility
does not replace API authorization.

## Native client compatibility

Derive exact payloads from these checked-in sources before implementation:

- `user_model.dart` and `common/hbbs/hbbs.dart`: `/api/login`, `/api/currentUser`,
  `/api/logout`, and login-option discovery.
- `group_model.dart`: `/api/users`, `/api/peers`,
  `/api/device-group/accessible`, and pagination/visibility behavior.
- `ab_model.dart`: personal/shared and legacy address book behavior, if needed for
  the selected native-device listing. Implement only an explicitly tested subset.
- `hbbs_http/sync.rs`: device information and heartbeat payloads and responses.
- `common.rs`: `/api/audit/{type}` event ingestion.

Do not return empty success responses for unsupported functions. Preserve the
client's documented-by-code error and fallback behavior. Authentication of device
reports requires an explicit binding to enrolled device identity; the current
UUID-only request fields are insufficient.

### Observed wire shapes

These shapes come from the checked-in client, not from a running API:

- Login request fields include `username`, `password`, `type: "account"`, `id`,
  `uuid`, `autoLogin`, and `deviceInfo`. Preserve the camel-case field names.
  The current login call sends JSON without explicitly setting a JSON content
  type, so verify request parsing with that actual request before release.
- A completed login response uses `type: "access_token"`, `access_token`, and a
  `user` object. User fields include `name`, `display_name`, `status`, and
  `is_admin`. The client interprets status 0 as disabled and -1 as unverified.
- `/api/currentUser` is a POST with `id` and `uuid` and a bearer token. It expects
  the user object directly, not wrapped in `data`. A 401 resets account state.
- `/api/login-options` expects an array. With no SSO configured, return an empty
  array rather than an HTML 404 page or an object.
- Accessible users/peers use `current` (one-based), `pageSize`, `accessible`, and
  `status=1`. Their response is `{ "total": 0, "data": [] }` for an empty result.
  `total` must count only authorized results. Accessible device groups use the
  same pagination envelope.
- Peer objects use string `id`, nested `info` containing `device_name` and `os`,
  and optional `user_name`, `device_group_name`, `note`, and `status`. Do not infer
  desktop reachability solely from an inventory entry's presence.
- Heartbeat requests contain `id`, base64 `uuid`, numeric `ver`, `modified_at`,
  and optional `conns`. The current call supplies no explicit authorization
  header. Responses may contain `sysinfo`, `disconnect`, `strategy`, and
  `modified_at`; accepting this shape is not sufficient to trust its sender.

Authentication failures should supply an `error` field in a JSON response with
the appropriate non-success status. The native login client recognizes that
field and treats malformed bodies as failures. Test both native and browser
authentication independently; their cookie/bearer contracts are different.

## Connection authorization

`POST /authorize` on the private listener accepts `token`, `target`, pinned
`public_key`, `conn_type` (0 for desktop), and `attempt`. It returns `allowed`
and `attempt` on success; denied requests return HTTP 403 with a structured
error. The attempt UUID correlates authorization, relay admission, and relay
events. Shared server public keys are not user credentials.

V1 rechecks current database policy on every decision and never caches positive
decisions, so it does not issue a policy revision. The public authenticated
`POST /api/management/relay-ticket` takes `target`, `uuid`, and `role`
(`controller` or `device`) and issues a 30-second, single-use opaque ticket.
Private `POST /relay/claim` consumes it for that target/attempt. Private
`POST /relay/event` accepts `paired` or `ended`; pairing rechecks both credentials
and current grants. The private listener requires `X-Management-Key` and binds
to loopback only.

The existing protobuf messages have token and control-permission fields, but
their presence does not establish that every client/server path uses them.
Verify the full sender/receiver handshake before selecting token validation or
target-bound connection tickets. The two relay legs must be bound to the same
authorized attempt; matching a caller-chosen UUID is insufficient proof.

Cover TCP/UDP rendezvous, direct and forced relay setup, WebSocket paths if
enabled, and reconnects. Do not impose a caller-token requirement on the receiving
relay leg without verifying how that leg authenticates. If the existing wire
contract cannot enforce the requirement, specify and build the necessary client
change before declaring compatibility.

## Persistence and operating contract

The management service owns a separate database. The existing peer database
remains owned by hbbs. Cross-service updates use authenticated APIs/events rather
than concurrent, undocumented database writes.

Schema version 1 provides a fresh-install schema. No previous management schema
exists in this project; unknown populated schemas are refused. Future upgrades
must supply explicit versioned migrations. The
owner executes production initialization/migrations under the workspace rules.
Do not start a community installer that silently migrates an existing database.
Backup instructions must include databases, server keys, and management secrets.

## Naming compatibility contract (2026-11-11)

Display names are Relivo, Relivo Console and Relivo Agent. Technical RustDesk package/library IDs, configuration filenames, service IDs and the rustdesk:// URI remain stable. Docker images use relivo/*; the existing rustdesk-control Compose project name remains the storage namespace so an upgrade reuses existing volumes. Versioned release directories and archives use relivo-2026-11-11.2.

## Existing Nginx ingress contract (2026-11-11)

The public origin and WSS routes remain unchanged. Apply compose.nginx.yaml after
compose.yaml; it replaces all edge port mappings, mapping 127.0.0.1:18083 to
container HTTP 8088 and retaining device ports. Host Nginx overwrites X-Real-IP
and X-Forwarded-For from the actual peer, forwards Upgrade/Connection and Host,
and owns TLS. The private Caddy ingress passes that X-Real-IP to hbbs/hbbr.
No forwarding header supplied by a public client is authoritative. The host and
containers sharing the stack network remain a single trust boundary.
