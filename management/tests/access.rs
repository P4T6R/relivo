use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use base64::Engine;
use http_body_util::BodyExt;
use rustdesk_management::{create_admin, internal_router, open_database, public_router, AppState};
use serde_json::{json, Value};
use std::sync::Arc;
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
        StatusCode::FORBIDDEN
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
    let (_, book, _) = call(&h.app, "GET", "/api/ab", &identity, Value::Null).await;
    assert_eq!(
        serde_json::from_str::<Value>(book["data"].as_str().unwrap()).unwrap()["peers"],
        json!([])
    );
}
