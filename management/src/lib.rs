mod admin;
mod auth;
mod native;

use axum::{
    extract::{DefaultBodyLimit, Request, State},
    http::{HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Serialize;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    SqlitePool,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Semaphore;
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub origin: String,
    pub internal_key: String,
    pub secure_cookies: bool,
    pub hash_slots: Arc<Semaphore>,
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug)]
pub struct ApiError(pub StatusCode, pub &'static str);
pub(crate) type ApiResult<T> = Result<T, ApiError>;
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({"error": self.1}))).into_response()
    }
}
impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        if let sqlx::Error::Database(ref e) = error {
            if e.is_unique_violation() || e.is_foreign_key_violation() || e.is_check_violation() {
                return Self(
                    StatusCode::CONFLICT,
                    "The change conflicts with existing records",
                );
            }
        }
        eprintln!("Management database operation failed: {error}");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "The operation could not be completed",
        )
    }
}
pub(crate) fn invalid(message: &'static str) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, message)
}
pub(crate) fn denied() -> ApiError {
    ApiError(StatusCode::FORBIDDEN, "Access denied")
}
pub(crate) fn unauthorized() -> ApiError {
    ApiError(StatusCode::UNAUTHORIZED, "Sign in required")
}
pub(crate) fn not_found() -> ApiError {
    ApiError(StatusCode::NOT_FOUND, "Record not found")
}

pub async fn open_database(path: &Path) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&db)
        .await?;
    if version == 0 {
        let tables: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        )
        .fetch_one(&db)
        .await?;
        anyhow::ensure!(
            tables == 0,
            "Refusing to initialize an existing database with an unknown schema"
        );
        let mut tx = db.begin().await?;
        sqlx::raw_sql(include_str!("schema.sql"))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
    } else {
        anyhow::ensure!(
            version == 1,
            "Unsupported database schema; run the documented owner-managed upgrade"
        );
    }
    Ok(db)
}

pub async fn create_admin(db: &SqlitePool, username: &str, password: &str) -> anyhow::Result<()> {
    auth::validate_username(username).map_err(|e| anyhow::anyhow!(e.1))?;
    auth::validate_password(password).map_err(|e| anyhow::anyhow!(e.1))?;
    let hash = auth::hash_password(password.to_owned())
        .await
        .map_err(|e| anyhow::anyhow!(e.1))?;
    let mut tx = db.begin().await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&mut *tx)
        .await?;
    anyhow::ensure!(
        count == 0,
        "Bootstrap is only allowed for an empty user database"
    );
    sqlx::query("INSERT INTO users(id,username,display_name,password_hash,role,created_at) VALUES(?,?,?,?, 'admin',?)")
        .bind(auth::random_token()).bind(username).bind(username).bind(hash).bind(now()).execute(&mut *tx).await?;
    audit(
        &mut tx,
        username,
        "bootstrap",
        "users",
        "allowed",
        "administrator",
        "Initial administrator created",
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.1))?;
    tx.commit().await?;
    Ok(())
}

pub(crate) async fn audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    actor: &str,
    action: &str,
    target: &str,
    outcome: &str,
    source: &str,
    detail: &str,
) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO audit(time,actor,action,target,outcome,source,detail) VALUES(?,?,?,?,?,?,?)",
    )
    .bind(now())
    .bind(actor)
    .bind(action)
    .bind(target)
    .bind(outcome)
    .bind(source)
    .bind(detail)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
}

async fn health(State(state): State<AppState>) -> ApiResult<Json<Health>> {
    sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .map_err(|_| ApiError(StatusCode::SERVICE_UNAVAILABLE, "Database unavailable"))?;
    Ok(Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    }))
}

pub fn public_router(state: AppState, web: Option<PathBuf>) -> Router {
    let management = Router::new()
        .route("/session", get(auth::session).delete(auth::logout))
        .route("/overview", get(admin::overview))
        .route("/users", get(admin::users).post(admin::create_user))
        .route("/users/{id}", put(admin::update_user))
        .route("/devices", get(admin::devices).post(admin::create_device))
        .route(
            "/devices/{id}",
            get(admin::device).put(admin::update_device),
        )
        .route(
            "/devices/{id}/credential",
            post(admin::rotate_device_credential),
        )
        .route("/groups", get(admin::groups).post(admin::create_group))
        .route(
            "/groups/{id}",
            put(admin::update_group).delete(admin::delete_group),
        )
        .route("/grants", get(admin::grants).post(admin::create_grant))
        .route("/grants/{id}", delete(admin::delete_grant))
        .route("/audit", get(admin::events))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::web_auth,
        ));
    let mut app = Router::new()
        .route("/healthz", get(health))
        .route("/management/v1/login", post(auth::web_login))
        .nest("/management/v1", management)
        .route("/api/login", post(auth::native_login))
        .route("/api/currentUser", post(native::current_user))
        .route("/api/logout", post(native::logout))
        .route(
            "/api/login-options",
            get(|| async { Json(Vec::<String>::new()) }),
        )
        .route("/api/users", get(native::users))
        .route("/api/peers", get(native::peers))
        .route("/api/device-group/accessible", get(native::groups))
        .route("/api/heartbeat", post(native::heartbeat))
        .route("/api/sysinfo", post(native::sysinfo))
        .route(
            "/api/sysinfo_ver",
            post(|| async { Json(serde_json::json!({"version":"1"})) }),
        )
        .route("/api/management/relay-ticket", post(native::relay_ticket))
        .route("/api/audit/{kind}", post(native::client_audit))
        .route(
            "/api/ab",
            get(native::legacy_address_book).post(|| async {
                ApiError(
                    StatusCode::FORBIDDEN,
                    "This address book is managed in the web console",
                )
            }),
        );
    if let Some(web) = web {
        app = app.fallback_service(ServeDir::new(&web));
    }
    app.layer(DefaultBodyLimit::max(64 * 1024))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            security_headers,
        ))
        .with_state(state)
}

pub fn internal_router(state: AppState) -> Router {
    Router::new()
        .route("/authorize", post(native::authorize))
        .route("/relay/claim", post(native::claim_relay))
        .route("/relay/event", post(native::relay_event))
        .layer(DefaultBodyLimit::max(8192))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::internal_auth,
        ))
        .with_state(state)
}

async fn security_headers(State(_state): State<AppState>, req: Request, next: Next) -> Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert("cache-control", HeaderValue::from_static("no-store"));
    headers.insert("content-security-policy", HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"));
    response
}
