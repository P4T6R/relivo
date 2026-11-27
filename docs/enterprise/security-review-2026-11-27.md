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
hbbs and hbbr with temporary data on loopback and attempts 35 attacks. Final
result: 35 blocked, 0 successful; it exits 1 if any attack succeeds. Covered: device ID takeover with another UUID,
key substitution with the same UUID, legacy punch-hole and relay requests over
UDP, device token used as an account, viewer and ungranted operator connections,
forged tokens, cross-role and cross-device ticket minting, ticket reuse with
another session UUID, a second controller leg, a third leg, private routes on the
public listener, wrong internal key, CSRF without token, foreign Origin, operator
privilege escalation (create admin, self-promotion, grants), session reuse after
logout, disabled user keeping agent access, brute force throttling, lockout of the
real administrator, 20 MB request body, 60 KB key registration, log injection
through the device ID, a device reporting as another device, a running session
after the controller account is disabled (cut within 60 s), a flood of managed
requests on `/ws/id`, and hbbs started without its management settings.

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
| Revoking a user, grant or device did not end a running session | Medium | hbbr re-checks every 60 s (`relay/event` "active") and cuts the relay on refusal; an unreachable API keeps it. Expired native sign-ins are kept one more day so a long session outlives the 8 h sign-in, while logout, disable and password change still end it |
| No limit on managed requests: each junk `/ws/id` or `/ws/relay` request cost a management call and an audit row | Medium | 60 per minute per client (IPv6 /64) in hbbs and hbbr, checked before the API call; bounded client table |
| hbbs/hbbr silently ran as open stock servers when `MANAGEMENT_URL` was missing | Low | The server image sets `MANAGEMENT_REQUIRED=Y`; startup then fails instead |
| An approved device could flood the activity log | Low | 120 reports per minute per device |
| Managed devices still answered unauthenticated punch-hole and local-address requests (outbound connections to attacker-chosen addresses) | Low | Ignored in managed mode (`rendezvous_mediator.rs`) |
| A console strategy could set a proxy, `disable-udp` or the device credential | Low | Added to the strategy blocklist (`hbbs_http/sync.rs`) |
| A renamed Windows executable (`rustdesk-host=…,key=….exe`) could override the server and key | Low | Ignored in Relivo builds (`platform/windows.rs`); compiled only by the Windows CI |

Tests: management 21, hbbs 23, hbbr 20, hbb_common 101, agent `management::`,
strategy and mediator tests, protocol smoke, and the attack run above.

## Remaining risks, by priority

1. **Device registration is unauthenticated UDP (availability only).** Anyone who
   sees a device's registration packet can replay it from another address, or
   change its key, so that connections to that device fail until it re-registers.
   No access is gained (signature and pinning hold). The open-source hbbs answers
   RegisterPk over TCP with NOT_SUPPORT, so `disable-udp=Y` would stop every
   device from registering; the fix is a protocol change (TCP/WSS registration
   with a challenge signed by the device key) on both server and agent.
2. **The logged-in user of a PC is trusted with the agent configuration.** This is
   RustDesk's model: the same-session IPC already hands the UI the whole
   configuration (`SyncConfig`), including the device key pair and the enrollment
   token. Hiding only the token would be cosmetic. Impact of a stolen token is
   limited (no connection without a controller account and a grant). A fix needs
   an IPC redesign where only the service holds secrets.
3. **Presence and enrollment oracles (low).** Online status of a known ID can be
   queried; hbbs reports "device offline" before authorization. Kept because the
   agent peer list uses it; requests are now rate limited.
4. **Inherited RustDesk channel design (low).** Both directions of the secure
   channel use counter nonces from 1 with one key; only the relay operator could
   observe both streams. Needs a protocol change on both ends.
5. **Owner decisions (low).** Retention of failed sign-in records; removing
   `runtime/admin-password` after the administrator stored the password elsewhere;
   a confirmation prompt for `rustdesk://` links (grants are enforced anyway).
