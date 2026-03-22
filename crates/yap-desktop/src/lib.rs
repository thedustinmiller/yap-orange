//! yap-desktop — orchestration library.
//!
//! Starts an embedded database, runs migrations, launches the Axum HTTP server
//! on a random port, then opens a Tauri webview pointed at that port.
//! On close everything shuts down cleanly.
//!
//! **Features:**
//! - `sqlite` (default) — In-process SQLite. Instant startup, no external deps.
//! - `postgres` — Embedded PostgreSQL via pg-embed. Downloads PG binary on first
//!   run (~50 MB), cached for future launches.

#[cfg(not(any(feature = "postgres", feature = "sqlite")))]
compile_error!("Enable either the `postgres` or `sqlite` feature for yap-desktop");

use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::sync::oneshot;

use yap_server::{AppState, BufferLayer, LogBuffer, build_router};

#[cfg(feature = "postgres")]
use pg_embed::pg_enums::PgAuthMethod;
#[cfg(feature = "postgres")]
use pg_embed::pg_fetch::{PG_V15, PgFetchSettings};
#[cfg(feature = "postgres")]
use pg_embed::postgres::{PgEmbed, PgSettings};
#[cfg(feature = "postgres")]
use yap_store_pg::{PgStore, run_migrations};

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
use yap_store_sqlite::{SqliteStore, run_migrations};

// ── Tauri state ────────────────────────────────────────────────────────────

struct ServerPort(u16);

/// Tauri command: returns the port the embedded Axum server is listening on.
///
/// The frontend calls this after detecting it is running inside Tauri, then
/// sets its `BASE_URL` accordingly.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // tauri commands require State<T> by value
fn get_server_port(state: tauri::State<ServerPort>) -> u16 {
    state.0
}

// ── Public entry point ─────────────────────────────────────────────────────

/// Run the desktop application.
///
/// Called from `main.rs`. Blocks until the window is closed.
pub fn run() {
    // Initialise tracing for the desktop process.
    let log_buffer = LogBuffer::new(500);

    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "yap_desktop=debug,yap_server=debug,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .with(BufferLayer::new(log_buffer.clone()))
        .init();

    // Build a multi-threaded Tokio runtime that will own the Axum server and
    // any other async work during the lifetime of the app.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime");

    // Start database + Axum synchronously before Tauri opens its window.
    #[cfg(feature = "postgres")]
    let (pg_handle, axum_port, shutdown_tx) = rt
        .block_on(start_services(log_buffer))
        .expect("Failed to start embedded services");

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let (axum_port, shutdown_tx) = rt
        .block_on(start_services(log_buffer))
        .expect("Failed to start embedded services");

    tracing::info!("Services ready — Axum on port {axum_port}");

    // Launch the Tauri application. This blocks until the last window closes.
    tauri::Builder::default()
        .manage(ServerPort(axum_port))
        .invoke_handler(tauri::generate_handler![get_server_port])
        .run(tauri::generate_context!())
        .expect("Error while running Tauri application");

    // ── Teardown ────────────────────────────────────────────────────────────

    // Signal Axum to stop accepting new connections.
    let _ = shutdown_tx.send(());

    // Give the server a moment to drain in-flight requests.
    rt.block_on(async {
        tokio::time::sleep(Duration::from_millis(250)).await;
    });

    // Dropping PgEmbed stops the embedded Postgres process.
    #[cfg(feature = "postgres")]
    drop(pg_handle);

    // Now it's safe to tear down the runtime.
    rt.shutdown_timeout(Duration::from_secs(2));
}

// ── SQLite service startup ──────────────────────────────────────────────────

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn start_services(log_buffer: Arc<LogBuffer>) -> anyhow::Result<(u16, oneshot::Sender<()>)> {
    let data_dir = data_dir()?;

    let db_path = data_dir.join("yap.db");
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());

    tracing::info!("Opening SQLite database at {}", db_path.display());
    let store = SqliteStore::connect(&db_url)
        .await
        .map_err(|e| anyhow::anyhow!("DB connect: {e}"))?;

    run_migrations(store.pool())
        .await
        .map_err(|e| anyhow::anyhow!("Migrations: {e}"))?;

    seed_and_serve(Arc::new(store), log_buffer).await
}

// ── PostgreSQL (pg-embed) service startup ───────────────────────────────────

#[cfg(feature = "postgres")]
async fn start_services(
    log_buffer: Arc<LogBuffer>,
) -> anyhow::Result<(PgEmbed, u16, oneshot::Sender<()>)> {
    let data_dir = data_dir()?;

    // pg-embed doesn't support port 0; find a free port first.
    let pg_port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        listener.local_addr()?.port()
    };

    // Use a subdirectory so pg-embed's PGDATA doesn't collide with yap.db.
    let pg_data_dir = data_dir.join("pgdata");
    std::fs::create_dir_all(&pg_data_dir)?;

    let pg_settings = PgSettings {
        database_dir: pg_data_dir,
        port: pg_port,
        user: "yap".to_string(),
        password: "yap".to_string(),
        auth_method: PgAuthMethod::Plain,
        persistent: true,
        timeout: Some(Duration::from_secs(30)),
        migration_dir: None,
    };

    let fetch_settings = PgFetchSettings {
        version: PG_V15,
        ..Default::default()
    };

    tracing::info!("Starting embedded Postgres on port {pg_port}…");
    let mut pg = PgEmbed::new(pg_settings, fetch_settings).await?;
    pg.setup().await?;
    pg.start_db().await?;

    if !pg.database_exists("yap").await? {
        pg.create_database("yap").await?;
    }

    let db_uri = pg.full_db_uri("yap");
    tracing::info!("Postgres ready, connecting…");

    let store = PgStore::connect(&db_uri)
        .await
        .map_err(|e| anyhow::anyhow!("DB connect: {e}"))?;

    run_migrations(store.pool())
        .await
        .map_err(|e| anyhow::anyhow!("Migrations: {e}"))?;

    let (axum_port, shutdown_tx) = seed_and_serve(Arc::new(store), log_buffer).await?;
    Ok((pg, axum_port, shutdown_tx))
}

// ── Shared helpers ──────────────────────────────────────────────────────────

/// Platform-aware data directory.
///   Linux:   ~/.local/share/yap-orange
///   macOS:   ~/Library/Application Support/yap-orange
///   Windows: %APPDATA%\yap-orange
fn data_dir() -> anyhow::Result<std::path::PathBuf> {
    let dir = dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine platform data directory"))?
        .join("yap-orange");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Ensure meta-schema and settings, then launch Axum on an ephemeral port.
async fn seed_and_serve(
    store: Arc<dyn yap_core::Store>,
    log_buffer: Arc<LogBuffer>,
) -> anyhow::Result<(u16, oneshot::Sender<()>)> {
    let seed_trees = yap_core::seed::default_seed_trees();
    if let Err(e) = yap_core::bootstrap::bootstrap(&*store, &seed_trees).await {
        tracing::warn!("Bootstrap failed: {e}");
    }

    let state = AppState {
        db: store,
        log_buffer,
    };
    let router = build_router(state);

    // Port 0 → OS assigns a free ephemeral port; we read it back immediately.
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let axum_port = listener.local_addr()?.port();

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
                tracing::info!("Axum graceful shutdown signal received");
            })
            .await
            .unwrap_or_else(|e| tracing::error!("Axum server error: {e}"));
    });

    Ok((axum_port, shutdown_tx))
}
