CREATE TABLE users (
 id TEXT PRIMARY KEY, username TEXT NOT NULL UNIQUE COLLATE NOCASE,
 display_name TEXT NOT NULL, password_hash TEXT NOT NULL,
 role TEXT NOT NULL CHECK(role IN ('admin','operator','viewer')),
 enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)), created_at INTEGER NOT NULL
);
CREATE TABLE sessions (
 token_hash TEXT PRIMARY KEY, user_id TEXT NOT NULL REFERENCES users(id),
 csrf TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('web','native')),
 expires_at INTEGER NOT NULL
);
CREATE INDEX session_user ON sessions(user_id);
CREATE TABLE devices (
 id TEXT PRIMARY KEY, name TEXT NOT NULL, platform TEXT NOT NULL DEFAULT '',
 state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','approved','disabled')),
 credential_hash TEXT NOT NULL UNIQUE, uuid TEXT NOT NULL DEFAULT '',
 public_key TEXT NOT NULL DEFAULT '', verified INTEGER NOT NULL DEFAULT 0,
 last_seen INTEGER, created_at INTEGER NOT NULL, note TEXT NOT NULL DEFAULT ''
);
CREATE TABLE groups (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE COLLATE NOCASE);
CREATE TABLE user_groups (
 user_id TEXT NOT NULL REFERENCES users(id), group_id TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
 PRIMARY KEY(user_id,group_id)
);
CREATE TABLE device_groups (
 device_id TEXT NOT NULL REFERENCES devices(id), group_id TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
 PRIMARY KEY(device_id,group_id)
);
CREATE TABLE grants (
 id TEXT PRIMARY KEY, subject_group TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
 target_group TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
 capability TEXT NOT NULL CHECK(capability IN ('view','connect')),
 UNIQUE(subject_group,target_group,capability)
);
CREATE TABLE audit (
 id INTEGER PRIMARY KEY AUTOINCREMENT, time INTEGER NOT NULL, actor TEXT NOT NULL,
 action TEXT NOT NULL, target TEXT NOT NULL, outcome TEXT NOT NULL,
 source TEXT NOT NULL, detail TEXT NOT NULL DEFAULT ''
);
CREATE INDEX audit_time ON audit(time);
CREATE TABLE client_audit_nonces (
 device_id TEXT NOT NULL, nonce TEXT NOT NULL, seen_at INTEGER NOT NULL,
 PRIMARY KEY(device_id,nonce)
);
CREATE TABLE relay_attempts (
 uuid TEXT PRIMARY KEY, device_id TEXT NOT NULL REFERENCES devices(id),
 controller_claimed INTEGER NOT NULL DEFAULT 0, device_claimed INTEGER NOT NULL DEFAULT 0,
 controller_hash TEXT NOT NULL DEFAULT '', device_hash TEXT NOT NULL DEFAULT '',
 expires_at INTEGER NOT NULL
);
CREATE TABLE relay_tickets (
 hash TEXT PRIMARY KEY, uuid TEXT NOT NULL REFERENCES relay_attempts(uuid),
 device_id TEXT NOT NULL, role TEXT NOT NULL CHECK(role IN ('controller','device')),
 credential_hash TEXT NOT NULL, expires_at INTEGER NOT NULL
);
CREATE TABLE login_limits (bucket TEXT PRIMARY KEY, count INTEGER NOT NULL, resets_at INTEGER NOT NULL);
PRAGMA user_version = 1;
