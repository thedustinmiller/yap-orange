//! yap-server — standalone HTTP API server for yap-orange.

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::signal;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use yap_core::export::ExportTree;
use yap_core::file_store::FsFileStore;
use yap_core::seed::{default_seed_trees, parse_seed_json};
use yap_server::{AppState, BufferLayer, LogBuffer, build_router};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let log_buffer = LogBuffer::new(500);

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "yap_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .with(BufferLayer::new(log_buffer.clone()))
        .init();

    let db: Arc<dyn yap_core::Store> = create_store().await?;

    let seed_trees = load_seed_trees();
    if let Err(e) = yap_core::bootstrap::bootstrap(&*db, &seed_trees).await {
        tracing::warn!("Bootstrap failed: {}", e);
    }

    // File store: use FILES_DIR env var or default to ./files/
    let files_dir = std::env::var("FILES_DIR")
        .unwrap_or_else(|_| "files".to_string());
    let files = Arc::new(
        FsFileStore::new(std::path::PathBuf::from(files_dir))
            .expect("Failed to create file store"),
    );

    let state = AppState {
        db,
        log_buffer,
        files,
    };
    let app = build_router(state);

    let host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port: u16 = std::env::var("SERVER_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;

    tracing::info!("Server listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Server shutdown complete");
    Ok(())
}

/// Create the store backend based on compile-time feature flags.
#[cfg(feature = "postgres")]
async fn create_store() -> anyhow::Result<Arc<dyn yap_core::Store>> {
    use yap_store_pg::{PgStore, run_migrations};

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://yap:yap@localhost:5432/yap".to_string());

    tracing::info!("Connecting to PostgreSQL...");
    let db = PgStore::connect(&database_url)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    run_migrations(db.pool())
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    tracing::info!("PostgreSQL connected");

    Ok(Arc::new(db))
}

/// Create the store backend based on compile-time feature flags.
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn create_store() -> anyhow::Result<Arc<dyn yap_core::Store>> {
    use yap_store_sqlite::{SqliteStore, run_migrations};

    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:yap.db?mode=rwc".to_string());

    tracing::info!("Connecting to SQLite...");
    let db = SqliteStore::connect(&database_url)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    run_migrations(db.pool())
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    tracing::info!("SQLite connected");

    Ok(Arc::new(db))
}

#[cfg(not(any(feature = "postgres", feature = "sqlite")))]
async fn create_store() -> anyhow::Result<Arc<dyn yap_core::Store>> {
    anyhow::bail!("No database backend enabled. Enable the 'postgres' or 'sqlite' feature.")
}

/// Load seed trees based on the `YAP_SEED_FILE` environment variable.
///
/// - Not set: use the built-in tutorial (compiled into the binary)
/// - File path: load and parse that JSON file
/// - `"none"` or empty: no seed data (production mode)
fn load_seed_trees() -> Vec<ExportTree> {
    match std::env::var("YAP_SEED_FILE") {
        Ok(val) if val == "none" || val.is_empty() => {
            tracing::info!("Seed data disabled (YAP_SEED_FILE={val:?})");
            vec![]
        }
        Ok(path) => {
            tracing::info!("Loading seed data from {path}");
            match std::fs::read_to_string(&path) {
                Ok(json) => match parse_seed_json(&json) {
                    Ok(trees) => trees,
                    Err(e) => {
                        tracing::warn!("Failed to parse seed file {path}: {e}");
                        vec![]
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to read seed file {path}: {e}");
                    vec![]
                }
            }
        }
        Err(_) => default_seed_trees(),
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C, shutting down...");
        }
        _ = terminate => {
            tracing::info!("Received SIGTERM, shutting down...");
        }
    }
}
