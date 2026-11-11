// Real local hbbs/hbbr integration. No external devices or production data.
// Build management and rustdesk-server debug binaries first; run with Node 22+.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createPublicKey, randomBytes, randomUUID, verify } from 'node:crypto';
import { createSocket } from 'node:dgram';
import { mkdtempSync, mkdirSync, openSync, readFileSync, writeFileSync } from 'node:fs';
import { connect } from 'node:net';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const fixture = process.argv[2] === '--docker-fixture'
  ? JSON.parse(readFileSync(process.argv[3], 'utf8')) : null;
if (fixture) {
  assert.equal(fixture.origin, 'https://localhost:24443');
  assert.match(fixture.project, /^rustdesk-test-[a-f0-9]+$/);
}
mkdirSync(resolve(root, '.local'), { recursive: true });
const work = fixture?.work ?? mkdtempSync(resolve(root, '.local/protocol-test-'));
const api = fixture?.origin ?? 'http://127.0.0.1:21414';
const privateApi = 'http://127.0.0.1:21420';
const children = [], sockets = [];
const password = fixture ? readFileSync(fixture.passwordFile, 'utf8').trim() : randomBytes(24).toString('hex');
if (!fixture) writeFileSync(resolve(work, 'internal-key'), randomBytes(32).toString('hex'), { mode: 0o600 });
const env = { ...process.env, MANAGEMENT_DATABASE: resolve(work, 'management.sqlite3'),
  MANAGEMENT_INTERNAL_KEY_FILE: resolve(work, 'internal-key'), MANAGEMENT_ORIGIN: api,
  MANAGEMENT_LISTEN: '127.0.0.1:21414', MANAGEMENT_INTERNAL_LISTEN: '127.0.0.1:21420',
  MANAGEMENT_WEB_DIR: resolve(root, 'management/web/dist'), MANAGEMENT_URL: privateApi,
  MANAGEMENT_RELAY_URL: 'ws://127.0.0.1:22119/', TEST_HBBS: 'no' };
const managerBin = resolve(root, 'management/target/debug/rustdesk-management');
const pause = ms => new Promise(r => setTimeout(r, ms));
function start(binary, args, name) {
  const log = openSync(resolve(work, `${name}.log`), 'w', 0o600);
  const child = spawn(binary, args, { cwd: work, env, stdio: ['ignore', log, log] });
  children.push(child);
  return child;
}
async function ready(port, child) {
  for (let i = 0; i < 100; i++) {
    if (child.exitCode !== null) throw Error(`Service exited (${child.exitCode}); logs in ${work}`);
    const up = await new Promise(r => {
      const s = connect(port, '127.0.0.1');
      s.once('connect', () => { s.destroy(); r(true); });
      s.once('error', () => r(false));
    });
    if (up) return;
    await pause(100);
  }
  throw Error(`Port ${port} did not become ready`);
}
let cookie = '', csrf = '';
async function request(path, method = 'GET', data, token, expected = 200) {
  const headers = { Origin: api, 'Content-Type': 'application/json' };
  if (token) headers.Authorization = `Bearer ${token}`;
  else if (cookie) { headers.Cookie = cookie; headers['X-CSRF-Token'] = csrf; }
  const res = await fetch(api + path, { method, headers, body: data === undefined ? undefined : JSON.stringify(data) });
  const text = await res.text();
  assert.equal(res.status, expected, `${path}: ${text}`);
  return { value: text ? JSON.parse(text) : null, cookie: res.headers.get('set-cookie')?.split(';')[0] };
}
function varint(n) { const b = []; do { const v = n & 127; n >>>= 7; b.push(v | (n ? 128 : 0)); } while (n); return Buffer.from(b); }
function field(n, v) { const b = Buffer.isBuffer(v) ? v : Buffer.from(v); return Buffer.concat([varint(n * 8 + 2), varint(b.length), b]); }
const message = (n, ...fields) => field(n, Buffer.concat(fields));
function decode(bytes) {
  const b = Buffer.from(bytes), out = {}; let offset = 0;
  function read() { let n = 0, shift = 0, v; do { v = b[offset++]; n |= (v & 127) << shift; shift += 7; } while (v & 128); return n; }
  while (offset < b.length) {
    const tag = read(), type = tag & 7, key = tag >>> 3;
    if (type === 0) out[key] = read();
    else { assert.equal(type, 2); const length = read(); out[key] = b.subarray(offset, offset + length); offset += length; }
  }
  return out;
}
async function ws(port) {
  const url = fixture ? `wss://localhost:24443/ws/${port === 22118 ? 'id' : 'relay'}` : `ws://127.0.0.1:${port}`;
  const s = new WebSocket(url); s.binaryType = 'arraybuffer'; sockets.push(s);
  s.queue = []; s.waiters = [];
  s.addEventListener('message', e => { const data = Buffer.from(e.data); const waiter = s.waiters.shift(); if (waiter) waiter(data); else s.queue.push(data); });
  await new Promise((ok, fail) => { s.addEventListener('open', ok, { once: true }); s.addEventListener('error', fail, { once: true }); });
  return s;
}
async function next(s, fieldNumber) {
  for (;;) {
    const b = s.queue.shift() ?? await new Promise((ok, fail) => {
      const timer = setTimeout(() => fail(Error('WebSocket message timeout')), 6000);
      s.waiters.push(b => { clearTimeout(timer); ok(b); });
    });
    if (!fieldNumber || decode(b)[fieldNumber]) return fieldNumber ? decode(decode(b)[fieldNumber]) : b;
  }
}
async function rejected(s, packet) {
  const closed = new Promise((ok, fail) => {
    const timer = setTimeout(() => fail(Error('Invalid relay ticket was not rejected')), 6000);
    s.addEventListener('close', () => { clearTimeout(timer); ok(); }, { once: true });
  });
  s.send(packet); await closed;
}
const id = '654321', pk = Buffer.alloc(32, 7);
let udp;
try {
  let manager;
  if (!fixture) {
    const init = spawnSync(managerBin, ['init-admin', 'admin'], { cwd: work, env, input: password, encoding: 'utf8' });
    assert.equal(init.status, 0, init.stderr);
    manager = start(managerBin, [], 'management'); await ready(21414, manager);
  }
  const login = await request('/management/v1/login', 'POST', { username: 'admin', password });
  cookie = login.cookie; csrf = login.value.csrf;
  const user = (await request('/management/v1/users', 'POST', { username: 'operator', display_name: 'Protocol test operator', role: 'operator', enabled: true, password })).value;
  const enrollment = (await request('/management/v1/devices', 'POST', { id, name: 'Protocol test device', platform: 'Linux' })).value;
  await request('/api/heartbeat', 'POST', { id, uuid: 'test-device-uuid', management_pk: pk.toString('base64') }, enrollment.credential);
  await request(`/management/v1/devices/${id}`, 'PUT', { name: 'Protocol test device', state: 'approved', platform: 'Linux' });
  const group = (await request('/management/v1/groups', 'POST', { name: 'Protocol test group', user_ids: [user.id], device_ids: [id] })).value;
  const grant = (await request('/management/v1/grants', 'POST', { subject_group: group.id, target_group: group.id, capability: 'connect' })).value;
  const token = (await request('/api/login', 'POST', { username: 'operator', password, type: 'account' })).value.access_token;
  if (!fixture) {
    const hbbs = start(resolve(root, 'rustdesk-server/target/debug/hbbs'), ['--bind', '127.0.0.1', '-p', '22116', '-k', '-'], 'hbbs');
    await ready(22118, hbbs);
    const hbbr = start(resolve(root, 'rustdesk-server/target/debug/hbbr'), ['--bind', '127.0.0.1', '-p', '22117', '-k', '-'], 'hbbr');
    await ready(22119, hbbr);
  }
  const serverKey = readFileSync(fixture?.publicKeyFile ?? resolve(work, 'id_ed25519.pub'), 'utf8').trim();
  udp = createSocket('udp4'); await new Promise(ok => udp.bind(0, '127.0.0.1', ok));
  const registered = new Promise((ok, fail) => {
    const timer = setTimeout(() => fail(Error('Device registration timeout')), 5000);
    const handler = b => { const response = decode(b)[16]; if (response) { clearTimeout(timer); udp.off('message', handler); assert.equal(decode(response)[1] ?? 0, 0); ok(); } };
    udp.on('message', handler);
  });
  udp.send(message(15, field(1, id), field(2, 'test-device-uuid'), field(3, pk)), fixture ? 24116 : 22116, '127.0.0.1');
  await registered;
  async function authorize(tokenValue, uuid) {
    const s = await ws(22118);
    s.send(message(18, field(1, id), field(2, uuid), field(8, tokenValue)));
    const response = await next(s, 19); s.close(); return response;
  }
  const bad = await authorize('invalid', randomUUID()); assert.ok(bad[6]?.length);
  const legacy = await ws(22118);
  legacy.send(message(8, field(1, id), field(3, serverKey)));
  assert.ok((await next(legacy, 11))[7]?.length); legacy.close();
  const uuid = randomUUID();
  const forwarded = new Promise((ok, fail) => {
    const timer = setTimeout(() => fail(Error('Receiver was not notified')), 5000);
    const handler = b => { const packet = decode(b)[18]; if (packet) { clearTimeout(timer); udp.off('message', handler); ok(decode(packet)); } };
    udp.on('message', handler);
  });
  const response = await authorize(token, uuid);
  assert.equal(response[2]?.toString(), uuid); assert.ok(response[5]?.length > 64);
  const signingKey = createPublicKey({format:'der',type:'spki',key:Buffer.concat([
    Buffer.from('302a300506032b6570032100','hex'), Buffer.from(serverKey,'base64')])});
  const signedIdentity = response[5];
  assert.ok(verify(null, signedIdentity.subarray(64), signingKey, signedIdentity.subarray(0,64)));
  const identity = decode(signedIdentity.subarray(64));
  assert.equal(identity[1].toString(), id); assert.deepEqual(identity[2], pk);
  const notification = await forwarded; assert.equal(notification[2].toString(), uuid); assert.equal(notification[8]?.length ?? 0, 0);
  async function ticket(credential, role, attempt = uuid) {
    return (await request('/api/management/relay-ticket', 'POST', { target: id, uuid: attempt, role }, credential)).value.ticket;
  }
  const operatorTicket = await ticket(token, 'controller');
  const deviceTicket = await ticket(enrollment.credential, 'device');
  const join = (ticketValue, attempt = uuid) => message(18, field(1, id), field(2, attempt), field(6, serverKey), field(8, ticketValue));
  await rejected(await ws(22119), join('0'.repeat(64)));
  const a = await ws(22119); a.send(join(operatorTicket));
  const b = await ws(22119); b.send(join(deviceTicket));
  const payload = Buffer.from('managed-relay-integration'); a.send(payload);
  assert.deepEqual(await next(b), payload);
  b.send(Buffer.from('response')); assert.equal((await next(a)).toString(), 'response');
  await rejected(await ws(22119), join(operatorTicket));
  a.close(); b.close();
  const staleAttempt = randomUUID();
  const stale = await ticket(token, 'controller', staleAttempt);
  await request(`/management/v1/grants/${grant.id}`, 'DELETE');
  const revoked = await authorize(token, randomUUID()); assert.ok(revoked[6]?.length);
  await rejected(await ws(22119), join(stale, staleAttempt));
  const activity = (await request('/management/v1/audit?q=relay.paired')).value;
  assert.equal(activity.total, 1);
  assert.ok(!JSON.stringify(activity).includes(token));
  if (fixture) {
    const stop = spawnSync('docker', ['compose', '--env-file', fixture.envFile, '-f', resolve(root, 'deployment/compose.yaml'), '-p', fixture.project, 'stop', 'management'], { encoding: 'utf8' });
    assert.equal(stop.status, 0, stop.stderr);
  } else {
    manager.kill('SIGINT'); await new Promise(ok => manager.once('exit', ok));
  }
  const unavailable = await authorize(token, randomUUID()); assert.ok(unavailable[6]?.length);
  console.log('PASS: real hbbs authorization, legacy-path rejection, signed identity, token-free device notification, two-leg relay, bidirectional payload, invalid/replayed tickets, revoked access, audit, management outage.');
  console.log(`Test-only data and service output: ${work}`);
} finally {
  for (const s of sockets) { try { s.close(); } catch {} }
  udp?.close();
  for (const child of children) if (child.exitCode === null) child.kill('SIGTERM');
}
