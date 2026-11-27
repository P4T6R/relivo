use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
    Router,
};
use base64::Engine;
use http_body_util::BodyExt;
use rustdesk_management::{create_admin, internal_router, open_database, public_router, AppState};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::Semaphore;
use tower::ServiceExt;

const PASSWORD: &str = "only-for-tests-password-29";
const INTERNAL: &str = "test-internal-secret-012345678901234567890";
#[derive(Clone, Default)]
struct Identity {
    cookie: String,
    csrf: String,
    token: String,
}
struct Harness {
    _dir: tempfile::TempDir,
    state: AppState,
    app: Router,
    private: Router,
    admin: Identity,
}
async fn call(
    app: &Router,
    method: &str,
    path: &str,
    who: &Identity,
    body: Value,
) -> (StatusCode, Value, String) {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("origin", "http://127.0.0.1:21114")
        .header("content-type", "application/json");
    if !who.cookie.is_empty() {
        req = req
            .header("cookie", &who.cookie)
            .header("x-csrf-token", &who.csrf);
    }
    if !who.token.is_empty() {
        req = req.header("authorization", format!("Bearer {}", who.token));
    }
    if path.starts_with("/authorize") || path.starts_with("/relay/") {
        req = req.header("x-management-key", INTERNAL);
    }
    let response = app
        .clone()
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let cookie = response
        .headers()
        .get("set-cookie")
        .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_owned())
        .unwrap_or_default();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let data = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, data, cookie)
}
/// Sends a request as a TCP peer with extra headers; returns the raw body text.
async fn raw(
    app: &Router,
    path: &str,
    token: &str,
    body: &str,
    peer: Option<&str>,
    headers: &[(&str, &str)],
) -> (StatusCode, String) {
    let mut req = Request::builder()
        .method("POST")
        .uri(path)
        .header("origin", "http://127.0.0.1:21114")
        .header("content-type", "application/json");
    if !token.is_empty() {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    for (name, value) in headers {
        req = req.header(*name, *value);
    }
    let mut req = req.body(Body::from(body.to_owned())).unwrap();
    if let Some(peer) = peer {
        let addr: SocketAddr = peer.parse().unwrap();
        req.extensions_mut().insert(ConnectInfo(addr));
    }
    let response = app.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}
fn pk(byte: u8) -> String {
    base64::engine::general_purpose::STANDARD.encode([byte; 32])
}
impl Harness {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = open_database(&dir.path().join("management.sqlite"))
            .await
            .unwrap();
        create_admin(&db, "admin", PASSWORD).await.unwrap();
        let state = AppState {
            db,
            origin: "http://127.0.0.1:21114".into(),
            internal_key: INTERNAL.into(),
            secure_cookies: false,
            hash_slots: Arc::new(Semaphore::new(2)),
        };
        let app = public_router(state.clone(), None);
        let private = internal_router(state.clone());
        let (status, body, cookie) = call(
            &app,
            "POST",
            "/management/v1/login",
            &Identity::default(),
            json!({"username":"admin","password":PASSWORD}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        Self {
            _dir: dir,
            state,
            app,
            private,
            admin: Identity {
                cookie,
                csrf: body["csrf"].as_str().unwrap().into(),
                token: String::new(),
            },
        }
    }
    async fn api(&self, method: &str, path: &str, body: Value) -> Value {
        let (status, value, _) = call(&self.app, method, path, &self.admin, body).await;
        assert_eq!(status, StatusCode::OK, "{path}: {value}");
        value
    }
    async fn user(&self, name: &str, role: &str) -> (String, Identity) {
        let result=self.api("POST","/management/v1/users",json!({"username":name,"display_name":name,"role":role,"enabled":true,"password":PASSWORD})).await;
        (
            result["id"].as_str().unwrap().into(),
            self.login(name).await,
        )
    }
    async fn login(&self, name: &str) -> Identity {
        let (status, result, _) = call(
            &self.app,
            "POST",
            "/api/login",
            &Identity::default(),
            json!({"username":name,"password":PASSWORD,"type":"account"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{result}");
        Identity {
            token: result["access_token"].as_str().unwrap().into(),
            ..Default::default()
        }
    }
    async fn device(&self, id: &str, approve: bool) -> Identity {
        let result = self
            .api(
                "POST",
                "/management/v1/devices",
                json!({"id":id,"name":format!("Workstation {id}"),"platform":"Linux"}),
            )
            .await;
        let identity = Identity {
            token: result["credential"].as_str().unwrap().into(),
            ..Default::default()
        };
        let (status,body,_)=call(&self.app,"POST","/api/heartbeat",&identity,json!({"id":id,"uuid":format!("uuid-{id}"),"management_pk":base64::engine::general_purpose::STANDARD.encode([7u8;32])})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        if approve {
            self.api(
                "PUT",
                &format!("/management/v1/devices/{id}"),
                json!({"name":format!("Workstation {id}"),"state":"approved","platform":"Linux"}),
            )
            .await;
        }
        identity
    }
    async fn group(&self, name: &str, users: Vec<&str>, devices: Vec<&str>) -> String {
        self.api(
            "POST",
            "/management/v1/groups",
            json!({"name":name,"user_ids":users,"device_ids":devices}),
        )
        .await["id"]
            .as_str()
            .unwrap()
            .into()
    }
    async fn grant(&self, subject: &str, target: &str) -> String {
        self.api(
            "POST",
            "/management/v1/grants",
            json!({"subject_group":subject,"target_group":target,"capability":"connect"}),
        )
        .await["id"]
            .as_str()
            .unwrap()
            .into()
    }
    async fn audit_total(&self, query: &str) -> i64 {
        self.api("GET", &format!("/management/v1/audit?q={query}"), Value::Null)
            .await["total"]
            .as_i64()
            .unwrap()
    }
    async fn device_record(&self, id: &str) -> Value {
        self.api("GET", &format!("/management/v1/devices/{id}"), Value::Null)
            .await["device"]
            .clone()
    }
    async fn ticket(&self, who: &Identity, target: &str, uuid: &str, role: &str) -> Value {
        let (status, data, _) = call(
            &self.app,
            "POST",
            "/api/management/relay-ticket",
            who,
            json!({"target":target,"uuid":uuid,"role":role}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{data}");
        data
    }
}

#[tokio::test]
async fn accounts_are_persistent_and_web_writes_require_csrf() {
    let h = Harness::new().await;
    let mut bad = h.admin.clone();
    bad.csrf = "wrong".into();
    let (status, _, _) = call(
        &h.app,
        "POST",
        "/management/v1/groups",
        &bad,
        json!({"name":"Blocked"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let request = Request::builder()
        .method("POST")
        .uri("/management/v1/login")
        .header("origin", "https://attacker.invalid")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"username":"admin","password":PASSWORD}).to_string(),
        ))
        .unwrap();
    assert_eq!(
        h.app.clone().oneshot(request).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let admin = h.api("GET", "/management/v1/users", Value::Null).await["items"][0].clone();
    let (status, _, _) = call(
        &h.app,
        "PUT",
        &format!("/management/v1/users/{}", admin["id"].as_str().unwrap()),
        &h.admin,
        json!({"display_name":"Admin","role":"viewer","enabled":true}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let reopened = open_database(&h._dir.path().join("management.sqlite"))
        .await
        .unwrap();
    let names: Vec<String> = sqlx::query_scalar("SELECT username FROM users")
        .fetch_all(&reopened)
        .await
        .unwrap();
    assert_eq!(names, vec!["admin"]);
    let (status, _, cookie) = call(
        &h.app,
        "DELETE",
        "/management/v1/session",
        &h.admin,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(cookie, "rd_session=");
    assert_eq!(
        call(
            &h.app,
            "GET",
            "/management/v1/session",
            &h.admin,
            Value::Null
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn healthcheck_reports_database_unavailability() {
    let h = Harness::new().await;
    assert_eq!(
        call(&h.app, "GET", "/healthz", &Identity::default(), Value::Null)
            .await
            .0,
        StatusCode::OK
    );
    h.state.db.close().await;
    assert_eq!(
        call(&h.app, "GET", "/healthz", &Identity::default(), Value::Null)
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn a_running_session_ends_when_access_is_revoked() {
    let h = Harness::new().await;
    let (uid, operator) = h.user("support", "operator").await;
    let device = h.device("111111", true).await;
    let group = h.group("Support", vec![&uid], vec!["111111"]).await;
    let grant = h.grant(&group, &group).await;
    let uuid = "revoke-during-session-01";
    for (identity, role) in [(&operator, "controller"), (&device, "device")] {
        let ticket = h.ticket(identity, "111111", uuid, role).await;
        let claim = json!({"uuid":uuid,"target":"111111","ticket":ticket["ticket"]});
        assert_eq!(call(&h.private, "POST", "/relay/claim", &Identity::default(), claim).await.0, StatusCode::OK);
    }
    let event = |name: &'static str| {
        let app = h.private.clone();
        async move { call(&app, "POST", "/relay/event", &Identity::default(), json!({"uuid":uuid,"event":name})).await.0 }
    };
    assert_eq!(event("paired").await, StatusCode::OK);
    // Still allowed: the re-check succeeds and writes no audit row.
    assert_eq!(event("active").await, StatusCode::OK);
    // Past the 8 h sign-in, a running session is kept (within the grace period).
    sqlx::query("UPDATE sessions SET expires_at=strftime('%s','now')-60 WHERE kind='native'")
        .execute(&h.state.db)
        .await
        .unwrap();
    assert_eq!(event("active").await, StatusCode::OK);
    assert_eq!(h.api("GET", "/management/v1/audit?q=relay.revoked", Value::Null).await["total"], 0);
    h.api("DELETE", &format!("/management/v1/grants/{grant}"), Value::Null).await;
    assert_eq!(event("active").await, StatusCode::FORBIDDEN);
    assert_eq!(h.api("GET", "/management/v1/audit?q=relay.revoked", Value::Null).await["total"], 1);
    // An unknown session is never "active".
    let unknown = call(&h.private, "POST", "/relay/event", &Identity::default(), json!({"uuid":"never-claimed-session-01","event":"active"})).await.0;
    assert_eq!(unknown, StatusCode::FORBIDDEN);
    drop(operator);
}

#[tokio::test]
async fn a_disabled_device_ends_its_running_session() {
    let h = Harness::new().await;
    let (uid, operator) = h.user("support", "operator").await;
    let device = h.device("111111", true).await;
    let group = h.group("Support", vec![&uid], vec!["111111"]).await;
    h.grant(&group, &group).await;
    let uuid = "disable-during-session-1";
    for (identity, role) in [(&operator, "controller"), (&device, "device")] {
        let ticket = h.ticket(identity, "111111", uuid, role).await;
        let claim = json!({"uuid":uuid,"target":"111111","ticket":ticket["ticket"]});
        assert_eq!(call(&h.private, "POST", "/relay/claim", &Identity::default(), claim).await.0, StatusCode::OK);
    }
    let active = json!({"uuid":uuid,"event":"active"});
    assert_eq!(call(&h.private, "POST", "/relay/event", &Identity::default(), active.clone()).await.0, StatusCode::OK);
    h.api("PUT", "/management/v1/devices/111111", json!({"name":"Workstation","state":"disabled"})).await;
    assert_eq!(call(&h.private, "POST", "/relay/event", &Identity::default(), active).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn device_activity_reports_are_rate_limited_per_device() {
    let h = Harness::new().await;
    let noisy = h.device("111111", true).await;
    let quiet = h.device("222222", true).await;
    let report = |id: &str, n: usize| json!({"id":id,"action":"new","nonce":format!("flood-test-nonce-{n:06}")});
    for n in 0..120 {
        assert_eq!(call(&h.app, "POST", "/api/audit/conn", &noisy, report("111111", n)).await.0, StatusCode::NO_CONTENT);
    }
    assert_eq!(call(&h.app, "POST", "/api/audit/conn", &noisy, report("111111", 120)).await.0, StatusCode::TOO_MANY_REQUESTS);
    // Another device is not affected.
    assert_eq!(call(&h.app, "POST", "/api/audit/conn", &quiet, report("222222", 0)).await.0, StatusCode::NO_CONTENT);
    let log = h.api("GET", "/management/v1/audit?q=client.conn", Value::Null).await;
    assert_eq!(log["total"], 121);
}

#[tokio::test]
async fn revocation_between_admission_and_pairing_is_enforced() {
    let h = Harness::new().await;
    let (uid, operator) = h.user("support", "operator").await;
    let device = h.device("111111", true).await;
    let group = h.group("Support", vec![&uid], vec!["111111"]).await;
    let grant = h.grant(&group, &group).await;
    let uuid = "revoke-before-pairing-01";
    for (identity, role) in [(&operator, "controller"), (&device, "device")] {
        let ticket = h.ticket(identity, "111111", uuid, role).await;
        assert_eq!(
            call(
                &h.private,
                "POST",
                "/relay/claim",
                &Identity::default(),
                json!({"uuid":uuid,"target":"111111","ticket":ticket["ticket"]})
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    h.api(
        "DELETE",
        &format!("/management/v1/grants/{grant}"),
        Value::Null,
    )
    .await;
    assert_eq!(
        call(
            &h.private,
            "POST",
            "/relay/event",
            &Identity::default(),
            json!({"uuid":uuid,"event":"paired"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        h.api("GET", "/management/v1/audit?q=relay.paired", Value::Null)
            .await["total"],
        0
    );
}

#[tokio::test]
async fn stopped_database_backup_restores_membership_permissions_and_audit() {
    let h = Harness::new().await;
    let (uid, operator) = h.user("support", "operator").await;
    h.device("111111", true).await;
    let group = h.group("Support", vec![&uid], vec!["111111"]).await;
    h.grant(&group, &group).await;
    let before = h.api("GET", "/management/v1/audit", Value::Null).await;
    h.state.db.close().await;
    let source = h._dir.path().join("management.sqlite");
    let backup = h._dir.path().join("restored.sqlite");
    std::fs::copy(&source, &backup).unwrap();
    for suffix in ["-wal", "-shm"] {
        let from = source.with_file_name(format!("management.sqlite{suffix}"));
        if from.exists() {
            std::fs::copy(
                from,
                backup.with_file_name(format!("restored.sqlite{suffix}")),
            )
            .unwrap();
        }
    }
    let state = AppState {
        db: open_database(&backup).await.unwrap(),
        ..h.state.clone()
    };
    let restored = public_router(state, None);
    let (status, peers, _) = call(&restored, "GET", "/api/peers", &operator, Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(peers["total"], 1);
    assert_eq!(peers["data"][0]["id"], "111111");
    let (status, log, _) = call(
        &restored,
        "GET",
        "/management/v1/audit",
        &h.admin,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(log["total"], before["total"]);
    assert_eq!(
        call(
            &restored,
            "POST",
            "/api/management/relay-ticket",
            &operator,
            json!({"uuid":"restored-session-0001","target":"111111","role":"controller"})
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn client_reports_are_authenticated_deduplicated_and_never_log_payload_secrets() {
    let h = Harness::new().await;
    let device = h.device("111111", true).await;
    let report = json!({"id":"111111","action":"new","nonce":"audit-test-nonce-0001","password":"do-not-retain-this","filename":"private-filename"});
    assert_eq!(
        call(
            &h.app,
            "POST",
            "/api/audit/conn",
            &Identity::default(),
            report.clone()
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    for _ in 0..2 {
        assert_eq!(
            call(&h.app, "POST", "/api/audit/conn", &device, report.clone())
                .await
                .0,
            StatusCode::NO_CONTENT
        );
    }
    let log = h
        .api("GET", "/management/v1/audit?q=client.conn", Value::Null)
        .await;
    assert_eq!(log["total"], 1);
    assert_eq!(log["items"][0]["source"], "client report");
    assert!(!log.to_string().contains("do-not-retain-this"));
    assert!(!log.to_string().contains("private-filename"));
    assert!(!log.to_string().contains(&device.token));
}

#[tokio::test]
async fn device_identity_is_pinned_and_pending_devices_cannot_connect() {
    let h = Harness::new().await;
    let device = h.device("123456", false).await;
    let admin = h.login("admin").await;
    assert_eq!(
        call(
            &h.app,
            "POST",
            "/api/management/relay-ticket",
            &admin,
            json!({"target":"123456","uuid":"pending-session-1234","role":"controller"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(call(&h.app,"POST","/api/heartbeat",&device,json!({"id":"123456","uuid":"a-different-uuid","management_pk":base64::engine::general_purpose::STANDARD.encode([7u8;32])})).await.0,StatusCode::FORBIDDEN);
    assert_eq!(call(&h.app,"POST","/api/heartbeat",&Identity::default(),json!({"id":"123456","uuid":"uuid-123456","management_pk":base64::engine::general_purpose::STANDARD.encode([7u8;32])})).await.0,StatusCode::UNAUTHORIZED);
    let new = h
        .api(
            "POST",
            "/management/v1/devices/123456/credential",
            json!({}),
        )
        .await;
    assert_ne!(new["credential"].as_str().unwrap(), device.token);
    assert_eq!(call(&h.app,"POST","/api/heartbeat",&device,json!({"id":"123456","uuid":"uuid-123456","management_pk":base64::engine::general_purpose::STANDARD.encode([7u8;32])})).await.0,StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn permissions_apply_to_native_inventory_rendezvous_and_both_relay_legs() {
    let h = Harness::new().await;
    let (uid, operator) = h.user("support", "operator").await;
    let (viewer_id, viewer) = h.user("observer", "viewer").await;
    let device = h.device("111111", true).await;
    h.device("222222", true).await;
    let subject = h.group("Support", vec![&uid, &viewer_id], vec![]).await;
    let fleet = h.group("Assigned fleet", vec![], vec!["111111"]).await;
    h.grant(&subject, &fleet).await;
    let (status, list, _) = call(
        &h.app,
        "GET",
        "/api/peers?current=1&pageSize=100&accessible=&status=1",
        &operator,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["total"], 1);
    assert_eq!(list["data"][0]["id"], "111111");
    let base = json!({"target":"111111","public_key":base64::engine::general_purpose::STANDARD.encode([7u8;32]),"conn_type":0,"token":operator.token,"attempt":"authorization-test-001"});
    assert_eq!(
        call(
            &h.private,
            "POST",
            "/authorize",
            &Identity::default(),
            base.clone()
        )
        .await
        .0,
        StatusCode::OK
    );
    let mut forbidden = base.clone();
    forbidden["target"] = json!("222222");
    assert_eq!(
        call(
            &h.private,
            "POST",
            "/authorize",
            &Identity::default(),
            forbidden
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let mut forbidden = base.clone();
    forbidden["token"] = json!(viewer.token);
    assert_eq!(
        call(
            &h.private,
            "POST",
            "/authorize",
            &Identity::default(),
            forbidden
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let mut forbidden = base.clone();
    forbidden["public_key"] = json!(base64::engine::general_purpose::STANDARD.encode([9u8; 32]));
    assert_eq!(
        call(
            &h.private,
            "POST",
            "/authorize",
            &Identity::default(),
            forbidden
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let uuid = "relay-session-000001";
    let controller = h.ticket(&operator, "111111", uuid, "controller").await;
    let receiver = h.ticket(&device, "111111", uuid, "device").await;
    let claim = json!({"uuid":uuid,"target":"111111","ticket":controller["ticket"]});
    let mut wrong = claim.clone();
    wrong["target"] = json!("222222");
    assert_eq!(
        call(
            &h.private,
            "POST",
            "/relay/claim",
            &Identity::default(),
            wrong
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, body, _) = call(
        &h.private,
        "POST",
        "/relay/claim",
        &Identity::default(),
        claim.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["role"], "controller");
    assert_eq!(
        call(
            &h.private,
            "POST",
            "/relay/claim",
            &Identity::default(),
            claim
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, body, _) = call(
        &h.private,
        "POST",
        "/relay/claim",
        &Identity::default(),
        json!({"uuid":uuid,"target":"111111","ticket":receiver["ticket"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["role"], "device");
    for event in ["paired", "ended"] {
        assert_eq!(
            call(
                &h.private,
                "POST",
                "/relay/event",
                &Identity::default(),
                json!({"uuid":uuid,"event":event})
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    let log = h
        .api(
            "GET",
            "/management/v1/audit?q=connection.authorize",
            Value::Null,
        )
        .await;
    assert_eq!(log["total"], 4);
    assert!(!log.to_string().contains(&operator.token));
}

#[tokio::test]
async fn grant_revocation_disable_and_expiry_invalidate_previously_minted_tickets() {
    let h = Harness::new().await;
    let (uid, operator) = h.user("support", "operator").await;
    h.device("111111", true).await;
    let group = h.group("Support", vec![&uid], vec!["111111"]).await;
    let grant = h.grant(&group, &group).await;
    let ticket = h
        .ticket(&operator, "111111", "revocation-attempt-1", "controller")
        .await;
    h.api(
        "DELETE",
        &format!("/management/v1/grants/{grant}"),
        Value::Null,
    )
    .await;
    assert_eq!(
        call(
            &h.private,
            "POST",
            "/relay/claim",
            &Identity::default(),
            json!({"uuid":"revocation-attempt-1","target":"111111","ticket":ticket["ticket"]})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    h.grant(&group, &group).await;
    let ticket = h
        .ticket(&operator, "111111", "expiring-attempt-001", "controller")
        .await;
    sqlx::query("UPDATE relay_tickets SET expires_at=0")
        .execute(&h.state.db)
        .await
        .unwrap();
    assert_eq!(
        call(
            &h.private,
            "POST",
            "/relay/claim",
            &Identity::default(),
            json!({"uuid":"expiring-attempt-001","target":"111111","ticket":ticket["ticket"]})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    h.api(
        "PUT",
        &format!("/management/v1/users/{uid}"),
        json!({"display_name":"Support","role":"operator","enabled":false}),
    )
    .await;
    assert_eq!(
        call(&h.app, "POST", "/api/currentUser", &operator, json!({}))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &h.app,
            "POST",
            "/api/management/relay-ticket",
            &operator,
            json!({"uuid":"disabled-attempt-001","target":"111111","role":"controller"})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn api_boundaries_reject_privilege_escalation_and_do_not_expose_secrets() {
    let h = Harness::new().await;
    h.user("support", "operator").await;
    let (status, body, cookie) = call(
        &h.app,
        "POST",
        "/management/v1/login",
        &Identity::default(),
        json!({"username":"support","password":PASSWORD}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let support = Identity {
        cookie,
        csrf: body["csrf"].as_str().unwrap().into(),
        token: String::new(),
    };
    assert_eq!(call(&h.app,"POST","/management/v1/users",&support,json!({"username":"hacker","display_name":"Hacker","role":"admin","enabled":true,"password":PASSWORD})).await.0,StatusCode::FORBIDDEN);
    assert_eq!(
        call(&h.app, "GET", "/management/v1/users", &support, Value::Null)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(&h.app, "GET", "/management/v1/audit", &support, Value::Null)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    h.device("555555", true).await;
    assert_eq!(
        call(
            &h.app,
            "GET",
            "/management/v1/devices/555555",
            &support,
            Value::Null
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let users = h.api("GET", "/management/v1/users", Value::Null).await;
    assert!(!users.to_string().contains("password"));
    let devices = h.api("GET", "/management/v1/devices", Value::Null).await;
    assert!(!devices.to_string().contains("credential"));
    let req = Request::builder()
        .method("POST")
        .uri("/authorize")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    assert_eq!(
        h.private.clone().oneshot(req).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &h.app,
            "POST",
            "/authorize",
            &Identity::default(),
            json!({})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn login_throttle_and_native_wire_contract_are_observable() {
    let h = Harness::new().await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/login")
        .body(Body::from(
            json!({"username":"admin","password":PASSWORD,"type":"account"}).to_string(),
        ))
        .unwrap();
    assert_eq!(
        h.app.clone().oneshot(req).await.unwrap().status(),
        StatusCode::OK
    );
    for _ in 0..8 {
        let (status, _, _) = call(
            &h.app,
            "POST",
            "/api/login",
            &Identity::default(),
            json!({"username":"missing","password":"bad-password"}),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(
        call(
            &h.app,
            "POST",
            "/api/login",
            &Identity::default(),
            json!({"username":"missing","password":"bad-password"})
        )
        .await
        .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    let identity = h.login("admin").await;
    let (_, current, _) = call(
        &h.app,
        "POST",
        "/api/currentUser",
        &identity,
        json!({"id":"123456","uuid":"example"}),
    )
    .await;
    assert_eq!(current["name"], "admin");
    assert!(current.get("data").is_none());
    let (_, options, _) = call(
        &h.app,
        "GET",
        "/api/login-options",
        &Identity::default(),
        Value::Null,
    )
    .await;
    assert_eq!(options, json!([]));
    assert_eq!(
        call(&h.app, "POST", "/api/ab", &identity, json!({"data":"{}"}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    // The native client reads 404 here as "legacy address book"; 405 was shown as an error.
    for path in ["/api/ab/settings", "/api/ab/personal"] {
        assert_eq!(
            call(&h.app, "POST", path, &identity, Value::Null).await.0,
            StatusCode::NOT_FOUND
        );
    }
    let (_, book, _) = call(&h.app, "GET", "/api/ab", &identity, Value::Null).await;
    assert_eq!(
        serde_json::from_str::<Value>(book["data"].as_str().unwrap()).unwrap()["peers"],
        json!([])
    );
}

#[tokio::test]
async fn rejected_device_reports_are_recorded_and_audited_once_per_window() {
    let h = Harness::new().await;
    let device = h.device("333333", true).await;
    let report = |uuid: &str, key: u8| json!({"id":"333333","uuid":uuid,"management_pk":pk(key)});
    // A reinstalled agent: same credential, new key pair, reporting every ~15 s.
    for _ in 0..3 {
        let (status, body, _) =
            call(&h.app, "POST", "/api/heartbeat", &device, report("uuid-333333", 9)).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"], "device identity changed; re-enrollment required");
    }
    assert_eq!(h.audit_total("device.identity_mismatch").await, 1);
    let log = h
        .api("GET", "/management/v1/audit?q=device.identity_mismatch", Value::Null)
        .await;
    assert!(log["items"][0]["detail"].as_str().unwrap().contains("public key"));
    assert!(!log.to_string().contains(&device.token));
    let record = h.device_record("333333").await;
    assert_eq!(record["last_rejection"], "public_key");
    assert!(record["last_rejected_at"].as_i64().is_some());
    assert_eq!(record["state"], "approved");
    // A different reason is audited immediately; the same one again after the window.
    call(&h.app, "POST", "/api/heartbeat", &device, report("other-uuid", 7)).await;
    assert_eq!(h.audit_total("device.identity_mismatch").await, 2);
    assert_eq!(h.device_record("333333").await["last_rejection"], "uuid");
    call(&h.app, "POST", "/api/heartbeat", &device, report("other-uuid", 7)).await;
    assert_eq!(h.audit_total("device.identity_mismatch").await, 2);
    sqlx::query("UPDATE devices SET last_rejected_at=last_rejected_at-601")
        .execute(&h.state.db)
        .await
        .unwrap();
    call(&h.app, "POST", "/api/heartbeat", &device, report("other-uuid", 7)).await;
    assert_eq!(h.audit_total("device.identity_mismatch").await, 3);
    // The pinned agent still reports: a recent conflict stays visible, a stale one clears.
    let (status, _, _) =
        call(&h.app, "POST", "/api/heartbeat", &device, report("uuid-333333", 7)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(h.device_record("333333").await["last_rejection"], "uuid");
    sqlx::query("UPDATE devices SET last_rejected_at=last_rejected_at-601")
        .execute(&h.state.db)
        .await
        .unwrap();
    call(&h.app, "POST", "/api/heartbeat", &device, report("uuid-333333", 7)).await;
    let record = h.device_record("333333").await;
    assert_eq!(record["last_rejection"], "");
    assert!(record["last_rejected_at"].is_null());
    // The credential installed on a computer with another ID: still 401, but attributed.
    let (status, body, _) = call(
        &h.app,
        "POST",
        "/api/heartbeat",
        &device,
        json!({"id":"999999","uuid":"uuid-333333","management_pk":pk(7)}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "device ID does not match the credential");
    assert_eq!(h.device_record("333333").await["last_rejection"], "id_mismatch");
    let log = h
        .api("GET", "/management/v1/audit?q=device.identity_mismatch", Value::Null)
        .await;
    assert!(log["items"][0]["detail"].as_str().unwrap().contains("999999"));
    // An unknown credential writes nothing: no audit row and no device row.
    let before = h.api("GET", "/management/v1/audit", Value::Null).await["total"].clone();
    let stranger = Identity {
        token: "ab".repeat(32),
        ..Default::default()
    };
    for id in ["777777", "888888"] {
        let (status, _, _) = call(
            &h.app,
            "POST",
            "/api/heartbeat",
            &stranger,
            json!({"id":id,"uuid":"x","management_pk":pk(1)}),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(h.api("GET", "/management/v1/audit", Value::Null).await["total"], before);
    assert_eq!(h.api("GET", "/management/v1/devices", Value::Null).await["total"], 1);
    // Disabling is recorded without an identity event; rotation clears everything.
    h.api(
        "PUT",
        "/management/v1/devices/333333",
        json!({"name":"Workstation 333333","state":"disabled"}),
    )
    .await;
    let (status, body, _) =
        call(&h.app, "POST", "/api/heartbeat", &device, report("uuid-333333", 7)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "device disabled");
    assert_eq!(h.device_record("333333").await["last_rejection"], "disabled");
    assert_eq!(h.audit_total("device.identity_mismatch").await, 4);
    h.api("POST", "/management/v1/devices/333333/credential", json!({}))
        .await;
    let record = h.device_record("333333").await;
    assert_eq!(record["last_rejection"], "");
    assert!(record["last_rejected_at"].is_null());
    assert_eq!(record["verified"], false);
}

fn is_backup(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy();
    name.contains(".pre-v2-") && name.ends_with(".bak")
}
#[tokio::test]
async fn version_1_database_is_migrated_in_place_without_data_loss() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("management.sqlite");
    {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let db = sqlx::SqlitePool::connect_with(options).await.unwrap();
        sqlx::raw_sql(include_str!("fixtures/schema-v1.sql"))
            .execute(&db)
            .await
            .unwrap();
        sqlx::raw_sql("INSERT INTO users(id,username,display_name,password_hash,role,created_at) VALUES('u1','owner','Owner','hash','admin',1);
            INSERT INTO devices(id,name,platform,state,credential_hash,uuid,public_key,verified,last_seen,created_at,note) VALUES('123456','Front desk','Windows','approved','cred','uuid','pk',1,5,1,'kept');
            INSERT INTO audit(time,actor,action,target,outcome,source,detail) VALUES(1,'owner','device.create','123456','allowed','administrator','');")
            .execute(&db)
            .await
            .unwrap();
        db.close().await;
    }
    let db = open_database(&path).await.unwrap();
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(version, 2);
    let row: (String, String, String, i64, String, Option<i64>, String) = sqlx::query_as(
        "SELECT name,state,public_key,last_seen,note,last_rejected_at,last_rejection FROM devices WHERE id='123456'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        row,
        ("Front desk".into(), "approved".into(), "pk".into(), 5, "kept".into(), None, String::new())
    );
    let counts: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM users),(SELECT count(*) FROM audit)")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(counts, (1, 1));
    db.close().await;
    let backups: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .filter(|e| is_backup(&e.file_name()))
        .collect();
    assert_eq!(backups.len(), 1);
    let backup = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(backups[0].path())
            .read_only(true),
    )
    .await
    .unwrap();
    let old: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT user_version FROM pragma_user_version),(SELECT count(*) FROM devices)",
    )
    .fetch_one(&backup)
    .await
    .unwrap();
    assert_eq!(old, (1, 1));
    backup.close().await;
    // Reopening a current database does not migrate (or back up) again.
    open_database(&path).await.unwrap().close().await;
    assert_eq!(
        std::fs::read_dir(dir.path())
            .unwrap()
            .filter(|e| is_backup(&e.as_ref().unwrap().file_name()))
            .count(),
        1
    );
    let newer = dir.path().join("newer.sqlite");
    {
        let db = sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&newer)
                .create_if_missing(true),
        )
        .await
        .unwrap();
        sqlx::raw_sql("CREATE TABLE t(x); PRAGMA user_version = 3;")
            .execute(&db)
            .await
            .unwrap();
        db.close().await;
    }
    assert!(open_database(&newer).await.is_err());
}

#[tokio::test]
async fn sysinfo_endpoints_use_the_rustdesk_client_wire_format() {
    let h = Harness::new().await;
    let device = h.device("444444", true).await;
    let info = json!({"id":"444444","uuid":"uuid-444444","management_pk":pk(7),"os":"Windows 11 Pro"}).to_string();
    let (status, body) = raw(&h.app, "/api/sysinfo", &device.token, &info, None, &[]).await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "SYSINFO_UPDATED"));
    assert_eq!(h.device_record("444444").await["platform"], "Windows 11 Pro");
    assert_eq!(
        raw(&h.app, "/api/sysinfo", "", &info, None, &[]).await.0,
        StatusCode::UNAUTHORIZED
    );
    // The client posts an empty body and compares the raw string with its stored copy.
    let (status, version) = raw(&h.app, "/api/sysinfo_ver", &device.token, "", None, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(version.starts_with("1-") && !version.contains('{'), "{version}");
    assert_eq!(
        raw(&h.app, "/api/sysinfo_ver", &device.token, "", None, &[]).await.1,
        version
    );
    assert_eq!(
        raw(&h.app, "/api/sysinfo_ver", "", "", None, &[]).await.0,
        StatusCode::UNAUTHORIZED
    );
    let rotated = h
        .api("POST", "/management/v1/devices/444444/credential", json!({}))
        .await["credential"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        raw(&h.app, "/api/sysinfo_ver", &device.token, "", None, &[]).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, renewed) = raw(&h.app, "/api/sysinfo_ver", &rotated, "", None, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(renewed, version);
    // The heartbeat stays a JSON object that the client parses as a map.
    let beat = json!({"id":"444444","uuid":"uuid-444444","management_pk":pk(7)}).to_string();
    let (status, body) = raw(&h.app, "/api/heartbeat", &rotated, &beat, None, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(serde_json::from_str::<Value>(&body).unwrap().is_object());
}

#[tokio::test]
async fn device_update_without_state_keeps_the_current_state_and_details() {
    let h = Harness::new().await;
    h.device("555555", true).await;
    h.api(
        "PUT",
        "/management/v1/devices/555555",
        json!({"name":"Renamed workstation"}),
    )
    .await;
    let record = h.device_record("555555").await;
    assert_eq!(record["name"], "Renamed workstation");
    assert_eq!(record["state"], "approved");
    assert_eq!(record["platform"], "Linux");
    h.api(
        "PUT",
        "/management/v1/devices/555555",
        json!({"name":"Renamed workstation","state":"pending","note":"Check"}),
    )
    .await;
    let record = h.device_record("555555").await;
    assert_eq!(record["state"], "pending");
    assert_eq!(record["note"], "Check");
    let (status, _, _) = call(
        &h.app,
        "PUT",
        "/management/v1/devices/555555",
        &h.admin,
        json!({"name":"Renamed workstation","state":"unknown"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn sign_in_limits_use_the_forwarded_address_only_behind_a_loopback_proxy() {
    let h = Harness::new().await;
    let bucket = |ip: &str| format!("ip:{}", hex::encode(Sha256::digest(ip.as_bytes())));
    sqlx::query("INSERT INTO login_limits(bucket,count,resets_at) VALUES(?,40,strftime('%s','now')+60)")
        .bind(bucket("203.0.113.7"))
        .execute(&h.state.db)
        .await
        .unwrap();
    let login = json!({"username":"admin","password":PASSWORD}).to_string();
    let native = |peer: &'static str, headers: &'static [(&'static str, &'static str)]| {
        let (app, login) = (h.app.clone(), login.clone());
        async move { raw(&app, "/api/login", "", &login, Some(peer), headers).await.0 }
    };
    // The exhausted client is limited; other clients behind the same proxy are not.
    assert_eq!(
        native("127.0.0.1:50000", &[("x-real-ip", "203.0.113.7")]).await,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        native("127.0.0.1:50001", &[("x-real-ip", "198.51.100.9")]).await,
        StatusCode::OK
    );
    assert_eq!(
        native("127.0.0.1:50002", &[("x-forwarded-for", "192.0.2.200, 203.0.113.7")]).await,
        StatusCode::TOO_MANY_REQUESTS
    );
    // Invalid header values fall back to the peer address.
    assert_eq!(
        native("127.0.0.1:50003", &[("x-real-ip", "203.0.113.7; DROP")]).await,
        StatusCode::OK
    );
    // A non-proxy peer cannot choose its bucket with headers.
    assert_eq!(
        native("192.0.2.10:50004", &[("x-real-ip", "203.0.113.7")]).await,
        StatusCode::OK
    );
    let (status, _) = raw(
        &h.app,
        "/management/v1/login",
        "",
        &login,
        Some("127.0.0.1:50005"),
        &[("x-real-ip", "203.0.113.7")],
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    let log = h
        .api("GET", "/management/v1/audit?q=login", Value::Null)
        .await
        .to_string();
    for ip in ["198.51.100.9", "127.0.0.1", "192.0.2.10"] {
        assert!(log.contains(&format!("from {ip}")), "{ip}: {log}");
    }
    assert!(!log.contains("DROP"));
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM login_limits WHERE bucket=?")
        .bind(bucket("198.51.100.9"))
        .fetch_one(&h.state.db)
        .await
        .unwrap();
    assert_eq!(rows, 1);
}

#[tokio::test]
async fn a_stranger_cannot_lock_an_account_out() {
    let h = Harness::new().await;
    let bad = json!({"username":"admin","password":"not-the-password"}).to_string();
    let good = json!({"username":"admin","password":PASSWORD}).to_string();
    let from = |ip: &'static str, body: String| {
        let app = h.app.clone();
        async move { raw(&app, "/api/login", "", &body, Some("127.0.0.1:50100"), &[("x-real-ip", ip)]).await.0 }
    };
    for _ in 0..8 {
        assert_eq!(from("203.0.113.66", bad.clone()).await, StatusCode::UNAUTHORIZED);
    }
    // The attacker's own address is limited for this account...
    assert_eq!(from("203.0.113.66", good.clone()).await, StatusCode::TOO_MANY_REQUESTS);
    // ...but the real administrator signs in from elsewhere.
    assert_eq!(from("198.51.100.20", good.clone()).await, StatusCode::OK);
    // A distributed attack still meets a per-account ceiling.
    let account = format!("account:{}", hex::encode(Sha256::digest(b"admin")));
    sqlx::query("UPDATE login_limits SET count=120 WHERE bucket=?")
        .bind(&account)
        .execute(&h.state.db)
        .await
        .unwrap();
    assert_eq!(from("192.0.2.77", good).await, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn an_ipv6_prefix_shares_one_sign_in_limit() {
    let h = Harness::new().await;
    let bucket = |key: &str| format!("ip:{}", hex::encode(Sha256::digest(key.as_bytes())));
    sqlx::query("INSERT INTO login_limits(bucket,count,resets_at) VALUES(?,40,strftime('%s','now')+60)")
        .bind(bucket("2001:db8:1:2::/64"))
        .execute(&h.state.db)
        .await
        .unwrap();
    let login = json!({"username":"admin","password":PASSWORD}).to_string();
    let from = |ip: &'static str| {
        let (app, login) = (h.app.clone(), login.clone());
        async move { raw(&app, "/api/login", "", &login, Some("127.0.0.1:50200"), &[("x-real-ip", ip)]).await.0 }
    };
    // Rotating addresses inside the exhausted /64 does not help.
    assert_eq!(from("2001:db8:1:2::1").await, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(from("2001:db8:1:2:ffff:ffff:ffff:fffe").await, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(from("2001:db8:1:3::1").await, StatusCode::OK);
}

#[tokio::test]
async fn connection_refusals_state_a_reason_and_audit_it_without_secrets() {
    let h = Harness::new().await;
    let (_, operator) = h.user("support", "operator").await;
    let (viewer_id, viewer) = h.user("observer", "viewer").await;
    let device = h.device("666666", true).await;
    h.device("666667", false).await;
    h.api(
        "POST",
        "/management/v1/devices",
        json!({"id":"666668","name":"Never reported"}),
    )
    .await;
    let group = h.group("Observers", vec![&viewer_id], vec!["666666"]).await;
    h.grant(&group, &group).await;
    let admin = h.login("admin").await;
    let ticket = |who: &Identity, target: &str, role: &str| {
        let (app, who) = (h.app.clone(), who.clone());
        let body = json!({"target":target,"uuid":"reason-attempt-0001","role":role});
        async move {
            let (status, body, _) =
                call(&app, "POST", "/api/management/relay-ticket", &who, body).await;
            (status, body["error"].as_str().unwrap_or("").to_owned())
        }
    };
    let denied = |reason: &str| (StatusCode::FORBIDDEN, reason.to_owned());
    assert_eq!(ticket(&operator, "666666", "controller").await, denied("no connect grant"));
    assert_eq!(ticket(&viewer, "666666", "controller").await, denied("viewer role cannot connect"));
    assert_eq!(ticket(&admin, "666667", "controller").await, denied("device not approved"));
    assert_eq!(ticket(&admin, "666668", "controller").await, denied("device not verified"));
    let expired = Identity {
        token: "cd".repeat(32),
        ..Default::default()
    };
    assert_eq!(
        ticket(&expired, "666666", "controller").await,
        (StatusCode::UNAUTHORIZED, "session expired".to_owned())
    );
    assert_eq!(
        ticket(&expired, "666666", "device").await,
        (StatusCode::UNAUTHORIZED, "device credential invalid".to_owned())
    );
    assert_eq!(ticket(&device, "666666", "device").await.0, StatusCode::OK);
    let authorize = |token: &str, key: u8| {
        let private = h.private.clone();
        let body = json!({"target":"666666","public_key":pk(key),"conn_type":0,"token":token,"attempt":"authorize-reason-01"});
        async move {
            let (status, body, _) = call(&private, "POST", "/authorize", &Identity::default(), body).await;
            (status, body["error"].as_str().unwrap_or("").to_owned())
        }
    };
    assert_eq!(authorize(&viewer.token, 7).await, denied("viewer role cannot connect"));
    assert_eq!(authorize(&admin.token, 9).await, denied("device identity mismatch"));
    assert_eq!(
        authorize(&expired.token, 7).await,
        (StatusCode::UNAUTHORIZED, "session expired".to_owned())
    );
    assert_eq!(authorize(&admin.token, 7).await.0, StatusCode::OK);
    let log = h
        .api("GET", "/management/v1/audit?q=connection.authorize", Value::Null)
        .await;
    assert_eq!(log["total"], 4);
    let details: Vec<&str> = log["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["detail"].as_str().unwrap())
        .collect();
    assert!(details.contains(&"authorize-reason-01: viewer role cannot connect"));
    assert!(details.contains(&"authorize-reason-01: device identity mismatch"));
    assert!(details.contains(&"authorize-reason-01"));
    for secret in [&viewer.token, &admin.token, &device.token] {
        assert!(!log.to_string().contains(secret.as_str()));
    }
}
