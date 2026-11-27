# Relivo security review — 2026-11-27

Requested by the owner: can someone intercept a remote-control session or take
control of a PC, and where must the system be hardened? Scope: branch
`test/windows-fixes`. Method: three independent code reviews (management service
and console, hbbs/hbbr, desktop agent), an active attack run against an isolated
local stack, and non-destructive checks of the production endpoint. No attack
was run against production and nothing was changed there by this review.

## Answer: session interception and takeover

No path was found that lets a third party read, join or take over a managed
session.

- The controller accepts only a device public key signed by the server key built
  into the agent, and aborts when the channel is not secured
  (`rustdesk/src/client.rs`, `start_managed`). There is no fallback to an
  unsigned key, plaintext, direct/LAN/punch-hole connection or public servers.
- hbbs signs only the key it stores, and management refuses the connection when
  that key differs from the key pinned at enrollment ("device identity changed").
  Re-registering a device ID with another key therefore cannot produce a MITM.
- hbbr pairs two legs only with single-use, 30-second relay tickets bound to the
  session UUID, the device and the role (controller/device), re-checked at
  claim and at pairing. The relay forwards ciphertext; the session key is sealed
  to the verified device key.
- The device admits only relay connections through the configured managed relay
  with a device ticket; direct IP, LAN, punch-hole and WebRTC are refused. The
  stock password/OTP/consent checks on the device are unchanged.
- Public transport is TLS 1.2/1.3 with a valid certificate; agent TLS validation
  is never disabled; API redirects are refused.

## Active attack run (isolated local stack)

`node management/tests/attack-smoke.mjs` starts management,
hbbs and hbbr with temporary data on loopback and attempts 32 attacks. Final
result: 32 blocked, 0 successful. Covered: device ID takeover with another UUID,
key substitution with the same UUID, legacy punch-hole and relay requests over
UDP, device token used as an account, viewer and ungranted operator connections,
forged tokens, cross-role and cross-device ticket minting, ticket reuse with
another session UUID, a second controller leg, a third leg, private routes on the
public listener, wrong internal key, CSRF without token, foreign Origin, operator
privilege escalation (create admin, self-promotion, grants), session reuse after
logout, disabled user keeping agent access, brute force throttling, lockout of the
real administrator, 20 MB request body, 60 KB key registration, log injection
through the device ID, and a device reporting as another device.

## Production checks (non-destructive)

TLS 1.0/1.1 refused, TLS 1.2/1.3 accepted; certificate valid until 2027-02-09;
CSP, `frame-ancestors 'none'`, `X-Frame-Options: DENY`, `nosniff`,
`Referrer-Policy: no-referrer` and HSTS present; private ports 18083, 21114,
21117–21120, 8088 and 2080 closed from the internet.

## Fixed in this review

| Issue | Severity | Fix |
| --- | --- | --- |
| Anyone could lock out a known account, including `admin`, with 9 bad sign-ins per minute | Medium | Limit per (account, client), per client, and a high per-account ceiling (`management/src/auth.rs`) |
| Two concurrent bad sign-ins made every other sign-in fail ("busy") | Medium | Wait up to 10 s for a password-hashing slot |
| IPv6 clients could rotate addresses to escape the per-client limit | Low | One /64 prefix counts as one client |
| One spoofed UDP RegisterPk could store and log about 64 KB per new ID | Medium | Key must be 32 bytes, UUID at most 128 bytes, ID in the RustDesk alphabet (`rustdesk-server/src/rendezvous_server.rs`) |
| Device IDs with line breaks could forge hbbs log lines; full keys were logged | Low | IDs logged escaped and bounded; key/UUID logged by length only (`peer.rs`) |
| Server-supplied refusal and mismatch texts were shown unfiltered by the agent | Low | One line, no control characters, bounded (`rustdesk/src/management.rs` `display_text`) |

Tests: management 18, hbbs 21, hbbr 17, hbb_common 101, agent `management::`,
protocol smoke, and the attack run above.

## Remaining risks, by priority

1. **Device registration is unauthenticated UDP (availability).** Anyone who sees
   a device's registration packet can replay it from another address, or change
   its key, so that connections to that device fail until it re-registers. No
   access is gained (signature and pinning hold). Fix: register over the TCP/WSS
   channel (`disable-udp=Y` in the packaged profile) and authenticate RegisterPk
   with the device key. Needs a native Windows validation before release.
2. **Windows: the device enrollment token is readable by local users of the PC**
   through the agent IPC, and is stored in plaintext configuration. Impact is
   limited (the token cannot connect to anything without a controller account
   and a grant). Fix: never return it over IPC, refuse non-elevated writes, store
   it with DPAPI. Needs a Windows test.
3. **No rate limit on `/ws/id` and `/ws/relay`.** Junk requests each cost a
   management call and an audit row. Fix: per-client limits at the proxy and in
   hbbs, and a shorter pre-request timeout.
4. **Revocation does not cut an active session.** Disabling a user or device
   blocks new sessions only (documented V1 behaviour). Fix: periodic re-check in
   hbbr.
5. **Presence and enrollment oracles (low).** Online status of a known ID can be
   queried; hbbs reports "device offline" before authorization. Kept because the
   agent peer list uses it.
6. **Fail-open configuration (low).** hbbs/hbbr run as stock servers if
   `MANAGEMENT_URL` is missing. Add an explicit "managed required" switch.
7. **Inherited RustDesk channel design (low).** Both directions of the secure
   channel use counter nonces from 1 with one key; only the relay operator could
   observe both streams. Needs a protocol change on both ends.
8. **Hygiene (low).** Approved devices can flood the audit log; failed sign-ins
   are never pruned; `runtime/admin-password` stays on disk after bootstrap; a
   renamed Windows executable can override the server (requires running an
   attacker binary); `rustdesk://` links start a session without confirmation
   (grants still enforced); the console strategy option list is a blocklist.
