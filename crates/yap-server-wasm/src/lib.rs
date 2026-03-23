//! WASM entry point for yap-orange.
//!
//! Compiles to a `cdylib` targeting `wasm32-unknown-unknown`. Exports two
//! functions for the Service Worker:
//!
//! - `init()` — install OPFS VFS, open SQLite, run migrations, build router
//! - `handle_request(method, url, body)` — route a fetch through Axum
//!
//! The Axum router is stored in a `thread_local` `OnceCell` after init.

use std::cell::OnceCell;
use std::sync::Arc;

use axum::body::Body;
use http::Request;
use http_body_util::BodyExt;
use tower_service::Service;
use wasm_bindgen::prelude::*;

use yap_core::Store as _;
use yap_server::log_buffer::LogBuffer;
use yap_server::{AppState, build_router};
use yap_store_wasm::db::WasmDb;
use yap_store_wasm::{WasmFileStore, WasmSqliteStore};

// thread_local is fine — WASM is single-threaded.
thread_local! {
    static ROUTER: OnceCell<axum::Router> = const { OnceCell::new() };
    static STORE: OnceCell<Arc<WasmSqliteStore>> = const { OnceCell::new() };
}

/// Initialize the WASM backend: install OPFS VFS, open SQLite, run migrations,
/// bootstrap meta-schema + settings, and build the Axum router.
#[wasm_bindgen]
pub async fn init() -> std::result::Result<(), JsValue> {
    // 1. Install OPFS VFS for persistent browser storage
    use sqlite_wasm_vfs::sahpool::{install as install_opfs_sahpool, OpfsSAHPoolCfg};
    install_opfs_sahpool::<sqlite_wasm_rs::WasmOsCallback>(
        &OpfsSAHPoolCfg::default(),
        true, // set as default VFS
    )
    .await
    .map_err(|e| JsValue::from_str(&format!("OPFS VFS install failed: {:?}", e)))?;

    // 2. Open SQLite database (persisted via OPFS)
    let db = WasmDb::open("yap-orange.db")
        .map_err(|e| JsValue::from_str(&format!("DB open failed: {}", e)))?;

    // 3. Enable foreign keys (WAL not needed for single-connection OPFS)
    db.exec("PRAGMA foreign_keys=ON;")
        .map_err(|e| JsValue::from_str(&format!("PRAGMA failed: {}", e)))?;

    // 4. Run migrations
    let store = Arc::new(WasmSqliteStore::new(db));
    store
        .run_migrations()
        .map_err(|e| JsValue::from_str(&format!("Migrations failed: {}", e)))?;

    // 5. Bootstrap meta-schema + settings + seed data (tutorial on first run)
    let seed_trees = yap_core::seed::default_seed_trees();
    yap_core::bootstrap::bootstrap(store.as_ref(), &seed_trees)
        .await
        .map_err(|e| JsValue::from_str(&format!("Bootstrap failed: {}", e)))?;

    // 6. Open a second connection for the file store (same OPFS database)
    let file_db = WasmDb::open("yap-orange.db")
        .map_err(|e| JsValue::from_str(&format!("File DB open failed: {}", e)))?;
    file_db.exec("PRAGMA foreign_keys=ON;")
        .map_err(|e| JsValue::from_str(&format!("File DB PRAGMA failed: {}", e)))?;
    let files: Arc<dyn yap_core::file_store::FileStore> = Arc::new(WasmFileStore::new(file_db));

    // 7. Build router and store references
    let log_buffer = LogBuffer::new(500);
    let state = AppState {
        db: store.clone() as Arc<dyn yap_core::Store>,
        log_buffer,
        files,
    };
    let router = build_router(state);

    STORE.with(|cell| {
        let _ = cell.set(store);
    });
    ROUTER.with(|cell| {
        let _ = cell.set(router);
    });

    Ok(())
}

/// Handle a single HTTP request by routing it through the Axum router.
///
/// Called from the Service Worker's `fetch` event handler.
/// Returns a JSON string with `{ status, headers, body }` that the SW
/// converts into a real `Response` object.
#[wasm_bindgen]
pub async fn handle_request(
    method: &str,
    url: &str,
    body: &str,
) -> std::result::Result<String, JsValue> {
    // 1. Clone the router out of thread_local (Router is Clone + cheap)
    let mut service = ROUTER.with(|cell| {
        cell.get()
            .cloned()
            .ok_or_else(|| JsValue::from_str("Router not initialized — call init() first"))
    })?;

    // 2. Build the http::Request
    let http_method = method
        .parse::<http::Method>()
        .map_err(|e| JsValue::from_str(&format!("Invalid method: {}", e)))?;

    let mut builder = Request::builder().method(http_method).uri(url);
    if !body.is_empty() {
        builder = builder.header("content-type", "application/json");
    }

    let request = builder
        .body(Body::from(body.to_string()))
        .map_err(|e| JsValue::from_str(&format!("Failed to build request: {}", e)))?;

    // 3. Route through Axum
    let response = service
        .call(request)
        .await
        .map_err(|e| JsValue::from_str(&format!("Router error: {}", e)))?;

    // 4. Extract response parts
    let (parts, resp_body) = response.into_parts();
    let body_bytes = resp_body
        .collect()
        .await
        .map_err(|e| JsValue::from_str(&format!("Body read error: {}", e)))?
        .to_bytes();
    let body_str = String::from_utf8_lossy(&body_bytes).into_owned();

    // 5. Serialize headers
    let mut headers = serde_json::Map::new();
    for (key, value) in parts.headers.iter() {
        if let Ok(v) = value.to_str() {
            headers.insert(key.to_string(), serde_json::Value::String(v.to_string()));
        }
    }

    // 6. Return JSON for the SW to construct a Response
    let result = serde_json::json!({
        "status": parts.status.as_u16(),
        "headers": headers,
        "body": body_str,
    });

    Ok(result.to_string())
}

/// Factory reset: clear all data, re-run migrations, and re-bootstrap.
///
/// Called from JS when the user wants to restore the database to its
/// initial state. The Axum router is unaffected — it holds the same
/// `Arc<dyn Store>`, so requests continue to work after reset.
#[wasm_bindgen]
pub async fn factory_reset() -> std::result::Result<(), JsValue> {
    let store = STORE.with(|cell| {
        cell.get()
            .cloned()
            .ok_or_else(|| JsValue::from_str("Store not initialized — call init() first"))
    })?;

    // 1. Clear all data
    store
        .clear_all_data()
        .await
        .map_err(|e| JsValue::from_str(&format!("clear_all_data failed: {}", e)))?;

    // 2. Re-run migrations (idempotent — all CREATE IF NOT EXISTS)
    store
        .run_migrations()
        .map_err(|e| JsValue::from_str(&format!("Migrations failed: {}", e)))?;

    // 3. Re-bootstrap with seed data (restores tutorial)
    let seed_trees = yap_core::seed::default_seed_trees();
    yap_core::bootstrap::bootstrap(store.as_ref(), &seed_trees)
        .await
        .map_err(|e| JsValue::from_str(&format!("Bootstrap failed: {}", e)))?;

    Ok(())
}
