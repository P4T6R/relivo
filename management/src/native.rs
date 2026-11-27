use crate::{
    admin::{self, Page},
    audit, auth, denied, invalid, now, ApiError, ApiResult, AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

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
    device_report(&s, &headers, body, false).await?;
    Ok(Json(json!({"modified_at":0,"version":"1"})))
}
/// The RustDesk client compares this body with the literal `SYSINFO_UPDATED`;
/// any other 2xx body makes it re-upload system information indefinitely.
pub(crate) async fn sysinfo(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult<&'static str> {
    device_report(&s, &headers, body, true).await?;
    Ok("SYSINFO_UPDATED")
}
/// The client posts an empty body and compares the raw response with the
/// version it stored after its last accepted upload. The value is derived from
/// the credential, so re-enrollment with a rotated credential forces an upload.
pub(crate) async fn sysinfo_version(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<String> {
    let credential = auth::digest(auth::bearer(&headers)?);
    let state: Option<String> =
        sqlx::query_scalar("SELECT state FROM devices WHERE credential_hash=?")
            .bind(&credential)
            .fetch_optional(&s.db)
            .await?;
    match state.as_deref() {
        None => Err(ApiError(StatusCode::UNAUTHORIZED, "device credential invalid")),
        Some("disabled") => Err(ApiError(StatusCode::FORBIDDEN, "device disabled")),
        Some(_) => Ok(format!(
            "1-{}",
            &auth::digest(&format!("sysinfo:{credential}"))[..16]
        )),
    }
}

/// A rejected agent retries every ~15 s; record at most one audit event per
/// device and reason in this window. The device row always holds the latest one.
const REJECTION_AUDIT_WINDOW: i64 = 600;
/// Unauthenticated rejections are only logged, globally limited to one line per minute.
static LAST_UNKNOWN_LOG: AtomicI64 = AtomicI64::new(0);
static UNKNOWN_SUPPRESSED: AtomicU64 = AtomicU64::new(0);
fn log_unknown_credential(reported_id: &str) {
    let t = now();
    let last = LAST_UNKNOWN_LOG.load(Ordering::Relaxed);
    if t - last < 60
        || LAST_UNKNOWN_LOG
            .compare_exchange(last, t, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
    {
        UNKNOWN_SUPPRESSED.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let suppressed = UNKNOWN_SUPPRESSED.swap(0, Ordering::Relaxed);
    // `reported_id` has passed `admin::device_id` (ASCII alphanumerics, `_`, `-`).
    eprintln!(
        "Device report rejected: unknown enrollment credential (reported ID {reported_id}; {suppressed} similar rejection(s) not logged)"
    );
}

#[derive(sqlx::FromRow)]
struct ReportingDevice {
    id: String,
    uuid: String,
    public_key: String,
    state: String,
    verified: bool,
    last_rejected_at: Option<i64>,
    last_rejection: String,
}
/// Stores the rejection on the device that owns the presented credential and
/// audits identity problems, de-duplicated per `REJECTION_AUDIT_WINDOW`.
async fn reject_report(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device: &ReportingDevice,
    reason: &str,
    detail: Option<String>,
) -> ApiResult<()> {
    let t = now();
    let recently_recorded = device.last_rejection == reason
        && device
            .last_rejected_at
            .is_some_and(|at| t - at < REJECTION_AUDIT_WINDOW);
    sqlx::query("UPDATE devices SET last_rejected_at=?,last_rejection=? WHERE id=?")
        .bind(t)
        .bind(reason)
        .bind(&device.id)
        .execute(&mut **tx)
        .await?;
    if let Some(detail) = detail.filter(|_| !recently_recorded) {
        audit(
            tx,
            &device.id,
            "device.identity_mismatch",
            &device.id,
            "denied",
            "device credential",
            &detail,
        )
        .await?;
    }
    Ok(())
}
async fn device_report(
    s: &AppState,
    headers: &HeaderMap,
    body: Value,
    sysinfo: bool,
) -> ApiResult<()> {
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
    let platform = if sysinfo {
        let platform = body["os"].as_str().unwrap_or("");
        if platform.len() > 120 {
            return Err(invalid("OS description too long"));
        }
        Some(platform)
    } else {
        None
    };
    let mut tx = s.db.begin().await?;
    // The credential hash is unique: it identifies the enrolled record even when
    // the agent reports another ID, which is the usual misconfiguration.
    let device: Option<ReportingDevice> = sqlx::query_as(
        "SELECT id,uuid,public_key,state,verified,last_rejected_at,last_rejection FROM devices WHERE credential_hash=?",
    )
    .bind(&credential)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(device) = device else {
        drop(tx);
        log_unknown_credential(id);
        return Err(ApiError(StatusCode::UNAUTHORIZED, "device credential invalid"));
    };
    if device.id != id {
        reject_report(
            &mut tx,
            &device,
            "id_mismatch",
            Some(format!(
                "This device's credential was presented with another device ID ({id}); install it on the matching computer"
            )),
        )
        .await?;
        tx.commit().await?;
        return Err(ApiError(StatusCode::UNAUTHORIZED, "device ID does not match the credential"));
    }
    if device.state == "disabled" {
        reject_report(&mut tx, &device, "disabled", None).await?;
        tx.commit().await?;
        return Err(ApiError(StatusCode::FORBIDDEN, "device disabled"));
    }
    let uuid_changed = !device.uuid.is_empty() && device.uuid != uuid;
    let pk_changed = !device.public_key.is_empty() && device.public_key != pk;
    if uuid_changed || pk_changed {
        let (reason, what) = match (uuid_changed, pk_changed) {
            (true, true) => ("uuid_public_key", "UUID and public key"),
            (true, false) => ("uuid", "UUID"),
            _ => ("public_key", "public key"),
        };
        reject_report(
            &mut tx,
            &device,
            reason,
            Some(format!(
                "Agent {what} differs from the pinned identity (reinstalled agent?); rotate the credential and re-enroll"
            )),
        )
        .await?;
        tx.commit().await?;
        return Err(ApiError(StatusCode::FORBIDDEN, "device identity changed; re-enrollment required"));
    }
    // Clear a stale rejection once the conflicting agent has been quiet for a full
    // window; a second agent still using the credential keeps it visible.
    sqlx::query("UPDATE devices SET uuid=?,public_key=?,verified=1,last_seen=?,platform=coalesce(?,platform),last_rejection=CASE WHEN last_rejected_at IS NULL OR last_rejected_at<=? THEN '' ELSE last_rejection END,last_rejected_at=CASE WHEN last_rejected_at IS NULL OR last_rejected_at<=? THEN NULL ELSE last_rejected_at END WHERE id=?")
        .bind(uuid)
        .bind(pk)
        .bind(now())
        .bind(platform)
        .bind(now() - REJECTION_AUDIT_WINDOW)
        .bind(now() - REJECTION_AUDIT_WINDOW)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if !device.verified {
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
    Ok(())
}

async fn credential_can_connect(
    conn: &mut sqlx::SqliteConnection,
    hash: &str,
    target: &str,
) -> ApiResult<bool> {
    credential_allowed_since(conn, hash, target, now()).await
}
/// How long an already paired relay may outlive its controller's sign-in expiry.
pub(crate) const ACTIVE_RELAY_GRACE: i64 = 86_400;
/// The grant check of `credential_can_connect`, for a sign-in valid after `since`.
async fn credential_allowed_since(
    conn: &mut sqlx::SqliteConnection,
    hash: &str,
    target: &str,
    since: i64,
) -> ApiResult<bool> {
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM sessions s JOIN users u ON u.id=s.user_id JOIN devices d ON d.id=? WHERE s.token_hash=? AND s.kind='native' AND s.expires_at>? AND u.enabled=1 AND u.role IN ('admin','operator') AND d.state='approved' AND d.verified=1 AND (u.role='admin' OR EXISTS(SELECT 1 FROM user_groups ug JOIN grants g ON g.subject_group=ug.group_id JOIN device_groups dg ON dg.group_id=g.target_group WHERE ug.user_id=u.id AND dg.device_id=d.id AND g.capability='connect'))")
        .bind(target).bind(hash).bind(since).fetch_one(conn).await?;
    Ok(count > 0)
}
fn forbidden(reason: &'static str) -> ApiError {
    ApiError(StatusCode::FORBIDDEN, reason)
}
fn device_state_denial(state: &str, verified: bool) -> Option<ApiError> {
    if !verified {
        Some(forbidden("device not verified"))
    } else if state == "disabled" {
        Some(forbidden("device disabled"))
    } else if state != "approved" {
        Some(forbidden("device not approved"))
    } else {
        None
    }
}
/// `None` when the native session may connect to `target`. `credential_can_connect`
/// stays the only allow decision; the rest only explains a refusal. A user without
/// a connect grant learns nothing about the target's existence or state.
async fn controller_denial(
    conn: &mut sqlx::SqliteConnection,
    hash: &str,
    target: &str,
) -> ApiResult<Option<ApiError>> {
    if credential_can_connect(&mut *conn, hash, target).await? {
        return Ok(None);
    }
    let user: Option<(String, String)> = sqlx::query_as("SELECT u.id,u.role FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash=? AND s.kind='native' AND s.expires_at>? AND u.enabled=1")
        .bind(hash).bind(now()).fetch_optional(&mut *conn).await?;
    let Some((user_id, role)) = user else {
        return Ok(Some(ApiError(StatusCode::UNAUTHORIZED, "session expired")));
    };
    if role == "viewer" {
        return Ok(Some(forbidden("viewer role cannot connect")));
    }
    if role != "admin" {
        let granted:i64=sqlx::query_scalar("SELECT count(*) FROM user_groups ug JOIN grants g ON g.subject_group=ug.group_id JOIN device_groups dg ON dg.group_id=g.target_group WHERE ug.user_id=? AND dg.device_id=? AND g.capability='connect'")
            .bind(&user_id).bind(target).fetch_one(&mut *conn).await?;
        if granted == 0 {
            return Ok(Some(forbidden("no connect grant")));
        }
    }
    let device: Option<(String, bool)> =
        sqlx::query_as("SELECT state,verified FROM devices WHERE id=?")
            .bind(target)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(Some(match device {
        None => forbidden("device not enrolled"),
        Some((state, verified)) => {
            device_state_denial(&state, verified).unwrap_or_else(denied)
        }
    }))
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
    let denial = if input.conn_type != 0 {
        Some(forbidden("connection type not allowed"))
    } else if input.token.is_empty() {
        Some(ApiError(StatusCode::UNAUTHORIZED, "sign in required"))
    } else if let Some(denial) = controller_denial(&mut tx, &hash, &input.target).await? {
        Some(denial)
    } else if input.public_key.is_empty() || pk.as_deref() != Some(input.public_key.as_str()) {
        Some(forbidden("device identity mismatch"))
    } else {
        None
    };
    // The detail holds the attempt ID and a fixed reason string, never the token.
    let detail = match &denial {
        Some(e) => format!("{}: {}", input.attempt, e.1),
        None => input.attempt.clone(),
    };
    audit(
        &mut tx,
        actor.as_deref().unwrap_or("unauthenticated"),
        "connection.authorize",
        &input.target,
        if denial.is_none() { "allowed" } else { "denied" },
        "rendezvous",
        &detail,
    )
    .await?;
    tx.commit().await?;
    if let Some(denial) = denial {
        return Err(denial);
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
    let denial = if input.role == "controller" {
        controller_denial(&mut tx, &credential, &input.target).await?
    } else {
        let device: Option<(String, bool)> = sqlx::query_as(
            "SELECT state,verified FROM devices WHERE id=? AND credential_hash=?",
        )
        .bind(&input.target)
        .bind(&credential)
        .fetch_optional(&mut *tx)
        .await?;
        match device {
            None => Some(ApiError(StatusCode::UNAUTHORIZED, "device credential invalid")),
            Some((state, verified)) => device_state_denial(&state, verified),
        }
    };
    if let Some(denial) = denial {
        return Err(denial);
    }
    sqlx::query("DELETE FROM relay_tickets WHERE expires_at<=?")
        .bind(now())
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM relay_attempts WHERE expires_at<=? AND uuid NOT IN(SELECT uuid FROM relay_tickets)").bind(now()-86400).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO relay_attempts(uuid,device_id,expires_at) VALUES(?,?,?) ON CONFLICT(uuid) DO NOTHING").bind(&input.uuid).bind(&input.target).bind(now()+60).execute(&mut *tx).await?;
    let (target,expiry,controller,device):(String,i64,i64,i64)=sqlx::query_as("SELECT device_id,expires_at,controller_claimed,device_claimed FROM relay_attempts WHERE uuid=?").bind(&input.uuid).fetch_one(&mut *tx).await?;
    if target != input.target {
        return Err(forbidden("relay attempt belongs to another device"));
    }
    if expiry <= now() {
        return Err(forbidden("relay attempt expired"));
    }
    if (input.role == "controller" && controller != 0) || (input.role == "device" && device != 0) {
        return Err(forbidden("relay role already admitted"));
    }
    let outstanding: i64 =
        sqlx::query_scalar("SELECT count(*) FROM relay_tickets WHERE credential_hash=?")
            .bind(&credential)
            .fetch_one(&mut *tx)
            .await?;
    if outstanding >= 20 {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many pending relay tickets",
        ));
    }
    let ticket = auth::random_token();
    // A ticket can never outlive its attempt: claims also require a live attempt.
    let ticket_expiry = (now() + 30).min(expiry);
    sqlx::query("INSERT INTO relay_tickets(hash,uuid,device_id,role,credential_hash,expires_at) VALUES(?,?,?,?,?,?)")
        .bind(auth::digest(&ticket)).bind(&input.uuid).bind(&target).bind(&input.role).bind(credential).bind(ticket_expiry).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"ticket":ticket,"expires_in":(ticket_expiry - now()).max(0)}),
    ))
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
    if !["paired", "active", "ended"].contains(&input.event.as_str()) {
        return Err(invalid("Unknown relay event"));
    }
    let mut tx = s.db.begin().await?;
    let target:Option<String>=sqlx::query_scalar("SELECT device_id FROM relay_attempts WHERE uuid=? AND controller_claimed=1 AND device_claimed=1").bind(&input.uuid).fetch_optional(&mut *tx).await?;
    let target = target.ok_or_else(denied)?;
    if input.event == "active" {
        // hbbr re-checks a running session: a revoked sign-in or grant, a disabled user
        // or device, or a rotated device credential ends it. Only the end is audited.
        let (controller, device): (String, String) = sqlx::query_as(
            "SELECT controller_hash,device_hash FROM relay_attempts WHERE uuid=?",
        )
        .bind(&input.uuid)
        .fetch_one(&mut *tx)
        .await?;
        let valid_device:i64=sqlx::query_scalar("SELECT count(*) FROM devices WHERE id=? AND credential_hash=? AND state='approved' AND verified=1").bind(&target).bind(device).fetch_one(&mut *tx).await?;
        if valid_device > 0
            && credential_allowed_since(&mut tx, &controller, &target, now() - ACTIVE_RELAY_GRACE)
                .await?
        {
            return Ok(Json(json!({"ok":true})));
        }
        audit(&mut tx, "relay", "relay.revoked", &target, "denied", "relay", &input.uuid).await?;
        tx.commit().await?;
        return Err(denied());
    }
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
/// Activity reports one approved device may add per minute.
const CLIENT_AUDIT_PER_MINUTE: i64 = 120;
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
    // A compromised device must not bury real events or fill the disk.
    sqlx::query("DELETE FROM login_limits WHERE resets_at<=?")
        .bind(now())
        .execute(&mut *tx)
        .await?;
    let bucket = format!("client-audit:{}", auth::digest(id));
    sqlx::query("INSERT INTO login_limits(bucket,count,resets_at) VALUES(?,1,?) ON CONFLICT(bucket) DO UPDATE SET count=count+1")
        .bind(&bucket).bind(now() + 60).execute(&mut *tx).await?;
    let reports: i64 = sqlx::query_scalar("SELECT count FROM login_limits WHERE bucket=?")
        .bind(&bucket)
        .fetch_one(&mut *tx)
        .await?;
    if reports > CLIENT_AUDIT_PER_MINUTE {
        tx.commit().await?;
        return Err(ApiError(StatusCode::TOO_MANY_REQUESTS, "Too many activity reports"));
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
// The native client picks its peer icon from these exact names; reports carry a full OS label.
fn peer_platform(os: &str) -> &'static str {
    let os = os.to_ascii_lowercase();
    if os.contains("windows") {
        "Windows"
    } else if os.contains("android") {
        "Android"
    } else if os.contains("mac") || os.contains("darwin") {
        "Mac OS"
    } else if os.contains("linux") {
        "Linux"
    } else {
        ""
    }
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
        .map(|d| json!({"id":d.id,"alias":d.name,"platform":peer_platform(&d.platform),"tags":[]}))
        .collect();
    Ok(Json(
        json!({"data":json!({"tags":[],"peers":peers}).to_string()}),
    ))
}
