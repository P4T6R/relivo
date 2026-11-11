use crate::{
    audit,
    auth::{self, Principal},
    invalid, not_found, now, ApiResult, AppState,
};
use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, SqlitePool};

#[derive(Deserialize, Default)]
pub(crate) struct Page {
    pub q: Option<String>,
    pub page: Option<i64>,
    pub limit: Option<i64>,
}
impl Page {
    fn limit(&self) -> i64 {
        self.limit.unwrap_or(25).clamp(1, 100)
    }
    fn offset(&self) -> i64 {
        (self.page.unwrap_or(1).clamp(1, 100000) - 1) * self.limit()
    }
    fn search(&self) -> String {
        self.q.as_deref().unwrap_or("").chars().take(100).collect()
    }
}
pub(crate) fn text(value: &str, max: usize) -> ApiResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(|c| c.is_control()) {
        return Err(invalid("Invalid or excessively long text"));
    }
    Ok(())
}
pub(crate) fn device_id(value: &str) -> ApiResult<()> {
    if !(6..=253).contains(&value.len())
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
    {
        return Err(invalid("Invalid RustDesk device ID"));
    }
    Ok(())
}
#[derive(Serialize, FromRow)]
pub(crate) struct User {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub enabled: bool,
    pub created_at: i64,
}
#[derive(Serialize, FromRow)]
pub(crate) struct Device {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub state: String,
    pub verified: bool,
    pub last_seen: Option<i64>,
    pub created_at: i64,
    pub note: String,
    pub public_key: String,
}
#[derive(Serialize, FromRow)]
pub(crate) struct Group {
    pub id: String,
    pub name: String,
}

pub(crate) const DEVICE_SCOPE:&str = "(?='admin' OR EXISTS (SELECT 1 FROM user_groups ug JOIN grants g ON g.subject_group=ug.group_id JOIN device_groups dg ON dg.group_id=g.target_group WHERE ug.user_id=? AND dg.device_id=d.id))";
const DEVICE_COLUMNS: &str =
    "d.id,d.name,d.platform,d.state,d.verified,d.last_seen,d.created_at,d.note,d.public_key";

pub(crate) async fn can_access(
    db: &SqlitePool,
    p: &Principal,
    id: &str,
    connect: bool,
) -> ApiResult<bool> {
    if connect && p.role == "viewer" {
        return Ok(false);
    }
    let n:i64=sqlx::query_scalar("SELECT count(*) FROM devices d WHERE d.id=? AND (?=0 OR (d.state='approved' AND d.verified=1)) AND (?='admin' OR EXISTS (SELECT 1 FROM user_groups ug JOIN grants g ON g.subject_group=ug.group_id JOIN device_groups dg ON dg.group_id=g.target_group WHERE ug.user_id=? AND dg.device_id=d.id AND (?=0 OR g.capability='connect')))")
        .bind(id).bind(connect).bind(&p.role).bind(&p.id).bind(connect).fetch_one(db).await?;
    Ok(n > 0)
}
pub(crate) async fn overview(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
) -> ApiResult<Json<Value>> {
    let sql=format!("SELECT count(*),coalesce(sum(CASE WHEN d.last_seen>? THEN 1 ELSE 0 END),0),coalesce(sum(CASE WHEN d.state='pending' THEN 1 ELSE 0 END),0) FROM devices d WHERE {DEVICE_SCOPE}");
    let (devices, online, pending): (i64, i64, i64) = sqlx::query_as(&sql)
        .bind(now() - 120)
        .bind(&p.role)
        .bind(&p.id)
        .fetch_one(&s.db)
        .await?;
    let users: i64 = if p.role == "admin" {
        sqlx::query_scalar("SELECT count(*) FROM users WHERE enabled=1")
            .fetch_one(&s.db)
            .await?
    } else {
        0
    };
    Ok(Json(
        json!({"devices":devices,"recently_seen":online,"pending":pending,"enabled_users":users,"server_time":now(),"connection_mode":"managed relay"}),
    ))
}
pub(crate) async fn users(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Query(page): Query<Page>,
) -> ApiResult<Json<Value>> {
    p.admin()?;
    let q = page.search();
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM users WHERE instr(lower(username||' '||display_name),lower(?))>0",
    )
    .bind(&q)
    .fetch_one(&s.db)
    .await?;
    let items=sqlx::query_as::<_,User>("SELECT id,username,display_name,role,enabled,created_at FROM users WHERE instr(lower(username||' '||display_name),lower(?))>0 ORDER BY username LIMIT ? OFFSET ?")
        .bind(q).bind(page.limit()).bind(page.offset()).fetch_all(&s.db).await?;
    Ok(Json(json!({"items":items,"total":total})))
}
#[derive(Deserialize)]
pub(crate) struct UserInput {
    username: Option<String>,
    display_name: String,
    role: String,
    enabled: bool,
    password: Option<String>,
}
pub(crate) async fn create_user(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Json(input): Json<UserInput>,
) -> ApiResult<Json<Value>> {
    p.admin()?;
    let username = input
        .username
        .ok_or_else(|| invalid("Username required"))?
        .to_ascii_lowercase();
    auth::validate_username(&username)?;
    text(&input.display_name, 120)?;
    auth::validate_role(&input.role)?;
    let password = input.password.ok_or_else(|| invalid("Password required"))?;
    auth::validate_password(&password)?;
    let _slot = s
        .hash_slots
        .try_acquire()
        .map_err(|_| invalid("Password service busy; try again"))?;
    let hash = auth::hash_password(password).await?;
    let id = auth::random_token();
    let mut tx = s.db.begin().await?;
    sqlx::query("INSERT INTO users(id,username,display_name,password_hash,role,enabled,created_at) VALUES(?,?,?,?,?,?,?)")
        .bind(&id).bind(&username).bind(input.display_name.trim()).bind(hash).bind(&input.role).bind(input.enabled).bind(now()).execute(&mut *tx).await?;
    audit(
        &mut tx,
        &p.username,
        "user.create",
        &username,
        "allowed",
        "administrator",
        &input.role,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id})))
}
pub(crate) async fn update_user(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(id): Path<String>,
    Json(input): Json<UserInput>,
) -> ApiResult<Json<Value>> {
    p.admin()?;
    text(&input.display_name, 120)?;
    auth::validate_role(&input.role)?;
    let hash = if let Some(password) = input.password.filter(|v| !v.is_empty()) {
        auth::validate_password(&password)?;
        let _slot = s
            .hash_slots
            .try_acquire()
            .map_err(|_| invalid("Password service busy; try again"))?;
        Some(auth::hash_password(password).await?)
    } else {
        None
    };
    let mut tx = s.db.begin().await?;
    let old: User = sqlx::query_as(
        "SELECT id,username,display_name,role,enabled,created_at FROM users WHERE id=?",
    )
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(not_found)?;
    if old.role == "admin" && old.enabled && (input.role != "admin" || !input.enabled) {
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM users WHERE role='admin' AND enabled=1")
                .fetch_one(&mut *tx)
                .await?;
        if count <= 1 {
            return Err(invalid(
                "The last enabled administrator cannot be disabled or demoted",
            ));
        }
    }
    sqlx::query("UPDATE users SET display_name=?,role=?,enabled=?,password_hash=coalesce(?,password_hash) WHERE id=?")
        .bind(input.display_name.trim()).bind(&input.role).bind(input.enabled).bind(&hash).bind(&id).execute(&mut *tx).await?;
    if hash.is_some() || !input.enabled || input.role != old.role {
        sqlx::query("DELETE FROM sessions WHERE user_id=?")
            .bind(&id)
            .execute(&mut *tx)
            .await?;
    }
    audit(
        &mut tx,
        &p.username,
        "user.update",
        &old.username,
        "allowed",
        "administrator",
        &format!("role={}, enabled={}", input.role, input.enabled),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id})))
}
pub(crate) async fn visible_devices(
    s: &AppState,
    p: &Principal,
    page: &Page,
) -> ApiResult<(Vec<Device>, i64)> {
    let filter=format!("FROM devices d WHERE {DEVICE_SCOPE} AND instr(lower(d.id||' '||d.name||' '||d.platform),lower(?))>0");
    let total: i64 = sqlx::query_scalar(&format!("SELECT count(*) {filter}"))
        .bind(&p.role)
        .bind(&p.id)
        .bind(page.search())
        .fetch_one(&s.db)
        .await?;
    let items = sqlx::query_as::<_, Device>(&format!(
        "SELECT {DEVICE_COLUMNS} {filter} ORDER BY d.name,d.id LIMIT ? OFFSET ?"
    ))
    .bind(&p.role)
    .bind(&p.id)
    .bind(page.search())
    .bind(page.limit())
    .bind(page.offset())
    .fetch_all(&s.db)
    .await?;
    Ok((items, total))
}
pub(crate) async fn devices(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Query(page): Query<Page>,
) -> ApiResult<Json<Value>> {
    let (items, total) = visible_devices(&s, &p, &page).await?;
    let mut values = Vec::new();
    for item in items {
        let connect = can_access(&s.db, &p, &item.id, true).await?;
        let mut v = json!(item);
        v["can_connect"] = json!(connect);
        values.push(v);
    }
    Ok(Json(json!({"items":values,"total":total})))
}
pub(crate) async fn device(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    if !can_access(&s.db, &p, &id, false).await? {
        return Err(not_found());
    }
    let item = sqlx::query_as::<_, Device>(&format!(
        "SELECT {DEVICE_COLUMNS} FROM devices d WHERE id=?"
    ))
    .bind(&id)
    .fetch_optional(&s.db)
    .await?
    .ok_or_else(not_found)?;
    Ok(Json(
        json!({"device":item,"can_connect":can_access(&s.db,&p,&id,true).await?}),
    ))
}
#[derive(Deserialize)]
pub(crate) struct DeviceInput {
    id: Option<String>,
    name: String,
    #[serde(default)]
    platform: String,
    #[serde(default)]
    note: String,
    state: Option<String>,
}
pub(crate) async fn create_device(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Json(input): Json<DeviceInput>,
) -> ApiResult<Json<Value>> {
    p.admin()?;
    let id = input.id.ok_or_else(|| invalid("Device ID required"))?;
    device_id(&id)?;
    text(&input.name, 120)?;
    if input.platform.len() > 120 || input.note.len() > 512 {
        return Err(invalid("Device details too long"));
    }
    let credential = auth::random_token();
    let mut tx = s.db.begin().await?;
    sqlx::query(
        "INSERT INTO devices(id,name,platform,note,credential_hash,created_at) VALUES(?,?,?,?,?,?)",
    )
    .bind(&id)
    .bind(input.name.trim())
    .bind(input.platform)
    .bind(input.note)
    .bind(auth::digest(&credential))
    .bind(now())
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        &p.username,
        "device.create",
        &id,
        "allowed",
        "administrator",
        "Enrollment pending",
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id,"credential":credential})))
}
pub(crate) async fn update_device(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(id): Path<String>,
    Json(input): Json<DeviceInput>,
) -> ApiResult<Json<Value>> {
    p.admin()?;
    text(&input.name, 120)?;
    if input.platform.len() > 120 || input.note.len() > 512 {
        return Err(invalid("Device details too long"));
    }
    let state = input.state.unwrap_or_else(|| "pending".into());
    if !["pending", "approved", "disabled"].contains(&state.as_str()) {
        return Err(invalid("Invalid device state"));
    }
    let mut tx = s.db.begin().await?;
    let verified: bool = sqlx::query_scalar("SELECT verified FROM devices WHERE id=?")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(not_found)?;
    if state == "approved" && !verified {
        return Err(invalid(
            "The device must report with its enrollment credential before approval",
        ));
    }
    sqlx::query("UPDATE devices SET name=?,platform=?,note=?,state=? WHERE id=?")
        .bind(input.name.trim())
        .bind(input.platform)
        .bind(input.note)
        .bind(&state)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        &p.username,
        "device.update",
        &id,
        "allowed",
        "administrator",
        &state,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id})))
}
pub(crate) async fn rotate_device_credential(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    p.admin()?;
    let credential = auth::random_token();
    let mut tx = s.db.begin().await?;
    let result=sqlx::query("UPDATE devices SET credential_hash=?,uuid='',public_key='',verified=0,state='pending',last_seen=NULL WHERE id=?").bind(auth::digest(&credential)).bind(&id).execute(&mut *tx).await?;
    if result.rows_affected() == 0 {
        return Err(not_found());
    }
    audit(
        &mut tx,
        &p.username,
        "device.credential.rotate",
        &id,
        "allowed",
        "administrator",
        "Re-enrollment required",
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"credential":credential})))
}
pub(crate) async fn groups(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Query(page): Query<Page>,
) -> ApiResult<Json<Value>> {
    let filter="FROM groups g WHERE (?='admin' OR EXISTS(SELECT 1 FROM user_groups ug WHERE ug.user_id=? AND ug.group_id=g.id) OR EXISTS(SELECT 1 FROM grants gr JOIN user_groups ug ON gr.subject_group=ug.group_id WHERE ug.user_id=? AND gr.target_group=g.id)) AND instr(lower(g.name),lower(?))>0";
    let total: i64 = sqlx::query_scalar(&format!("SELECT count(*) {filter}"))
        .bind(&p.role)
        .bind(&p.id)
        .bind(&p.id)
        .bind(page.search())
        .fetch_one(&s.db)
        .await?;
    let rows = sqlx::query_as::<_, Group>(&format!(
        "SELECT g.id,g.name {filter} ORDER BY g.name LIMIT ? OFFSET ?"
    ))
    .bind(&p.role)
    .bind(&p.id)
    .bind(&p.id)
    .bind(page.search())
    .bind(page.limit())
    .bind(page.offset())
    .fetch_all(&s.db)
    .await?;
    let mut items = Vec::new();
    for row in rows {
        let (users, devices): (Vec<String>, Vec<String>) = if p.role == "admin" {
            (
                sqlx::query_scalar(
                    "SELECT user_id FROM user_groups WHERE group_id=? ORDER BY user_id",
                )
                .bind(&row.id)
                .fetch_all(&s.db)
                .await?,
                sqlx::query_scalar(
                    "SELECT device_id FROM device_groups WHERE group_id=? ORDER BY device_id",
                )
                .bind(&row.id)
                .fetch_all(&s.db)
                .await?,
            )
        } else {
            (vec![], vec![])
        };
        items.push(json!({"id":row.id,"name":row.name,"user_ids":users,"device_ids":devices}));
    }
    Ok(Json(json!({"items":items,"total":total})))
}
#[derive(Deserialize)]
pub(crate) struct GroupInput {
    name: String,
    #[serde(default)]
    user_ids: Vec<String>,
    #[serde(default)]
    device_ids: Vec<String>,
}
async fn save_group(
    s: AppState,
    p: Principal,
    id: String,
    input: GroupInput,
    create: bool,
) -> ApiResult<Json<Value>> {
    p.admin()?;
    text(&input.name, 80)?;
    if input.user_ids.len() > 1000 || input.device_ids.len() > 1000 {
        return Err(invalid("Too many group members"));
    }
    let mut tx = s.db.begin().await?;
    if create {
        sqlx::query("INSERT INTO groups(id,name) VALUES(?,?)")
            .bind(&id)
            .bind(input.name.trim())
            .execute(&mut *tx)
            .await?;
    } else {
        let result = sqlx::query("UPDATE groups SET name=? WHERE id=?")
            .bind(input.name.trim())
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        if result.rows_affected() == 0 {
            return Err(not_found());
        }
    }
    sqlx::query("DELETE FROM user_groups WHERE group_id=?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM device_groups WHERE group_id=?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    for user in &input.user_ids {
        sqlx::query("INSERT INTO user_groups(user_id,group_id) VALUES(?,?)")
            .bind(user)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
    }
    for device in &input.device_ids {
        sqlx::query("INSERT INTO device_groups(device_id,group_id) VALUES(?,?)")
            .bind(device)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
    }
    audit(
        &mut tx,
        &p.username,
        if create {
            "group.create"
        } else {
            "group.update"
        },
        &id,
        "allowed",
        "administrator",
        &input.name,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id})))
}
pub(crate) async fn create_group(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Json(input): Json<GroupInput>,
) -> ApiResult<Json<Value>> {
    save_group(s, p, auth::random_token(), input, true).await
}
pub(crate) async fn update_group(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(id): Path<String>,
    Json(input): Json<GroupInput>,
) -> ApiResult<Json<Value>> {
    save_group(s, p, id, input, false).await
}
pub(crate) async fn delete_group(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    p.admin()?;
    let mut tx = s.db.begin().await?;
    let r = sqlx::query("DELETE FROM groups WHERE id=?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    if r.rows_affected() == 0 {
        return Err(not_found());
    }
    audit(
        &mut tx,
        &p.username,
        "group.delete",
        &id,
        "allowed",
        "administrator",
        "",
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize, Serialize, FromRow)]
pub(crate) struct Grant {
    id: Option<String>,
    subject_group: String,
    target_group: String,
    capability: String,
}
pub(crate) async fn grants(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Query(page): Query<Page>,
) -> ApiResult<Json<Value>> {
    p.admin()?;
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM grants")
        .fetch_one(&s.db)
        .await?;
    let items = sqlx::query_as::<_, Grant>("SELECT * FROM grants ORDER BY id LIMIT ? OFFSET ?")
        .bind(page.limit())
        .bind(page.offset())
        .fetch_all(&s.db)
        .await?;
    Ok(Json(json!({"items":items,"total":total})))
}
pub(crate) async fn create_grant(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Json(input): Json<Grant>,
) -> ApiResult<Json<Value>> {
    p.admin()?;
    if !["view", "connect"].contains(&input.capability.as_str()) {
        return Err(invalid("Unknown capability"));
    }
    let id = auth::random_token();
    let mut tx = s.db.begin().await?;
    sqlx::query("INSERT INTO grants(id,subject_group,target_group,capability) VALUES(?,?,?,?)")
        .bind(&id)
        .bind(&input.subject_group)
        .bind(&input.target_group)
        .bind(&input.capability)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        &p.username,
        "grant.create",
        &id,
        "allowed",
        "administrator",
        &format!(
            "{} -> {}: {}",
            input.subject_group, input.target_group, input.capability
        ),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id})))
}
pub(crate) async fn delete_grant(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    p.admin()?;
    let mut tx = s.db.begin().await?;
    let r = sqlx::query("DELETE FROM grants WHERE id=?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    if r.rows_affected() == 0 {
        return Err(not_found());
    }
    audit(
        &mut tx,
        &p.username,
        "grant.revoke",
        &id,
        "allowed",
        "administrator",
        "",
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Serialize, FromRow)]
struct Event {
    id: i64,
    time: i64,
    actor: String,
    action: String,
    target: String,
    outcome: String,
    source: String,
    detail: String,
}
pub(crate) async fn events(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Query(page): Query<Page>,
) -> ApiResult<Json<Value>> {
    p.admin()?;
    let q = page.search();
    let filter =
        "FROM audit WHERE instr(lower(actor||' '||action||' '||target||' '||outcome),lower(?))>0";
    let total: i64 = sqlx::query_scalar(&format!("SELECT count(*) {filter}"))
        .bind(&q)
        .fetch_one(&s.db)
        .await?;
    let items = sqlx::query_as::<_, Event>(&format!(
        "SELECT * {filter} ORDER BY id DESC LIMIT ? OFFSET ?"
    ))
    .bind(q)
    .bind(page.limit())
    .bind(page.offset())
    .fetch_all(&s.db)
    .await?;
    Ok(Json(json!({"items":items,"total":total})))
}
