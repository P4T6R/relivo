# Central administration: research and proposed implementation

Research date: 2026-11-10. Architecture approved by the owner on 2026-11-10.
Implementation is in `management/` with focused client/server hooks. See
[implementation status](implementation-status.md) for actual validation and limits.
The existing official-binary desktop preview does not include these changes.

The owner selected: web console, devices, users, groups, access rights, and
activity logs. SSO, LDAP, and two-factor authentication are outside this first
delivery. This document records the research, rather than claiming feature
parity with a commercial edition.

## Findings

**Official product documentation.** RustDesk Server Pro provides centralized
administration and identity/policy features as a commercial self-hosted product.
The public `rustdesk-server-pro` repository contains installation and maintenance
scripts, not the implementation of those enterprise features. They cannot be
enabled by changing a flag in our OSS checkout.

- [Official Pro documentation](https://rustdesk.com/docs/en/self-host/rustdesk-server-pro/)
- [Public Pro repository](https://github.com/rustdesk/rustdesk-server-pro)

**Local source evidence.** The client already makes account, inventory, address
book, heartbeat, and audit API calls. The OSS server does not implement the
corresponding administration service. Its `ui/` directory is a small service
controller, not the requested web console.

| Capability | Baseline source evidence | Work identified before implementation |
| --- | --- | --- |
| Client account login | `rustdesk/flutter/lib/models/user_model.dart`; `common/hbbs/hbbs.dart` | Account storage, session lifecycle, compatible responses |
| Users and devices | `rustdesk/flutter/lib/models/group_model.dart` | Authorized, scoped API queries |
| Address books | `rustdesk/flutter/lib/models/ab_model.dart` | Storage, sharing rules, compatibility tests |
| Device reporting | `rustdesk/src/hbbs_http/sync.rs` | Verified enrollment, ingestion, inventory, freshness rules |
| Audit submission | `rustdesk/src/common.rs` | Event storage, trust classification, filtering |
| Connection authorization | `rustdesk-server/src/rendezvous_server.rs`, `relay_server.rs` | Identity validation and target-specific authorization |
| Protocol extension points | Both `libs/hbb_common/protos/rendezvous.proto` files | Implement and verify token/permission handling across all connection paths |

The current rendezvous handler checks a configured shared server key. The relay
checks a shared key and pairs requests by a session identifier. Neither check
establishes that an individual user may access a specific device. Hiding an item
in the console or address book would therefore not enforce remote-access rights.
Likewise, a client UUID is not sufficient evidence of device identity.

## Existing implementation options

| Option | What the published sources provide | Consequence for this workspace |
| --- | --- | --- |
| Official RustDesk Pro | Maintainer-provided commercial administration stack | Requires the commercial server; the public repository does not supply its implementation |
| `lejianwen/rustdesk-api` | Go API and separate web frontend; accounts, devices, address books, groups, and activity views | Additive API option, but connection enforcement needs separate server work; latest observed release is v2.7, dated 2025-09-28 |
| BetterDesk | Published console, roles, fleet management, and audit features | Replaces the Rust hbbs/hbbr implementation with a Go server and adds a Node console |
| Extend this fork | Implement a management service and console against the existing client/server | Preserves our current foundation; requires development and maintenance, including actual transport authorization |

The `rustdesk-api` documentation recommends its companion server fork to require
login before initiating connections. That recommendation is not proof of
granular device ACL enforcement with our unchanged server. Compatibility with
our client revision has not been tested.

- [API project and documented integration requirements](https://github.com/lejianwen/rustdesk-api/blob/master/README_EN.md)
- [API v2.7](https://github.com/lejianwen/rustdesk-api/releases/tag/v2.7)

BetterDesk's latest non-prerelease release observed through GitHub was
`v3.5.200`, published 2026-10-30, commit
`4d23f92ad1af6134717ea7c5b9df6cf06d4882e2`. Its current development branch is
different; it is not the proposed evaluation baseline. The release fixes
device-scope and connection-permission bugs. Its source declares AGPL-3.0.
These are published claims and source observations, not results of a local
security review or interoperability test.

- [BetterDesk release](https://github.com/UNITRONIX/BetterDesk/releases/tag/v3.5.200)
- [Versioned console documentation](https://github.com/UNITRONIX/BetterDesk/blob/v3.5.200/docs/wiki/Web-Console.md)
- [Versioned role model](https://github.com/UNITRONIX/BetterDesk/blob/v3.5.200/docs/wiki/Organizations-and-RBAC.md)
- [Versioned source and license](https://github.com/UNITRONIX/BetterDesk/tree/v3.5.200)

The separate `sctgdesk-api-server` repository is archived and directs users to a
consolidated server repository. It should not be selected as an independently
maintained add-on based on older tutorials.

- [Archived API repository](https://github.com/sctg-development/sctgdesk-api-server)
- [Consolidated project](https://github.com/sctg-development/sctgdesk-server)

## Recommendation and decision

**Recommendation:** extend the existing RustDesk fork with a separate management
API and web console, and add authorization hooks to the connection services.
This fits the owner's current project and avoids silently replacing its server.
Use the documented client contracts and public implementations as compatibility
references. Do not promise an immediate Pro-equivalent product.

The alternative is to evaluate BetterDesk locally as a replacement backend,
preserving the existing source trees. This could reuse more existing features,
but changes the server architecture and maintenance base. Its published feature
list alone is insufficient to approve deployment.

**Decision:** the owner approved extending the current fork. No license purchase,
server replacement, production connection, or production database migration has
been performed.

The ordered specifications below make the recommended option reviewable:

1. [PRD](01-prd.md)
2. [SRS](02-srs.md)
3. [Contract](03-contract.md)
4. [Architecture](04-architecture.md)
5. [Modeling and acceptance](05-modeling.md)

## Evidence limits

- Source inspection and published documentation establish feasibility, not
  working integration.
- The local preview uses an official prebuilt client; our fork has not been built.
- Rust and Flutter toolchains are not currently available in the shell.
- The Ubuntu Hostinger VPS is user-reported; its actual capacity and configuration
  have not been inspected. Production execution remains with the owner.
- No real two-device connection, denied-connection test, restore test, or load
  test has been performed for the proposed administration service.
