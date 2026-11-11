use anyhow::{bail, Context, Result};
use rustdesk_management::{create_admin, internal_router, open_database, public_router, AppState};
use std::{
    env,
    io::{self, Read},
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
};
use tokio::{net::TcpListener, sync::Semaphore};

#[tokio::main]
async fn main() -> Result<()> {
    let database = PathBuf::from(
        env::var("MANAGEMENT_DATABASE").unwrap_or_else(|_| "data/management.sqlite3".into()),
    );
    if let Some(parent) = database.parent().filter(|p| !p.as_os_str().is_empty()) {
        tokio::fs::create_dir_all(parent).await?;
    }
    let db = open_database(&database).await?;
    let args: Vec<String> = env::args().collect();
    if args.get(1).map(String::as_str) == Some("init-admin") {
        let username = args
            .get(2)
            .context("Usage: rustdesk-management init-admin USERNAME < password-file")?;
        let mut password = String::new();
        io::stdin().take(130).read_to_string(&mut password)?;
        create_admin(&db, username, password.trim_end_matches(['\r', '\n'])).await?;
        println!("Administrator created. No server started.");
        return Ok(());
    }
    if args.len() > 1 {
        bail!("Unknown command");
    }
    let listen: SocketAddr = env::var("MANAGEMENT_LISTEN")
        .unwrap_or_else(|_| "127.0.0.1:21114".into())
        .parse()?;
    let internal: SocketAddr = env::var("MANAGEMENT_INTERNAL_LISTEN")
        .unwrap_or_else(|_| "127.0.0.1:21120".into())
        .parse()?;
    anyhow::ensure!(
        internal.ip().is_loopback(),
        "The internal API must bind to loopback"
    );
    let origin = env::var("MANAGEMENT_ORIGIN").unwrap_or_else(|_| format!("http://{listen}"));
    let secure = origin.starts_with("https://");
    anyhow::ensure!(
        secure || (listen.ip().is_loopback() && origin == format!("http://{listen}")),
        "Use an HTTPS origin outside local development"
    );
    let key_path = env::var("MANAGEMENT_INTERNAL_KEY_FILE")
        .context("Set MANAGEMENT_INTERNAL_KEY_FILE to a private shared secret file")?;
    let key = tokio::fs::read_to_string(key_path).await?.trim().to_owned();
    anyhow::ensure!(
        key.len() >= 32,
        "Internal key must contain at least 32 characters"
    );
    let state = AppState {
        db,
        origin,
        internal_key: key,
        secure_cookies: secure,
        hash_slots: Arc::new(Semaphore::new(2)),
    };
    let web = PathBuf::from(
        env::var("MANAGEMENT_WEB_DIR").unwrap_or_else(|_| "management/web/dist".into()),
    );
    anyhow::ensure!(
        web.join("index.html").exists(),
        "Build the web console and set MANAGEMENT_WEB_DIR"
    );
    let listener = TcpListener::bind(listen).await?;
    let internal_listener = TcpListener::bind(internal).await?;
    println!("Console: {} | internal API: {}", state.origin, internal);
    let public_state = state.clone();
    let public = async {
        axum::serve(
            listener,
            public_router(public_state, Some(web))
                .into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown())
        .await
    };
    let private = async {
        axum::serve(internal_listener, internal_router(state))
            .with_graceful_shutdown(shutdown())
            .await
    };
    tokio::try_join!(public, private)?;
    Ok(())
}
async fn shutdown() {
    if let Err(e) = tokio::signal::ctrl_c().await {
        eprintln!("Shutdown signal unavailable: {e}");
    }
}
