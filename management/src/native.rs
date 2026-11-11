use crate::{
    admin::{self, Page},
    audit, auth, denied, invalid, now, unauthorized, ApiResult, AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};

pub(crate) async fn current_user(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    Ok(Json(
        auth::native_principal(&s, &headers).await?.native_payload(),
    ))
}
pub(crate) async fn logout(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    auth::revoke(&s, &auth::native_principal(&s, &headers).await?).await?;
    Ok(Json(json!({})))
}
#[derive(Deserialize)]
pub(crate) struct NativePage {
    current: Option<i64>,
    #[serde(rename = "pageSize")]
    page_size: Option<i64>,
}
impl NativePage {
    fn page(self) -> Page {
        Page {
            page: self.current,
            limit: self.page_size,
            q: None,
        }
    }
}
pub(crate) async fn users(State(s): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let p = auth::native_principal(&s, &headers).await?;
    Ok(Json(json!({"total":1,"data":[p.native_payload()]})))
}
pub(crate) async fn peers(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(page): Query<NativePage>,
) -> ApiResult<Json<Value>> {
    let p = auth::native_principal(&s, &headers).await?;
    let (items, total) = admin::visible_devices(&s, &p, &page.page()).await?;
    let data:Vec<Value>=items.into_iter().map(|d|json!({"id":d.id,"info":{"device_name":d.name,"os":d.platform},"note":d.note,"status":if d.state=="approved"{1}else{0}})).collect();
    Ok(Json(json!({"total":total,"data":data})))
}
pub(crate) async fn groups(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(page): Query<NativePage>,
) -> ApiResult<Json<Value>> {
    let p = auth::native_principal(&s, &headers).await?;
    let Json(result) = admin::groups(State(s), axum::Extension(p), Query(page.page())).await?;
    Ok(Json(
        json!({"total":result["total"],"data":result["items"]}),
    ))
}

pub(crate) async fn heartbeat(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    device_report(&s, &headers, body, false).await
}
pub(crate) async fn sysinfo(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    device_report(&s, &headers, body, true).await
}
async fn device_report(
    s: &AppState,
    headers: &HeaderMap,
    body: Value,
    sysinfo: bool,
) -> ApiResult<Json<Value>> {
    let credential = auth::digest(auth::bearer(headers)?);
    let id = body["id"]
        .as_str()
        .ok_or_else(|| invalid("Device ID required"))?;
    let uuid = body["uuid"]
        .as_str()
        .ok_or_else(|| invalid("Device UUID required"))?;
    let pk = body["management_pk"]
        .as_str()
        .ok_or_else(|| invalid("Managed device public key required"))?;
    admin::device_id(id)?;
    if uuid.is_empty() || uuid.len() > 256 {
        return Err(invalid("Invalid UUID"));
    }
    let pk_bytes = base64::engine::general_purpose::STANDARD
        .decode(pk)
        .map_err(|_| invalid("Invalid device public key"))?;
    if pk_bytes.len() != 32 {
        return Err(invalid("Invalid device public key"));
    }
    let mut tx = s.db.begin().await?;
    let device: Option<(String, String, String, bool)> = sqlx::query_as(
        "SELECT uuid,public_key,state,verified FROM devices WHERE id=? AND credential_hash=?",
    )
    .bind(id)
    .bind(&credential)
    .fetch_optional(&mut *tx)
    .await?;
    let (old_uuid, old_pk, state, verified) = device.ok_or_else(unauthorized)?;
    if state == "disabled"
        || (!old_uuid.is_empty() && old_uuid != uuid)
        || (!old_pk.is_empty() && old_pk != pk)
    {
        return Err(denied());
    }
    sqlx::query("UPDATE devices SET uuid=?,public_key=?,verified=1,last_seen=? WHERE id=?")
        .bind(uuid)
        .bind(pk)
        .bind(now())
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if sysinfo {
        let platform = body["os"].as_str().unwrap_or("");
        if platform.len() > 120 {
            return Err(invalid("OS description too long"));
        }
        sqlx::query("UPDATE devices SET platform=? WHERE id=?")
            .bind(platform)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if !verified {
        audit(
            &mut tx,
            id,
            "device.enroll",
            id,
            "allowed",
            "device credential",
            "Identity pinned; administrator approval required",
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(json!({"modified_at":0,"version":"1"})))
}

async fn credential_can_connect(
    conn: &mut sqlx::SqliteConnection,
    hash: &str,
    target: &str,
) -> ApiResult<bool> {
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM sessions s JOIN users u ON u.id=s.user_id JOIN devices d ON d.id=? WHERE s.token_hash=? AND s.kind='native' AND s.expires_at>? AND u.enabled=1 AND u.role IN ('admin','operator') AND d.state='approved' AND d.verified=1 AND (u.role='admin' OR EXISTS(SELECT 1 FROM user_groups ug JOIN grants g ON g.subject_group=ug.group_id JOIN device_groups dg ON dg.group_id=g.target_group WHERE ug.user_id=u.id AND dg.device_id=d.id AND g.capability='connect'))")
        .bind(target).bind(hash).bind(now()).fetch_one(conn).await?;
    Ok(count > 0)
}
#[derive(Deserialize)]
pub(crate) struct Authorization {
    token: String,
    target: String,
    attempt: String,
    public_key: String,
    #[serde(default)]
    conn_type: i32,
}
pub(crate) async fn authorize(
    State(s): State<AppState>,
    Json(input): Json<Authorization>,
) -> ApiResult<Json<Value>> {
    admin::device_id(&input.target)?;
    relay_uuid(&input.attempt)?;
    let hash = auth::digest(&input.token);
    let mut tx = s.db.begin().await?;
    let actor: Option<String> = sqlx::query_scalar(
        "SELECT u.username FROM users u JOIN sessions s ON s.user_id=u.id WHERE s.token_hash=?",
    )
    .bind(&hash)
    .fetch_optional(&mut *tx)
    .await?;
    let pk: Option<String> = sqlx::query_scalar("SELECT public_key FROM devices WHERE id=?")
        .bind(&input.target)
        .fetch_optional(&mut *tx)
        .await?;
    let allowed = input.conn_type == 0
        && !input.public_key.is_empty()
        && pk.as_deref() == Some(input.public_key.as_str())
        && credential_can_connect(&mut tx, &hash, &input.target).await?;
    audit(
        &mut tx,
        actor.as_deref().unwrap_or("unauthenticated"),
        "connection.authorize",
        &input.target,
        if allowed { "allowed" } else { "denied" },
        "rendezvous",
        &input.attempt,
    )
    .await?;
    tx.commit().await?;
    if !allowed {
        return Err(denied());
    }
    Ok(Json(json!({"allowed":true,"attempt":input.attempt})))
}

#[derive(Deserialize)]
pub(crate) struct TicketRequest {
    target: String,
    uuid: String,
    role: String,
}
fn relay_uuid(value: &str) -> ApiResult<()> {
    if !(16..=128).contains(&value.len())
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
    {
        return Err(invalid("Invalid relay attempt ID"));
    }
    Ok(())
}
pub(crate) async fn relay_ticket(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<TicketRequest>,
) -> ApiResult<Json<Value>> {
    admin::device_id(&input.target)?;
    relay_uuid(&input.uuid)?;
    if !["controller", "device"].contains(&input.role.as_str()) {
        return Err(invalid("Invalid relay role"));
    }
    let credential = auth::digest(auth::bearer(&headers)?);
    let mut tx = s.db.begin().await?;
    let allowed = if input.role == "controller" {
        credential_can_connect(&mut tx, &credential, &input.target).await?
    } else {
        sqlx::query_scalar::<_,i64>("SELECT count(*) FROM devices WHERE id=? AND credential_hash=? AND state='approved' AND verified=1")
            .bind(&input.target).bind(&credential).fetch_one(&mut *tx).await?>0
    };
    if !allowed {
        return Err(denied());
    }
    sqlx::query("DELETE FROM relay_tickets WHERE expires_at<=?")
        .bind(now())
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM relay_attempts WHERE expires_at<=? AND uuid NOT IN(SELECT uuid FROM relay_tickets)").bind(now()-86400).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO relay_attempts(uuid,device_id,expires_at) VALUES(?,?,?) ON CONFLICT(uuid) DO NOTHING").bind(&input.uuid).bind(&input.target).bind(now()+60).execute(&mut *tx).await?;
    let (target,expiry,controller,device):(String,i64,i64,i64)=sqlx::query_as("SELECT device_id,expires_at,controller_claimed,device_claimed FROM relay_attempts WHERE uuid=?").bind(&input.uuid).fetch_one(&mut *tx).await?;
    if target != input.target
        || expiry <= now()
        || (input.role == "controller" && controller != 0)
        || (input.role == "device" && device != 0)
    {
        return Err(denied());
    }
    let outstanding: i64 =
        sqlx::query_scalar("SELECT count(*) FROM relay_tickets WHERE credential_hash=?")
            .bind(&credential)
            .fetch_one(&mut *tx)
            .await?;
    if outstanding >= 20 {
        return Err(invalid("Too many pending relay tickets"));
    }
    let ticket = auth::random_token();
    sqlx::query("INSERT INTO relay_tickets(hash,uuid,device_id,role,credential_hash,expires_at) VALUES(?,?,?,?,?,?)")
        .bind(auth::digest(&ticket)).bind(&input.uuid).bind(&target).bind(&input.role).bind(credential).bind(now()+30).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"ticket":ticket,"expires_in":30})))
}
#[derive(Deserialize)]
pub(crate) struct Claim {
    ticket: String,
    uuid: String,
    target: String,
}
pub(crate) async fn claim_relay(
    State(s): State<AppState>,
    Json(input): Json<Claim>,
) -> ApiResult<Json<Value>> {
    relay_uuid(&input.uuid)?;
    admin::device_id(&input.target)?;
    let mut tx = s.db.begin().await?;
    let ticket:Option<(String,String)>=sqlx::query_as("SELECT role,credential_hash FROM relay_tickets WHERE hash=? AND uuid=? AND device_id=? AND expires_at>?")
        .bind(auth::digest(&input.ticket)).bind(&input.uuid).bind(&input.target).bind(now()).fetch_optional(&mut *tx).await?;
    let (role, credential) = ticket.ok_or_else(denied)?;
    let (allowed, actor) = if role == "controller" {
        let allow = credential_can_connect(&mut tx, &credential, &input.target).await?;
        let actor: Option<String> = sqlx::query_scalar(
            "SELECT u.username FROM users u JOIN sessions s ON u.id=s.user_id WHERE s.token_hash=?",
        )
        .bind(&credential)
        .fetch_optional(&mut *tx)
        .await?;
        (allow, actor.unwrap_or_else(|| "revoked".into()))
    } else {
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM devices WHERE id=? AND credential_hash=? AND state='approved' AND verified=1").bind(&input.target).bind(&credential).fetch_one(&mut *tx).await?;
        (count > 0, input.target.clone())
    };
    if !allowed {
        return Err(denied());
    }
    let sql = if role == "controller" {
        "UPDATE relay_attempts SET controller_claimed=1,controller_hash=? WHERE uuid=? AND device_id=? AND controller_claimed=0 AND expires_at>?"
    } else {
        "UPDATE relay_attempts SET device_claimed=1,device_hash=? WHERE uuid=? AND device_id=? AND device_claimed=0 AND expires_at>?"
    };
    let updated = sqlx::query(sql)
        .bind(&credential)
        .bind(&input.uuid)
        .bind(&input.target)
        .bind(now())
        .execute(&mut *tx)
        .await?;
    if updated.rows_affected() != 1 {
        return Err(denied());
    }
    sqlx::query("DELETE FROM relay_tickets WHERE hash=?")
        .bind(auth::digest(&input.ticket))
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        &actor,
        "relay.admit",
        &input.target,
        "allowed",
        "relay",
        &format!("{} {}", role, input.uuid),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"role":role,"target":input.target,"attempt":input.uuid}),
    ))
}
#[derive(Deserialize)]
pub(crate) struct RelayEvent {
    uuid: String,
    event: String,
}
pub(crate) async fn relay_event(
    State(s): State<AppState>,
    Json(input): Json<RelayEvent>,
) -> ApiResult<Json<Value>> {
    if !["paired", "ended"].contains(&input.event.as_str()) {
        return Err(invalid("Unknown relay event"));
    }
    let mut tx = s.db.begin().await?;
    let target:Option<String>=sqlx::query_scalar("SELECT device_id FROM relay_attempts WHERE uuid=? AND controller_claimed=1 AND device_claimed=1").bind(&input.uuid).fetch_optional(&mut *tx).await?;
    let target = target.ok_or_else(denied)?;
    if input.event == "paired" {
        let (controller, device, expires): (String, String, i64) = sqlx::query_as(
            "SELECT controller_hash,device_hash,expires_at FROM relay_attempts WHERE uuid=?",
        )
        .bind(&input.uuid)
        .fetch_one(&mut *tx)
        .await?;
        let valid_device:i64=sqlx::query_scalar("SELECT count(*) FROM devices WHERE id=? AND credential_hash=? AND state='approved' AND verified=1").bind(&target).bind(device).fetch_one(&mut *tx).await?;
        if expires <= now()
            || valid_device == 0
            || !credential_can_connect(&mut tx, &controller, &target).await?
        {
            return Err(denied());
        }
    }
    audit(
        &mut tx,
        "relay",
        &format!("relay.{}", input.event),
        &target,
        "observed",
        "relay",
        &input.uuid,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
pub(crate) async fn client_audit(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(kind): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult<StatusCode> {
    if !["conn", "file", "alarm"].contains(&kind.as_str()) {
        return Err(invalid("Unknown audit type"));
    }
    let id = body["id"]
        .as_str()
        .ok_or_else(|| invalid("Device ID required"))?;
    let mut tx = s.db.begin().await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM devices WHERE id=? AND credential_hash=? AND state='approved'",
    )
    .bind(id)
    .bind(auth::digest(auth::bearer(&headers)?))
    .fetch_one(&mut *tx)
    .await?;
    if count == 0 {
        return Err(denied());
    }
    if let Some(nonce) = body["nonce"].as_str() {
        relay_uuid(nonce)?;
        sqlx::query("DELETE FROM client_audit_nonces WHERE seen_at<?")
            .bind(now() - 300)
            .execute(&mut *tx)
            .await?;
        let r=sqlx::query("INSERT INTO client_audit_nonces(device_id,nonce,seen_at) VALUES(?,?,?) ON CONFLICT DO NOTHING").bind(id).bind(nonce).bind(now()).execute(&mut *tx).await?;
        if r.rows_affected() == 0 {
            tx.commit().await?;
            return Ok(StatusCode::NO_CONTENT);
        }
    }
    // Only retain a documented, non-secret event label; the arbitrary payload may contain passwords or filenames.
    let action = match body["action"].as_str() {
        Some("new") => "opened",
        Some("close") => "closed",
        _ => "activity",
    };
    audit(
        &mut tx,
        id,
        &format!("client.{kind}.{action}"),
        id,
        "reported",
        "client report",
        "Client-reported activity; not independently verified",
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
pub(crate) async fn legacy_address_book(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let p = auth::native_principal(&s, &headers).await?;
    let (devices, _) = admin::visible_devices(
        &s,
        &p,
        &Page {
            limit: Some(100),
            ..Default::default()
        },
    )
    .await?;
    let peers: Vec<Value> = devices
        .into_iter()
        .map(|d| json!({"id":d.id,"alias":d.name,"platform":d.platform,"tags":[]}))
        .collect();
    Ok(Json(
        json!({"data":json!({"tags":[],"peers":peers}).to_string()}),
    ))
}
