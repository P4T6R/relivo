// Isolated local Docker acceptance. Never targets the production Compose project.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { chmodSync, mkdtempSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import https from 'node:https';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

process.umask(0o077);
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
mkdirSync(resolve(root, '.local'), {recursive:true});
const work = mkdtempSync(resolve(root, '.local/docker-test-'));
const project = `rustdesk-test-${randomBytes(5).toString('hex')}`;
const envFile = resolve(work, '.env');
const args = ['compose', '--env-file', envFile, '-f', resolve(root, 'deployment/compose.yaml'), '-p', project];
const nginxMode = process.argv.includes('--nginx');
const nginxName = `${project}-nginx`;
let nginxStarted = false;
if (nginxMode) args.push('-f', resolve(root, 'deployment/compose.nginx.yaml'));
function command(binary, argv, options = {}) {
  const result = spawnSync(binary, argv, {cwd:root, encoding:'utf8', timeout:180000, maxBuffer:8*1024*1024, ...options});
  assert.equal(result.status, 0, `${binary} ${argv[0]} failed: ${result.stderr || result.error || result.stdout}`);
  return result.stdout;
}
const compose = (tail, options) => command('docker', [...args, ...tail], options);
command('python3', ['deployment/prepare.py', '--test', '--output', envFile, '--tag', process.argv[2] || '2026-11-11.2']);
if (nginxMode) writeFileSync(envFile, readFileSync(envFile, 'utf8') + "PROXY_PORT='24080'\n");
const passwordFile = resolve(work, 'runtime/admin-password');
const password = readFileSync(passwordFile, 'utf8').trim();
const caFile = resolve(work, 'test-ca.crt');
const publicKeyFile = resolve(work, 'id_ed25519.pub');
let ca;
async function request(path, method='GET', body, headers={}) {
  return await new Promise((ok, fail) => {
    // Compose runs synchronously between checks; never reuse a TLS socket from
    // the intentionally destroyed proxy before its close event can be handled.
    const req = https.request('https://localhost:24443'+path, {method,ca,agent:false,headers:{Origin:'https://localhost:24443','Content-Type':'application/json',...headers}}, res => {
      const chunks = []; res.on('data', c=>chunks.push(c));
      res.on('end',()=>ok({status:res.statusCode,headers:res.headers,body:Buffer.concat(chunks).toString()}));
    });
    req.on('error',error=>fail(new Error(`${method} ${path}: ${error.message}`,{cause:error}))); req.setTimeout(10000,()=>req.destroy(Error('HTTP timeout')));
    req.end(body === undefined ? undefined : JSON.stringify(body));
  });
}
try {
  compose(['config','--quiet']);
  compose(['--profile','setup','run','--rm','-T','bootstrap'], {input:password});
  compose(['up','-d','--no-build','--wait','--wait-timeout','150']);
  for (const service of ['edge','management','hbbs','hbbr']) {
    const id = compose(['ps','-q',service]).trim();
    const info = JSON.parse(command('docker',['inspect',id]))[0];
    assert.equal(info.State.Health.Status,'healthy');
    assert.equal(info.Config.User,'10001:10001');
    assert.equal(info.HostConfig.ReadonlyRootfs,true);
    assert.ok(info.HostConfig.CapDrop.includes('ALL'));
    if (service === 'edge') {
      const expectedPorts = nginxMode ? ['8088/tcp','21115/tcp','21116/tcp','21116/udp'] : ['80/tcp','443/tcp','21115/tcp','21116/tcp','21116/udp'];
      assert.deepEqual(Object.keys(info.HostConfig.PortBindings).sort(),expectedPorts.sort());
      for (const bindings of Object.values(info.HostConfig.PortBindings)) assert.ok(bindings.every(p=>p.HostIp==='127.0.0.1'));
      if (nginxMode) assert.equal((info.HostConfig.CapAdd || []).length, 0);
    } else {
      assert.match(info.HostConfig.NetworkMode,/^container:/);
      assert.equal(Object.keys(info.HostConfig.PortBindings ?? {}).length,0);
    }
  }
  if (nginxMode) {
    const keyFile = resolve(work, 'test-key.pem');
    command('openssl',['req','-x509','-newkey','rsa:2048','-nodes','-days','1','-subj','/CN=localhost','-addext','subjectAltName=DNS:localhost','-keyout',keyFile,'-out',caFile]);
    let site = readFileSync(resolve(root,'deployment/nginx/relivo.conf'),'utf8');
    site = site.replaceAll('relivo-desk.duckdns.org','localhost')
      .replace('listen 80;', 'listen 127.0.0.1:24480;').replace('listen [::]:80;','')
      .replace('listen 443 ssl;', 'listen 127.0.0.1:24443 ssl;').replace('listen [::]:443 ssl;','')
      .replaceAll('/etc/letsencrypt/live/localhost/fullchain.pem','/test/cert.pem')
      .replaceAll('/etc/letsencrypt/live/localhost/privkey.pem','/test/key.pem')
      .replace('127.0.0.1:18083','127.0.0.1:24080')
      .replace('proxy_set_header Host $host;', 'proxy_set_header Host $http_host;');
    const nginxConfig = resolve(work,'nginx.conf');
    writeFileSync(nginxConfig, 'pid /tmp/nginx.pid; error_log /dev/stderr; events {} http { access_log off; client_body_temp_path /tmp/body; proxy_temp_path /tmp/proxy; fastcgi_temp_path /tmp/fastcgi; uwsgi_temp_path /tmp/uwsgi; scgi_temp_path /tmp/scgi;\n'+site+'\n}\n');
    for (const file of [nginxConfig,caFile,keyFile]) chmodSync(file,0o444);
    const mounts = ['--mount',`type=bind,src=${nginxConfig},dst=/test/nginx.conf,readonly`,'--mount',`type=bind,src=${caFile},dst=/test/cert.pem,readonly`,'--mount',`type=bind,src=${keyFile},dst=/test/key.pem,readonly`];
    // Matches the VPS Nginx 1.24 syntax; this test-only image is never deployed.
    const image = process.env.RELIVO_TEST_NGINX_IMAGE || 'nginx:1.24-alpine@sha256:77e5d4a6ad906c5d3793764085706577fa705b1dc6f244ea0241c4b5e2155385';
    const flags = ['--network','host','--user','10001:10001','--read-only','--cap-drop','ALL','--security-opt','no-new-privileges:true','--tmpfs','/tmp:rw,nosuid,size=32m,mode=1777',...mounts,'--entrypoint','nginx',image];
    command('docker',['run','--rm',...flags,'-t','-c','/test/nginx.conf']);
    command('docker',['run','-d','--name',nginxName,...flags,'-c','/test/nginx.conf','-g','daemon off;']);
    nginxStarted = true;
  } else {
    compose(['cp','edge:/data/caddy/pki/authorities/local/root.crt',caFile]);
  }
  compose(['cp','hbbs:/data/id_ed25519.pub',publicKeyFile]);
  ca = readFileSync(caFile);
  // A newly started host proxy may need a moment before accepting connections.
  if (nginxMode) {
    for (let attempt=0; ; attempt++) {
      try { assert.equal((await request('/healthz')).status,200); break; }
      catch (error) { if (attempt===30) throw error; await new Promise(r=>setTimeout(r,100)); }
    }
  }
  const home = await request('/');
  assert.equal(home.status,200);
  assert.match(home.body,/<title>Relivo Console<\/title>/);
  const asset = home.body.match(/src="([^"]+\.js)"/);
  assert.ok(asset);
  assert.equal((await request(asset[1])).status,200);
  assert.equal((await request('/healthz')).status,200);
  // The public static-file fallback refuses POST; neither private route exists here.
  assert.equal((await request('/authorize','POST',{})).status,405);
  assert.equal((await request('/relay/claim','POST',{})).status,405);
  const repeat = spawnSync('docker',[...args,'--profile','setup','run','--rm','-T','bootstrap'],{input:password,encoding:'utf8'});
  assert.notEqual(repeat.status,0);
  assert.match(repeat.stderr,/empty user database/);
  const fixture = resolve(work,'fixture.json');
  writeFileSync(fixture,JSON.stringify({work,project,envFile,origin:'https://localhost:24443',passwordFile,publicKeyFile}));
  const result = command(process.execPath,['management/tests/protocol-smoke.mjs','--docker-fixture',fixture],{env:{...process.env,NODE_EXTRA_CA_CERTS:caFile}});
  process.stdout.write(result);
  compose(['stop']);
  const backup = resolve(work,'snapshot.tar.gz');
  writeFileSync(backup,compose(['--profile','maintenance','run','--rm','-T','snapshot'],{encoding:null}));
  const entries = command('tar',['-tzf',backup]);
  for (const entry of ['management/management.sqlite3','peers/id_ed25519',nginxMode ? 'tls/' : 'tls/caddy/','tls-config/']) assert.ok(entries.includes(entry),entry);
  // Recreate every container, including the namespace owner, while keeping volumes.
  compose(['down']);
  compose(['up','-d','--no-build','--wait','--wait-timeout','150']);
  const restoredKey = resolve(work,'restored-public-key');
  compose(['cp','hbbs:/data/id_ed25519.pub',restoredKey]);
  assert.equal(readFileSync(restoredKey,'utf8'),readFileSync(publicKeyFile,'utf8'));
  const login = await request('/management/v1/login','POST',{username:'admin',password});
  assert.equal(login.status,200);
  const session = JSON.parse(login.body);
  const cookie = login.headers['set-cookie'][0].split(';')[0];
  assert.match(login.headers['set-cookie'][0],/Secure/);
  const headers = {Cookie:cookie,'X-CSRF-Token':session.csrf};
  for (const [path,total] of [['/users',2],['/devices',1],['/audit?q=relay.paired',1]]) {
    const response = await request('/management/v1'+path,'GET',undefined,headers);
    assert.equal(response.status,200); assert.equal(JSON.parse(response.body).total,total);
  }
  console.log(`PASS (${nginxMode ? 'Nginx TLS + private Caddy' : 'standalone Caddy TLS'}): non-root/read-only containers, private listeners, trusted HTTPS/WSS, bootstrap protection, protocol permissions, outage denial, stopped-stack snapshot, full-stack recreation, preserved accounts/devices/audit/server identity.`);
  console.log(`Local test evidence: ${work}`);
} catch (error) {
  const logs = spawnSync('docker',[...args,'logs','--no-color','--tail','100'],{encoding:'utf8'});
  writeFileSync(resolve(work,'services.log'),(logs.stdout||'')+(logs.stderr||''));
  console.error(`Test failed. Local service output: ${work}/services.log`);
  throw error;
} finally {
  // Only this fresh, randomly named test project's disposable volumes are removed.
  if (nginxStarted) command('docker',['rm','-f',nginxName]);
  compose(['down','--volumes','--remove-orphans']);
}
