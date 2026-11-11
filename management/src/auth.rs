use crate::{audit, denied, invalid, now, unauthorized, ApiError, ApiResult, AppState};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    body::Bytes,
    extract::{ConnectInfo, Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Extension, Json,
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use subtle::ConstantTimeEq;

pub(crate) fn random_token() -> String {
    let mut bytes = [0; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}
pub(crate) fn digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
pub(crate) fn constant_eq(a: &str, b: &str) -> bool {
    bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}
pub(crate) fn validate_username(value: &str) -> ApiResult<()> {
    if !(3..=64).contains(&value.len())
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-@".contains(&c))
    {
        return Err(invalid(
            "Use 3–64 letters, numbers, dots, hyphens, underscores or @ for the username",
        ));
    }
    Ok(())
}
pub(crate) fn validate_password(value: &str) -> ApiResult<()> {
    if !(12..=128).contains(&value.len()) {
        return Err(invalid("Use a password between 12 and 128 bytes"));
    }
    Ok(())
}
pub(crate) fn validate_role(role: &str) -> ApiResult<()> {
    if !["admin", "operator", "viewer"].contains(&role) {
        return Err(invalid("Unknown role"));
    }
    Ok(())
}
pub(crate) async fn hash_password(password: String) -> ApiResult<String> {
    tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
            .map(|v| v.to_string())
            .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "Password hashing failed"))
    })
    .await
    .map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Password worker unavailable",
        )
    })?
}

#[derive(Clone, Serialize, sqlx::FromRow)]
pub(crate) struct Principal {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: String,
    #[serde(skip)]
    pub token_hash: String,
    #[serde(skip)]
    pub csrf: String,
}
impl Principal {
    pub fn admin(&self) -> ApiResult<()> {
        if self.role == "admin" {
            Ok(())
        } else {
            Err(denied())
        }
    }
    pub fn native_payload(&self) -> Value {
        json!({"name":self.username,"display_name":self.display_name,"status":1,"is_admin":self.role=="admin"})
    }
}

pub(crate) async fn session_by_hash(s: &AppState, hash: &str, kind: &str) -> ApiResult<Principal> {
    sqlx::query_as::<_,Principal>("SELECT u.id,u.username,u.display_name,u.role,s.token_hash,s.csrf FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash=? AND s.kind=? AND s.expires_at>? AND u.enabled=1")
        .bind(hash).bind(kind).bind(now()).fetch_optional(&s.db).await?.ok_or_else(unauthorized)
}
pub(crate) fn bearer(headers: &HeaderMap) -> ApiResult<&str> {
    let value = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(unauthorized)?;
    if value.len() != 64 || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(unauthorized());
    }
    Ok(value)
}
pub(crate) async fn native_principal(s: &AppState, headers: &HeaderMap) -> ApiResult<Principal> {
    session_by_hash(s, &digest(bearer(headers)?), "native").await
}
fn cookie(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| part.trim().strip_prefix("rd_session="))
}
pub(crate) fn origin(s: &AppState, headers: &HeaderMap) -> ApiResult<()> {
    if headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) != Some(s.origin.as_str()) {
        return Err(denied());
    }
    Ok(())
}
pub(crate) async fn web_auth(
    State(s): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let raw = cookie(req.headers()).ok_or_else(unauthorized)?;
    if raw.len() != 64 {
        return Err(unauthorized());
    }
    let p = session_by_hash(&s, &digest(raw), "web").await?;
    if !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        origin(&s, req.headers())?;
        let csrf = req
            .headers()
            .get("x-csrf-token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !constant_eq(csrf, &p.csrf) {
            return Err(denied());
        }
    }
    req.extensions_mut().insert(p);
    Ok(next.run(req).await)
}
pub(crate) async fn internal_auth(
    State(s): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let key = req
        .headers()
        .get("x-management-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if s.internal_key.len() < 32 || !constant_eq(key, &s.internal_key) {
        return Err(unauthorized());
    }
    Ok(next.run(req).await)
}

#[derive(Deserialize)]
struct Login {
    username: String,
    password: String,
}
async fn login(s: &AppState, raw: Bytes, ip: &str, kind: &str) -> ApiResult<(Principal, String)> {
    let input: Login =
        serde_json::from_slice(&raw).map_err(|_| invalid("Invalid login request"))?;
    validate_username(&input.username)?;
    if input.password.len() > 128 {
        return Err(unauthorized());
    }
    let username = input.username.to_ascii_lowercase();
    let mut tx = s.db.begin().await?;
    sqlx::query("DELETE FROM login_limits WHERE resets_at<?")
        .bind(now())
        .execute(&mut *tx)
        .await?;
    for (bucket, limit) in [
        (format!("account:{}", digest(&username)), 8),
        (format!("ip:{}", digest(ip)), 40),
    ] {
        sqlx::query("INSERT INTO login_limits(bucket,count,resets_at) VALUES(?,1,?) ON CONFLICT(bucket) DO UPDATE SET count=count+1")
            .bind(&bucket).bind(now()+60).execute(&mut *tx).await?;
        let count: i64 = sqlx::query_scalar("SELECT count FROM login_limits WHERE bucket=?")
            .bind(bucket)
            .fetch_one(&mut *tx)
            .await?;
        if count > limit {
            tx.commit().await?;
            return Err(ApiError(
                StatusCode::TOO_MANY_REQUESTS,
                "Too many attempts; try again in a minute",
            ));
        }
    }
    tx.commit().await?;
    let _slot = s.hash_slots.try_acquire().map_err(|_| {
        ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Sign-in service busy; try again",
        )
    })?;
    let user: Option<(String, String, i64)> =
        sqlx::query_as("SELECT id,password_hash,enabled FROM users WHERE username=?")
            .bind(&username)
            .fetch_optional(&s.db)
            .await?;
    let verified = if let Some((_, hash, _)) = &user {
        let hash = hash.clone();
        let password = input.password;
        tokio::task::spawn_blocking(move || {
            PasswordHash::new(&hash)
                .map(|v| {
                    Argon2::default()
                        .verify_password(password.as_bytes(), &v)
                        .is_ok()
                })
                .unwrap_or(false)
        })
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Password worker unavailable",
            )
        })?
    } else {
        hash_password("nonexistent-account-padding".to_owned()).await?;
        false
    };
    let mut tx = s.db.begin().await?;
    if !verified || !matches!(user, Some((_, _, 1))) {
        audit(
            &mut tx,
            &username,
            "login",
            "session",
            "denied",
            "authentication",
            "Invalid credentials",
        )
        .await?;
        tx.commit().await?;
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "Invalid username or password",
        ));
    }
    let (id, verified_hash, _) = user.ok_or_else(unauthorized)?;
    let token = random_token();
    let hash = digest(&token);
    let csrf = random_token();
    sqlx::query("DELETE FROM sessions WHERE expires_at<=?")
        .bind(now())
        .execute(&mut *tx)
        .await?;
    // A password reset or disable may happen while Argon2 runs; re-check the account.
    let unchanged: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM users WHERE id=? AND enabled=1 AND password_hash=?",
    )
    .bind(&id)
    .bind(verified_hash)
    .fetch_one(&mut *tx)
    .await?;
    if unchanged == 0 {
        return Err(unauthorized());
    }
    sqlx::query("INSERT INTO sessions(token_hash,user_id,csrf,kind,expires_at) VALUES(?,?,?,?,?)")
        .bind(&hash)
        .bind(&id)
        .bind(&csrf)
        .bind(kind)
        .bind(now() + 8 * 3600)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        &username,
        "login",
        "session",
        "allowed",
        "authentication",
        kind,
    )
    .await?;
    tx.commit().await?;
    Ok((session_by_hash(s, &hash, kind).await?, token))
}
pub(crate) async fn web_login(
    State(s): State<AppState>,
    remote: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    raw: Bytes,
) -> ApiResult<Response> {
    origin(&s, &headers)?;
    let ip = remote
        .map(|v| v.0 .0.ip().to_string())
        .unwrap_or_else(|| "local".into());
    let (p, token) = login(&s, raw, &ip, "web").await?;
    let mut response = Json(json!({"user":p,"csrf":p.csrf})).into_response();
    let secure = if s.secure_cookies { "; Secure" } else { "" };
    let cookie =
        format!("rd_session={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=28800{secure}");
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(|_| invalid("Invalid cookie"))?,
    );
    Ok(response)
}
pub(crate) async fn native_login(
    State(s): State<AppState>,
    remote: Option<Extension<ConnectInfo<SocketAddr>>>,
    raw: Bytes,
) -> ApiResult<Json<Value>> {
    let ip = remote
        .map(|v| v.0 .0.ip().to_string())
        .unwrap_or_else(|| "local".into());
    let (p, token) = login(&s, raw, &ip, "native").await?;
    Ok(Json(
        json!({"type":"access_token","access_token":token,"user":p.native_payload()}),
    ))
}
pub(crate) async fn session(Extension(p): Extension<Principal>) -> Json<Value> {
    Json(json!({"user":p,"csrf":p.csrf}))
}
pub(crate) async fn revoke(s: &AppState, p: &Principal) -> ApiResult<()> {
    let mut tx = s.db.begin().await?;
    sqlx::query("DELETE FROM sessions WHERE token_hash=?")
        .bind(&p.token_hash)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        &p.username,
        "logout",
        "session",
        "allowed",
        "authentication",
        "",
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
pub(crate) async fn logout(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
) -> ApiResult<Response> {
    revoke(&s, &p).await?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static("rd_session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0"),
    );
    Ok(response)
}
