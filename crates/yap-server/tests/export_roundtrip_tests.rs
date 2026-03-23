//! Export/Import round-trip integration tests.
//!
//! Each test creates blocks, exports them, imports the export as a copy,
//! re-exports the imported copy, and compares the two exports.
//! Differences are reported via `eprintln!` but do NOT cause test failures.
//!
//! Run with: cargo test -p yap-server --test export_roundtrip_tests -- --nocapture

use std::sync::Arc;

use axum::Router;
use http::StatusCode;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

use yap_core::Store;
use yap_server::{AppState, LogBuffer, build_router};
use yap_store_sqlite::{SqliteStore, run_migrations};

// =============================================================================
// Test Helpers
// =============================================================================

async fn test_app() -> Router {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("connect to in-memory SQLite");

    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&pool)
        .await
        .expect("enable foreign keys");

    run_migrations(&pool).await.expect("run migrations");

    let db: Arc<dyn Store> = Arc::new(SqliteStore::new(pool));
    let log_buffer = LogBuffer::new(100);

    let dir = tempfile::tempdir().expect("create temp dir");
    let files: Arc<dyn yap_core::file_store::FileStore> = Arc::new(
        yap_core::file_store::FsFileStore::new(dir.path().join("files"))
            .expect("create file store"),
    );

    let state = AppState {
        db,
        log_buffer,
        files,
    };
    build_router(state)
}

/// Send a request and return (status, parsed JSON body).
async fn json_request(
    app: &Router,
    method: http::Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = http::Request::builder().method(method).uri(uri);

    let request = if let Some(body) = body {
        builder = builder.header("content-type", "application/json");
        builder
            .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    } else {
        builder.body(axum::body::Body::empty()).unwrap()
    };

    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();

    let json = if body_bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body_bytes).unwrap_or(Value::Null)
    };

    (status, json)
}

/// Helper: create a block via the API and return (block_id, lineage_id).
async fn create_test_block(
    app: &Router,
    namespace: &str,
    name: &str,
    content: &str,
    content_type: &str,
) -> (Uuid, Uuid) {
    let (status, body) = json_request(
        app,
        http::Method::POST,
        "/api/blocks",
        Some(json!({
            "namespace": namespace,
            "name": name,
            "content": content,
            "content_type": content_type,
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "create block failed: {:?}",
        body
    );
    let block_id: Uuid = body["block_id"].as_str().unwrap().parse().unwrap();
    let lineage_id: Uuid = body["lineage_id"].as_str().unwrap().parse().unwrap();
    (block_id, lineage_id)
}

/// Helper: create a block with custom properties.
async fn create_test_block_with_props(
    app: &Router,
    namespace: &str,
    name: &str,
    content: &str,
    content_type: &str,
    properties: Value,
) -> (Uuid, Uuid) {
    let (status, body) = json_request(
        app,
        http::Method::POST,
        "/api/blocks",
        Some(json!({
            "namespace": namespace,
            "name": name,
            "content": content,
            "content_type": content_type,
            "properties": properties,
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "create block with props failed: {:?}",
        body
    );
    let block_id: Uuid = body["block_id"].as_str().unwrap().parse().unwrap();
    let lineage_id: Uuid = body["lineage_id"].as_str().unwrap().parse().unwrap();
    (block_id, lineage_id)
}

/// Compare two exported trees, returning a list of differences.
/// Ignores: exported_at, source_namespace, positions, UUIDs, hashes.
fn compare_trees(original: &Value, reimported: &Value, label: &str) -> Vec<String> {
    let mut diffs = Vec::new();

    let orig_nodes = original["nodes"].as_array();
    let reimp_nodes = reimported["nodes"].as_array();

    match (orig_nodes, reimp_nodes) {
        (Some(orig), Some(reimp)) => {
            if orig.len() != reimp.len() {
                diffs.push(format!(
                    "[{}] node count: {} vs {}",
                    label,
                    orig.len(),
                    reimp.len()
                ));
            }

            // Sort both by name for stable comparison
            let mut orig_sorted: Vec<&Value> = orig.iter().collect();
            let mut reimp_sorted: Vec<&Value> = reimp.iter().collect();
            orig_sorted.sort_by_key(|n| n["name"].as_str().unwrap_or(""));
            reimp_sorted.sort_by_key(|n| n["name"].as_str().unwrap_or(""));

            for (o, r) in orig_sorted.iter().zip(reimp_sorted.iter()) {
                let name = o["name"].as_str().unwrap_or("?");

                // Compare name
                if o["name"] != r["name"] {
                    diffs.push(format!(
                        "[{}] name mismatch: {:?} vs {:?}",
                        label, o["name"], r["name"]
                    ));
                }
                // Compare content_type
                if o["content_type"] != r["content_type"] {
                    diffs.push(format!(
                        "[{}] {}: content_type {:?} vs {:?}",
                        label, name, o["content_type"], r["content_type"]
                    ));
                }
                // Compare content_template
                if o["content_template"] != r["content_template"] {
                    diffs.push(format!("[{}] {}: content_template differs", label, name));
                }
                // Compare properties (skip underscore keys from both sides)
                let orig_props = filter_underscore_keys(&o["properties"]);
                let reimp_props = filter_underscore_keys(&r["properties"]);
                if orig_props != reimp_props {
                    diffs.push(format!(
                        "[{}] {}: properties {:?} vs {:?}",
                        label, name, orig_props, reimp_props
                    ));
                }
            }
        }
        _ => {
            diffs.push(format!("[{}] missing nodes array", label));
        }
    }

    // Compare edge count
    let orig_edges = original["edges"].as_array().map(|a| a.len()).unwrap_or(0);
    let reimp_edges = reimported["edges"].as_array().map(|a| a.len()).unwrap_or(0);
    if orig_edges != reimp_edges {
        diffs.push(format!(
            "[{}] edge count: {} vs {}",
            label, orig_edges, reimp_edges
        ));
    }

    diffs
}

/// Remove keys starting with `_` from a JSON object value.
fn filter_underscore_keys(value: &Value) -> Value {
    match value.as_object() {
        Some(obj) => {
            let filtered: serde_json::Map<String, Value> = obj
                .iter()
                .filter(|(k, _)| !k.starts_with('_'))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            Value::Object(filtered)
        }
        None => value.clone(),
    }
}

// =============================================================================
// Round-Trip Tests
// =============================================================================

#[tokio::test]
async fn test_roundtrip_basic() {
    let app = test_app().await;
    let label = "roundtrip_basic";

    // 1. Create source blocks
    let (root_id, _) = create_test_block(&app, "", "rt_basic", "Hello world", "content").await;
    create_test_block(&app, "rt_basic", "child_a", "Content A", "content").await;
    create_test_block(&app, "rt_basic", "child_b", "Content B", "content").await;

    // 2. Export
    let (status, original) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{root_id}/export"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "export failed: {:?}", original);

    // 3. Create destination parent
    let (dst_id, _) = create_test_block(&app, "", "rt_basic_dst", "", "content").await;

    // 4. Import as copy
    let (status, import_result) = json_request(
        &app,
        http::Method::POST,
        &format!("/api/blocks/{dst_id}/import?mode=copy"),
        Some(original.clone()),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "import failed: {:?}",
        import_result
    );

    let root_block_id = import_result["root_block_id"].as_str().unwrap();

    // 5. Re-export the imported copy
    let (status, reimported) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{root_block_id}/export"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "re-export failed: {:?}", reimported);

    // 6. Compare
    let diffs = compare_trees(&original, &reimported, label);
    if diffs.is_empty() {
        eprintln!("[OK] {label}: round-trip identical");
    } else {
        for d in &diffs {
            eprintln!("[DIFF] {d}");
        }
    }
}

#[tokio::test]
async fn test_roundtrip_with_links() {
    let app = test_app().await;
    let label = "roundtrip_with_links";

    // 1. Create a separate root that will be the link target
    create_test_block(&app, "", "link_target", "I am the target", "content").await;

    // 2. Create the export root with a child that links to link_target
    let (root_id, _) = create_test_block(&app, "", "rt_links", "", "content").await;
    create_test_block(
        &app,
        "rt_links",
        "link_source",
        "See [[link_target]]",
        "content",
    )
    .await;

    // 3. Export
    let (status, original) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{root_id}/export"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "export failed: {:?}", original);

    // 4. Create destination parent
    let (dst_id, _) = create_test_block(&app, "", "rt_links_dst", "", "content").await;

    // 5. Import as merge so external links resolve
    let (status, import_result) = json_request(
        &app,
        http::Method::POST,
        &format!("/api/blocks/{dst_id}/import?mode=merge"),
        Some(original.clone()),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "import failed: {:?}",
        import_result
    );

    let root_block_id = import_result["root_block_id"].as_str().unwrap();

    // 6. Re-export the imported copy
    let (status, reimported) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{root_block_id}/export"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "re-export failed: {:?}", reimported);

    // 7. Compare
    let diffs = compare_trees(&original, &reimported, label);
    if diffs.is_empty() {
        eprintln!("[OK] {label}: round-trip identical");
    } else {
        for d in &diffs {
            eprintln!("[DIFF] {d}");
        }
    }
}

#[tokio::test]
async fn test_roundtrip_with_edges() {
    let app = test_app().await;
    let label = "roundtrip_with_edges";

    // 1. Create root with 2 children
    let (root_id, _) = create_test_block(&app, "", "rt_edges", "", "content").await;
    let (_, lin_a) =
        create_test_block(&app, "rt_edges", "edge_a", "Edge A content", "content").await;
    let (_, lin_b) =
        create_test_block(&app, "rt_edges", "edge_b", "Edge B content", "content").await;

    // 2. Create a semantic edge between edge_a and edge_b
    let (status, _) = json_request(
        &app,
        http::Method::POST,
        "/api/edges",
        Some(json!({
            "from_lineage_id": lin_a,
            "to_lineage_id": lin_b,
            "edge_type": "related",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "edge creation failed");

    // 3. Export
    let (status, original) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{root_id}/export"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "export failed: {:?}", original);

    // 4. Create destination parent
    let (dst_id, _) = create_test_block(&app, "", "rt_edges_dst", "", "content").await;

    // 5. Import as copy
    let (status, import_result) = json_request(
        &app,
        http::Method::POST,
        &format!("/api/blocks/{dst_id}/import?mode=copy"),
        Some(original.clone()),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "import failed: {:?}",
        import_result
    );

    let root_block_id = import_result["root_block_id"].as_str().unwrap();

    // 6. Re-export the imported copy
    let (status, reimported) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{root_block_id}/export"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "re-export failed: {:?}", reimported);

    // 7. Compare
    let diffs = compare_trees(&original, &reimported, label);
    if diffs.is_empty() {
        eprintln!("[OK] {label}: round-trip identical");
    } else {
        for d in &diffs {
            eprintln!("[DIFF] {d}");
        }
    }
}

#[tokio::test]
async fn test_roundtrip_with_properties() {
    let app = test_app().await;
    let label = "roundtrip_with_properties";

    // 1. Create root
    let (root_id, _) = create_test_block(&app, "", "rt_props", "", "content").await;

    // 2. Create child with both public and underscore-prefixed properties
    create_test_block_with_props(
        &app,
        "rt_props",
        "prop_child",
        "Has properties",
        "content",
        json!({"priority": "high", "_internal": "hidden"}),
    )
    .await;

    // 3. Export — _internal should be stripped
    let (status, original) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{root_id}/export"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "export failed: {:?}", original);

    // Verify _internal is not present in the export
    let nodes = original["nodes"].as_array().unwrap();
    for node in nodes {
        if node["name"].as_str() == Some("prop_child") {
            let props = &node["properties"];
            if props.get("_internal").is_some() {
                eprintln!("[DIFF] [{label}] _internal key should be stripped from export");
            } else {
                eprintln!("[OK] {label}: _internal key correctly stripped from export");
            }
        }
    }

    // 4. Create destination parent
    let (dst_id, _) = create_test_block(&app, "", "rt_props_dst", "", "content").await;

    // 5. Import as copy
    let (status, import_result) = json_request(
        &app,
        http::Method::POST,
        &format!("/api/blocks/{dst_id}/import?mode=copy"),
        Some(original.clone()),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "import failed: {:?}",
        import_result
    );

    let root_block_id = import_result["root_block_id"].as_str().unwrap();

    // 6. Re-export the imported copy
    let (status, reimported) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{root_block_id}/export"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "re-export failed: {:?}", reimported);

    // 7. Compare
    let diffs = compare_trees(&original, &reimported, label);
    if diffs.is_empty() {
        eprintln!("[OK] {label}: round-trip identical");
    } else {
        for d in &diffs {
            eprintln!("[DIFF] {d}");
        }
    }
}

#[tokio::test]
async fn test_roundtrip_with_type_embeds() {
    let app = test_app().await;
    let label = "roundtrip_with_type_embeds";

    // 1. Create a schema block at types::todo
    create_test_block_with_props(
        &app,
        "types",
        "todo",
        "",
        "schema",
        json!({
            "fields": [
                {"name": "status", "type": "string"},
                {"name": "assignee", "type": "string"}
            ]
        }),
    )
    .await;

    // 2. Create the export root
    let (root_id, _) = create_test_block(&app, "", "rt_types", "", "content").await;

    // 3. Create a typed child with content_type "todo" and properties
    create_test_block_with_props(
        &app,
        "rt_types",
        "my_task",
        "A task to do",
        "todo",
        json!({"status": "open"}),
    )
    .await;

    // 4. Export
    let (status, original) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{root_id}/export"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "export failed: {:?}", original);

    // 5. Create destination parent
    let (dst_id, _) = create_test_block(&app, "", "rt_types_dst", "", "content").await;

    // 6. Import as copy
    let (status, import_result) = json_request(
        &app,
        http::Method::POST,
        &format!("/api/blocks/{dst_id}/import?mode=copy"),
        Some(original.clone()),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "import failed: {:?}",
        import_result
    );

    let root_block_id = import_result["root_block_id"].as_str().unwrap();

    // 7. Re-export the imported copy
    let (status, reimported) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{root_block_id}/export"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "re-export failed: {:?}", reimported);

    // 8. Compare
    let diffs = compare_trees(&original, &reimported, label);
    if diffs.is_empty() {
        eprintln!("[OK] {label}: round-trip identical");
    } else {
        for d in &diffs {
            eprintln!("[DIFF] {d}");
        }
    }
}
