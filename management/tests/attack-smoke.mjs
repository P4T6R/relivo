// Active attack regression against an ISOLATED local management + hbbs + hbbr (temporary data, loopback ports).
// Build management and rustdesk-server debug binaries and the console first; run with Node 22+:
//   node management/tests/attack-smoke.mjs
// Exits 1 if any attack succeeds. See docs/enterprise/security-review-2026-11-27.md.
import { spawn, spawnSync } from 'node:child_process';
import { randomBytes, randomUUID } from 'node:crypto';
import { createSocket } from 'node:dgram';
import { mkdtempSync, openSync, readFileSync, writeFileSync } from 'node:fs';
import { connect } from 'node:net';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const work = mkdtempSync(resolve(root, '.local/attack-run-'));
const api = 'http://127.0.0.1:21414', privateApi = 'http://127.0.0.1:21420';
const children = [], sockets = [];
const password = randomBytes(24).toString('hex');
const internalKey = randomBytes(32).toString('hex');
writeFileSync(resolve(work, 'internal-key'), internalKey, { mode: 0o600 });
const env = { ...process.env, MANAGEMENT_DATABASE: resolve(work, 'management.sqlite3'),
  MANAGEMENT_INTERNAL_KEY_FILE: resolve(work, 'internal-key'), MANAGEMENT_ORIGIN: api,
  MANAGEMENT_LISTEN: '127.0.0.1:21414', MANAGEMENT_INTERNAL_LISTEN: '127.0.0.1:21420',
  MANAGEMENT_WEB_DIR: resolve(root, 'management/web/dist'), MANAGEMENT_URL: privateApi,
  MANAGEMENT_RELAY_URL: 'ws://127.0.0.1:22119/', TEST_HBBS: 'no' };
const results = [];
const report = (name, ok, detail = '') => { results.push({ name, ok, detail }); console.log(`${ok ? 'SAFE' : 'VULN'} | ${name}${detail ? ' | ' + detail : ''}`); };
const sleep = ms => new Promise(r => setTimeout(r, ms));
function start(binary, args, name) {
  const log = openSync(resolve(work, `${name}.log`), 'a');
  const child = spawn(binary, args, { cwd: work, env, stdio: ['ignore', log, log] }); children.push(child); return child;
}
async function ready(port) {
  for (let i = 0; i < 100; i++) { const ok = await new Promise(r => { const s = connect(port, '127.0.0.1'); s.once('connect', () => { s.end(); r(true); }); s.once('error', () => r(false)); }); if (ok) return; await sleep(100); }
  throw Error(`port ${port} not ready`);
}
async function call(path, { method = 'GET', data, token, cookie, csrf, origin = api, headers = {}, base = api, raw } = {}) {
  const h = { 'Content-Type': 'application/json', ...headers };
  if (origin) h.Origin = origin;
  if (token) h.Authorization = `Bearer ${token}`;
  if (cookie) h.Cookie = cookie;
  if (csrf) h['X-CSRF-Token'] = csrf;
  try {
    const res = await fetch(base + path, { method, headers: h, body: raw ?? (data === undefined ? undefined : JSON.stringify(data)) });
    const text = await res.text(); let json = null; try { json = JSON.parse(text); } catch {}
    return { status: res.status, json, text, cookie: res.headers.get('set-cookie')?.split(';')[0] };
  } catch (e) { return { status: 0, error: String(e.cause?.code || e.message) }; }
}
function varint(n) { const b = []; do { const v = n & 127; n >>>= 7; b.push(v | (n ? 128 : 0)); } while (n); return Buffer.from(b); }
function field(n, v) { const b = Buffer.isBuffer(v) ? v : Buffer.from(v); return Buffer.concat([varint(n * 8 + 2), varint(b.length), b]); }
const vfield = (n, v) => Buffer.concat([varint(n * 8), varint(v)]);
const message = (n, ...fields) => field(n, Buffer.concat(fields));
function decode(bytes) {
  const b = Buffer.from(bytes), out = {}; let offset = 0;
  function read() { let n = 0, shift = 0, v; do { v = b[offset++]; n |= (v & 127) << shift; shift += 7; } while (v & 128); return n >>> 0; }
  while (offset < b.length) { const tag = read(), type = tag & 7, key = tag >>> 3;
    if (type === 0) out[key] = read(); else if (type === 2) { const l = read(); out[key] = b.subarray(offset, offset + l); offset += l; } else break; }
  return out;
}
async function ws(port) {
  const s = new WebSocket(`ws://127.0.0.1:${port}`); s.binaryType = 'arraybuffer'; sockets.push(s);
  s.queue = []; s.waiters = []; s.closed = false;
  s.addEventListener('message', e => { const d = Buffer.from(e.data); const w = s.waiters.shift(); if (w) w(d); else s.queue.push(d); });
  s.addEventListener('close', () => { s.closed = true; for (const w of s.waiters.splice(0)) w(null); });
  await new Promise((ok, fail) => { s.addEventListener('open', ok, { once: true }); s.addEventListener('error', fail, { once: true }); });
  return s;
}
async function nextMsg(s, ms = 4000) {
  if (s.queue.length) return s.queue.shift();
  if (s.closed) return null;
  return await new Promise(ok => { const t = setTimeout(() => ok(undefined), ms); s.waiters.push(d => { clearTimeout(t); ok(d); }); });
}
let udp;
async function udpAsk(packet, port = 22116, ms = 2500) {
  return await new Promise(ok => {
    const t = setTimeout(() => { udp.off('message', h); ok(null); }, ms);
    const h = b => { clearTimeout(t); udp.off('message', h); ok(decode(b)); };
    udp.on('message', h); udp.send(packet, port, '127.0.0.1');
  });
}
const registerPk = (id, uuid, pk) => message(15, field(1, id), field(2, uuid), field(3, pk));
const managerBin = resolve(root, 'management/target/debug/rustdesk-management');
try {
  const init = spawnSync(managerBin, ['init-admin', 'admin'], { cwd: work, env, input: password, encoding: 'utf8' });
  if (init.status !== 0) throw Error(init.stderr);
  start(managerBin, [], 'management'); await ready(21414);
  const login = await call('/management/v1/login', { method: 'POST', data: { username: 'admin', password } });
  const admin = { cookie: login.cookie, csrf: login.json.csrf };
  const A = (path, method = 'GET', data) => call(path, { method, data, ...admin });
  const op = (await A('/management/v1/users', 'POST', { username: 'operator', display_name: 'Op', role: 'operator', enabled: true, password })).json;
  const viewer = (await A('/management/v1/users', 'POST', { username: 'viewer', display_name: 'Viewer', role: 'viewer', enabled: true, password })).json;
  const id = '654321', id2 = '765432', uuid0 = 'test-device-uuid', pk = Buffer.alloc(32, 7), pk2 = Buffer.alloc(32, 9);
  const dev = (await A('/management/v1/devices', 'POST', { id, name: 'Target PC', platform: 'Windows' })).json;
  const dev2 = (await A('/management/v1/devices', 'POST', { id: id2, name: 'Other PC', platform: 'Windows' })).json;
  await call('/api/heartbeat', { method: 'POST', data: { id, uuid: uuid0, management_pk: pk.toString('base64') }, token: dev.credential });
  await call('/api/heartbeat', { method: 'POST', data: { id: id2, uuid: 'uuid-2', management_pk: pk2.toString('base64') }, token: dev2.credential });
  for (const d of [id, id2]) await A(`/management/v1/devices/${d}`, 'PUT', { name: d, state: 'approved', platform: 'Windows' });
  const group = (await A('/management/v1/groups', 'POST', { name: 'G', user_ids: [op.id, viewer.id], device_ids: [id] })).json;
  await A('/management/v1/grants', 'POST', { subject_group: group.id, target_group: group.id, capability: 'connect' });
  const tokOp = (await call('/api/login', { method: 'POST', data: { username: 'operator', password, type: 'account' } })).json.access_token;
  const tokViewer = (await call('/api/login', { method: 'POST', data: { username: 'viewer', password, type: 'account' } })).json?.access_token;
  start(resolve(root, 'rustdesk-server/target/debug/hbbs'), ['--bind', '127.0.0.1', '-p', '22116', '-k', '-'], 'hbbs'); await ready(22118);
  start(resolve(root, 'rustdesk-server/target/debug/hbbr'), ['--bind', '127.0.0.1', '-p', '22117', '-k', '-'], 'hbbr'); await ready(22119);
  const serverKey = readFileSync(resolve(work, 'id_ed25519.pub'), 'utf8').trim();
  udp = createSocket('udp4'); await new Promise(ok => udp.bind(0, '127.0.0.1', ok));
  const reg = await udpAsk(registerPk(id, uuid0, pk));
  console.log('setup: legit device registered, result', reg?.[16] ? (decode(reg[16])[1] ?? 0) : 'none');
  const attacker = createSocket('udp4'); await new Promise(ok => attacker.bind(0, '127.0.0.1', ok));
  const attackerAsk = (packet, ms = 2500) => new Promise(ok => { const t = setTimeout(() => { attacker.removeAllListeners('message'); ok(null); }, ms);
    attacker.once('message', b => { clearTimeout(t); ok(decode(b)); }); attacker.send(packet, 22116, '127.0.0.1'); });
  async function authorize(tok, target = id, uuid = randomUUID()) {
    const s = await ws(22118); s.send(message(18, field(1, target), field(2, uuid), field(8, tok)));
    let m; for (;;) { m = await nextMsg(s); if (!m) break; const d = decode(m); if (d[19]) { s.close(); return decode(d[19]); } }
    s.close(); return null;
  }
  const signedPk = r => r?.[5] ? decode(r[5].subarray(64))[2] : null;

  // 1. Device ID takeover with a different UUID (attacker machine claims the victim ID)
  await sleep(1100);
  const t1 = await attackerAsk(registerPk(id, 'attacker-uuid', pk2));
  const t1res = t1?.[16] ? (decode(t1[16])[1] ?? 0) : null;
  let r = await authorize(tokOp);
  report('ID takeover: RegisterPk with another UUID', t1res !== 0 && signedPk(r)?.equals(pk), `register result=${t1res}, signed pk is original=${signedPk(r)?.equals(pk)}`);
  // 2. Same UUID, attacker key (UUID leaked): key substitution
  await sleep(1100);
  const t2 = await attackerAsk(registerPk(id, uuid0, pk2));
  const t2res = t2?.[16] ? (decode(t2[16])[1] ?? 0) : null;
  r = await authorize(tokOp);
  const sp = signedPk(r);
  report('Key substitution: RegisterPk same UUID + attacker key', !(sp && sp.equals(pk2)), `register result=${t2res}, relay granted=${!r?.[6]?.length}, signed pk is attacker=${sp?.equals(pk2) ?? false}, refuse="${r?.[6]?.toString() ?? ''}"`);
  await sleep(1100); await udpAsk(registerPk(id, uuid0, pk));
  await sleep(1100);
  const huge = await attackerAsk(registerPk('888777666', 'x-uuid', Buffer.alloc(60000, 1)));
  const hugeRes = huge?.[16] ? (decode(huge[16])[1] ?? 0) : null;
  report('RegisterPk with a 60 KB key (storage/log flood)', hugeRes === 2, `result=${hugeRes}`);
  await sleep(1100);
  const inj = await attackerAsk(registerPk('777666\n[ERROR] forged', 'x-uuid', pk2));
  report('RegisterPk with newline in ID (log injection)', (inj?.[16] ? (decode(inj[16])[1] ?? 0) : null) === 2);
  // 3. Unenrolled ID registration
  await sleep(1100);
  const t3 = await attackerAsk(registerPk('999111', 'x-uuid', pk2));
  console.log('INFO | unenrolled ID RegisterPk result', t3?.[16] ? (decode(t3[16])[1] ?? 0) : 'none');
  // 4. Legacy punch hole over UDP (stock RustDesk path)
  const t4 = await attackerAsk(message(8, field(1, id), field(3, serverKey)));
  const ph = t4?.[11] ? decode(t4[11]) : null;
  report('Legacy PunchHoleRequest over UDP', !ph || !ph[1]?.length, ph ? `socket_addr=${ph[1]?.length ?? 0}B other_failure="${ph[7]?.toString() ?? ''}"` : 'no response');
  // 5. Legacy RequestRelay over UDP without token
  const t5 = await attackerAsk(message(18, field(1, id), field(2, randomUUID())));
  report('RequestRelay over UDP without token', !t5?.[19] || decode(t5[19])[6]?.length > 0, t5 ? JSON.stringify(Object.keys(t5)) : 'no response');
  // 6. Presence oracle
  const t6 = await attackerAsk(message(23, field(1, 'x'), field(2, id), field(2, id2)));
  console.log('INFO | OnlineRequest response', t6?.[24] ? `states=${decode(t6[24])[1]?.toString('hex')}` : 'none');
  // 7-9. authorize abuse
  r = await authorize(dev.credential); report('Device enrollment token used as controller account', !!r?.[6]?.length, r?.[6]?.toString());
  r = await authorize(tokOp, id2); report('Operator without grant on device 2', !!r?.[6]?.length, r?.[6]?.toString());
  r = await authorize(tokViewer || 'none'); report('Viewer role connect', !!r?.[6]?.length, r?.[6]?.toString());
  r = await authorize(tokOp.slice(0, -2) + '00'); report('Forged account token', !!r?.[6]?.length, r?.[6]?.toString());
  // 10. Ticket API abuse
  const tk = (cred, data) => call('/api/management/relay-ticket', { method: 'POST', data, token: cred });
  const uuidA = randomUUID(); await authorize(tokOp, id, uuidA);
  let x = await tk(dev.credential, { target: id, uuid: uuidA, role: 'controller' }); report('Device token mints controller ticket', !x.json?.ticket, `HTTP ${x.status}`);
  x = await tk(tokOp, { target: id, uuid: uuidA, role: 'device' }); report('Operator token mints device ticket', !x.json?.ticket, `HTTP ${x.status}`);
  x = await tk(tokOp, { target: id2, uuid: uuidA, role: 'controller' }); report('Ticket for a device without grant', !x.json?.ticket, `HTTP ${x.status}`);
  x = await tk(dev2.credential, { target: id, uuid: uuidA, role: 'device' }); report('Other device token mints device ticket for victim', !x.json?.ticket, `HTTP ${x.status}`);
  x = await tk(tokOp, { target: id, uuid: randomUUID(), role: 'controller' }); console.log(`INFO | controller ticket for a uuid never authorized by hbbs: HTTP ${x.status}`);
  // 11. Relay hijack attempts
  const join = (t, u, target = id) => message(18, field(1, target), field(2, u), field(6, serverKey), field(8, t));
  const ctl = (await tk(tokOp, { target: id, uuid: uuidA, role: 'controller' })).json.ticket;
  const devT = (await tk(dev.credential, { target: id, uuid: uuidA, role: 'device' })).json.ticket;
  const wrongUuid = await ws(22119); wrongUuid.send(join(ctl, randomUUID())); await nextMsg(wrongUuid, 2500);
  report('Ticket replayed with another session uuid', wrongUuid.closed, `closed=${wrongUuid.closed}`);
  const ctl2 = (await tk(tokOp, { target: id, uuid: uuidA, role: 'controller' })).json.ticket;
  const a = await ws(22119); a.send(join(ctl2, uuidA));
  const evil = await ws(22119); const ctl3 = (await tk(tokOp, { target: id, uuid: uuidA, role: 'controller' })).json?.ticket;
  evil.send(join(ctl3 || '0'.repeat(64), uuidA)); await sleep(500);
  a.send(Buffer.from('from-controller-a'));
  const evilGot = await nextMsg(evil, 1500);
  report('Second controller leg pairs with first controller', !(evilGot && Buffer.from(evilGot).toString() === 'from-controller-a'), `evil received=${evilGot ? Buffer.from(evilGot).toString() : 'nothing'}, evil closed=${evil.closed}`);
  const b = await ws(22119); b.send(join(devT, uuidA)); await sleep(500);
  const third = await ws(22119); third.send(join(devT, uuidA)); await sleep(800);
  report('Reused device ticket (third leg)', third.closed, `closed=${third.closed}`);
  for (const s of [a, b, evil, third, wrongUuid]) try { s.close(); } catch {}
  // 12. Private routes through the public listener
  for (const p of ['/authorize', '/relay/claim', '/relay/event']) {
    const y = await call(p, { method: 'POST', data: {}, headers: { 'X-Management-Key': internalKey } });
    report(`Private route ${p} on public listener`, y.status === 404 || y.status === 405 || y.status === 0, `HTTP ${y.status}`);
  }
  const k = await call('/authorize', { method: 'POST', data: { token: tokOp, target: id, public_key: pk.toString('base64'), conn_type: 0, attempt: randomUUID() }, base: privateApi, headers: { 'X-Management-Key': 'f'.repeat(64) }, origin: null });
  report('Private API with wrong internal key', k.status === 401 || k.status === 403, `HTTP ${k.status}`);
  // 13. CSRF and origin
  let c = await call('/management/v1/users', { method: 'POST', data: { username: 'evil1', display_name: 'e', role: 'admin', enabled: true, password }, cookie: admin.cookie });
  report('Admin POST with cookie but no CSRF token', c.status === 403 || c.status === 401, `HTTP ${c.status}`);
  c = await call('/management/v1/users', { method: 'POST', data: { username: 'evil2', display_name: 'e', role: 'admin', enabled: true, password }, ...admin, origin: 'https://evil.example' });
  report('Admin POST from foreign Origin', c.status === 403, `HTTP ${c.status}`);
  // 14. Privilege escalation by operator via console API
  const opLogin = await call('/management/v1/login', { method: 'POST', data: { username: 'operator', password } });
  const opS = { cookie: opLogin.cookie, csrf: opLogin.json?.csrf };
  c = await call('/management/v1/users', { method: 'POST', data: { username: 'evil3', display_name: 'e', role: 'admin', enabled: true, password }, ...opS });
  report('Operator creates an admin account', c.status === 403, `HTTP ${c.status}`);
  c = await call(`/management/v1/users/${op.id}`, { method: 'PUT', data: { display_name: 'Op', role: 'admin', enabled: true }, ...opS });
  report('Operator promotes itself to admin', c.status === 403 || c.status === 404 || c.status === 405, `HTTP ${c.status}`);
  c = await call('/management/v1/grants', { method: 'POST', data: { subject_group: group.id, target_group: group.id, capability: 'connect' }, ...opS });
  report('Operator creates grants', c.status === 403, `HTTP ${c.status}`);
  c = await call(`/management/v1/devices/${id2}/rotate`, { method: 'POST', ...opS }); console.log(`INFO | operator rotate device credential: HTTP ${c.status}`);
  c = await call('/management/v1/audit', { ...opS }); console.log(`INFO | operator reads audit: HTTP ${c.status}`);
  // 14b. A running relay session is cut when the controller account is disabled
  const live = randomUUID(); await authorize(tokOp, id, live);
  const liveCtl = (await tk(tokOp, { target: id, uuid: live, role: 'controller' })).json.ticket;
  const liveDev = (await tk(dev.credential, { target: id, uuid: live, role: 'device' })).json.ticket;
  const la = await ws(22119); la.send(join(liveCtl, live));
  const lb = await ws(22119); lb.send(join(liveDev, live)); await sleep(500);
  la.send(Buffer.from('before-revocation'));
  const before = await nextMsg(lb, 2000);
  await A(`/management/v1/users/${op.id}`, 'PUT', { display_name: 'Op', role: 'operator', enabled: false });
  const keepAlive = setInterval(() => { try { la.send(Buffer.from('k')); } catch {} }, 5000);
  const cutAt = Date.now();
  while (!(la.closed && lb.closed) && Date.now() - cutAt < 80000) await sleep(500);
  clearInterval(keepAlive);
  report('Running session survives revocation of the controller account', !!before && la.closed && lb.closed,
    `paired=${!!before}, cut after ${Math.round((Date.now() - cutAt) / 1000)}s`);
  await A(`/management/v1/users/${op.id}`, 'PUT', { display_name: 'Op', role: 'operator', enabled: true });
  // 15. Session revocation
  const lo = await call('/management/v1/session', { method: 'DELETE', ...opS }); console.log('INFO | logout HTTP', lo.status);
  c = await call('/management/v1/devices', { ...opS }); report('Console session reused after logout', c.status === 401, `HTTP ${c.status}`);
  await A(`/management/v1/users/${op.id}`, 'PUT', { display_name: 'Op', role: 'operator', enabled: false });
  x = await tk(tokOp, { target: id, uuid: randomUUID(), role: 'controller' });
  r = await authorize(tokOp);
  report('Disabled user keeps agent access', !x.json?.ticket && !!r?.[6]?.length, `ticket HTTP ${x.status}, authorize refuse="${r?.[6]?.toString() ?? ''}"`);
  // 16. Login brute force throttling
  let codes = [];
  for (let i = 0; i < 15; i++) codes.push((await call('/api/login', { method: 'POST', data: { username: 'admin', password: 'wrong' + i, type: 'account' } })).status);
  report('Login brute force is throttled', codes.includes(429), codes.join(','));
  const realAdmin = await call('/api/login', { method: 'POST', data: { username: 'admin', password, type: 'account' }, headers: { 'X-Real-IP': '198.51.100.44' } });
  report('Attacker locks the real admin out (lockout DoS)', realAdmin.status === 200, `admin from another address: HTTP ${realAdmin.status}`);
  const spoof = (await call('/api/login', { method: 'POST', data: { username: 'admin', password: 'wrong', type: 'account' }, headers: { 'X-Real-IP': '203.0.113.' + (Math.random() * 200 | 0) } })).status;
  console.log(`INFO | throttled login with spoofed X-Real-IP from a loopback peer: HTTP ${spoof} (only reachable through the proxy, which overwrites it)`);
  // 17. Oversized body
  const big = await call('/api/login', { method: 'POST', raw: '{"username":"' + 'a'.repeat(20 * 1024 * 1024) + '"}' });
  report('20 MB body rejected', big.status === 413 || big.status === 400 || big.status === 0, `HTTP ${big.status} ${big.error ?? ''}`);
  // 18. Enumeration through device token on another device report
  x = await call('/api/heartbeat', { method: 'POST', data: { id, uuid: uuid0, management_pk: pk2.toString('base64') }, token: dev2.credential });
  report('Device 2 token reports as device 1', x.status === 401 || x.status === 403, `HTTP ${x.status}`);
  // 19. Flooding /ws/id with managed requests
  let flood = null;
  for (let i = 0; i < 70; i++) { const fr = await authorize('junk-token-' + i); if (fr?.[6]?.toString().includes('too many')) { flood = i; break; } }
  report('Flood of RequestRelay on /ws/id is rate limited', flood !== null, flood === null ? 'never limited' : `limited after ${flood} requests`);
  // 20. Misconfigured managed server must not start open
  const open = spawnSync(resolve(root, 'rustdesk-server/target/debug/hbbs'), ['--bind', '127.0.0.1', '-p', '22216', '-k', '-'],
    { cwd: work, env: { ...process.env, MANAGEMENT_REQUIRED: 'Y', MANAGEMENT_URL: '' }, timeout: 8000, encoding: 'utf8' });
  report('hbbs without MANAGEMENT_URL while MANAGEMENT_REQUIRED=Y', open.status !== 0 && open.status !== null, `exit=${open.status} signal=${open.signal}`);
  const after = await authorize((await call('/api/login', { method: 'POST', data: { username: 'admin', password, type: 'account' } })).json?.access_token ?? '');
  console.log('INFO | admin authorize after attacks refuse=', after?.[6]?.toString() ?? '(granted)');
} catch (e) { console.log('ERROR', e.stack); results.push({ name: 'run', ok: false }); }
finally {
  for (const s of sockets) try { s.close(); } catch {}
  try { udp?.close(); } catch {}
  for (const c of children) c.kill('SIGTERM');
  console.log(`SUMMARY: ${results.filter(r => r.ok).length} safe, ${results.filter(r => !r.ok).length} vulnerable. Logs: ${work}`);
  setTimeout(() => process.exit(results.some(r => !r.ok) || !results.length ? 1 : 0), 300);
}
