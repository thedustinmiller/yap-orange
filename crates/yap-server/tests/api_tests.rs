//! HTTP API handler tests using tower::ServiceExt::oneshot.
//!
//! Tests the full request/response cycle through the Axum router without a network.
//! Uses in-memory SQLite for fast, isolated tests.
//!
//! Run with: cargo test -p yap-server --test api_tests -- --nocapture

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

    let state = AppState { db, log_buffer };
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

// =============================================================================
// Health
// =============================================================================

#[tokio::test]
async fn test_health() {
    let app = test_app().await;
    let (status, body) = json_request(&app, http::Method::GET, "/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["database"], "connected");
}

// =============================================================================
// Block CRUD
// =============================================================================

#[tokio::test]
async fn test_create_block() {
    let app = test_app().await;
    let (block_id, lineage_id) = create_test_block(&app, "", "myblock", "hello", "content").await;
    assert_ne!(block_id, Uuid::nil());
    assert_ne!(lineage_id, Uuid::nil());
}

#[tokio::test]
async fn test_create_block_with_namespace() {
    let app = test_app().await;
    let (_, body) = json_request(
        &app,
        http::Method::POST,
        "/api/blocks",
        Some(json!({
            "namespace": "research::ml",
            "name": "attention",
            "content": "Attention mechanism",
            "content_type": "content",
        })),
    )
    .await;
    assert_eq!(body["namespace"], "research::ml::attention");
    assert_eq!(body["name"], "attention");
}

#[tokio::test]
async fn test_get_block() {
    let app = test_app().await;
    let (block_id, _) = create_test_block(&app, "", "getme", "content here", "content").await;

    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{block_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "getme");
    assert_eq!(body["content"], "content here");
    assert_eq!(body["content_type"], "content");
}

#[tokio::test]
async fn test_get_block_not_found() {
    let app = test_app().await;
    let fake_id = Uuid::now_v7();
    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{fake_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn test_update_block() {
    let app = test_app().await;
    let (block_id, _) = create_test_block(&app, "", "oldname", "", "content").await;

    let (status, body) = json_request(
        &app,
        http::Method::PUT,
        &format!("/api/blocks/{block_id}"),
        Some(json!({ "name": "newname" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "newname");
}

#[tokio::test]
async fn test_delete_block() {
    let app = test_app().await;
    let (block_id, _) = create_test_block(&app, "", "deleteme", "", "content").await;

    let (status, _) = json_request(
        &app,
        http::Method::DELETE,
        &format!("/api/blocks/{block_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Verify it's gone
    let (status, _) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{block_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_block_with_children_rejected() {
    let app = test_app().await;
    let (parent_id, _) = create_test_block(&app, "", "parent_del", "", "content").await;

    // Create a child via the store
    let (_child_id, _) = json_request(
        &app,
        http::Method::POST,
        "/api/blocks",
        Some(json!({
            "namespace": "parent_del",
            "name": "child",
            "content": "",
            "content_type": "content",
        })),
    )
    .await;

    // Trying to delete parent should fail (has children)
    let (status, body) = json_request(
        &app,
        http::Method::DELETE,
        &format!("/api/blocks/{parent_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("children"));
}

#[tokio::test]
async fn test_delete_block_recursive() {
    let app = test_app().await;
    let (parent_id, _) = create_test_block(&app, "", "rec_parent", "", "content").await;
    create_test_block(&app, "rec_parent", "rec_child", "", "content").await;

    let (status, _) = json_request(
        &app,
        http::Method::DELETE,
        &format!("/api/blocks/{parent_id}/recursive"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Verify parent is gone
    let (status, _) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{parent_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_list_roots() {
    let app = test_app().await;
    create_test_block(&app, "", "root1", "", "content").await;
    create_test_block(&app, "", "root2", "", "content").await;

    let (status, body) = json_request(&app, http::Method::GET, "/api/blocks/roots", None).await;
    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().unwrap();
    assert!(arr.len() >= 2);
    let names: Vec<&str> = arr.iter().filter_map(|b| b["name"].as_str()).collect();
    assert!(names.contains(&"root1"));
    assert!(names.contains(&"root2"));
}

#[tokio::test]
async fn test_get_block_children() {
    let app = test_app().await;
    let (parent_id, _) = create_test_block(&app, "", "parent_ch", "", "content").await;
    create_test_block(&app, "parent_ch", "ch1", "", "content").await;
    create_test_block(&app, "parent_ch", "ch2", "", "content").await;

    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{parent_id}/children"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 2);
}

// =============================================================================
// Block Operations
// =============================================================================

#[tokio::test]
async fn test_move_block() {
    let app = test_app().await;
    let (parent1_id, _) = create_test_block(&app, "", "mv_p1", "", "content").await;
    let (parent2_id, _) = create_test_block(&app, "", "mv_p2", "", "content").await;
    let (child_id, _) = create_test_block(&app, "mv_p1", "mv_child", "", "content").await;

    let (status, body) = json_request(
        &app,
        http::Method::POST,
        &format!("/api/blocks/{child_id}/move"),
        Some(json!({ "parent_id": parent2_id })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["parent_id"], parent2_id.to_string());

    // Verify parent1 has no children
    let (_, p1_children) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{parent1_id}/children"),
        None,
    )
    .await;
    assert_eq!(p1_children.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_restore_block() {
    let app = test_app().await;
    let (block_id, _) = create_test_block(&app, "", "restore_me", "content", "content").await;

    // Delete it
    json_request(
        &app,
        http::Method::DELETE,
        &format!("/api/blocks/{block_id}"),
        None,
    )
    .await;

    // Restore it
    let (status, body) = json_request(
        &app,
        http::Method::POST,
        &format!("/api/blocks/{block_id}/restore"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "restore_me");

    // Verify it's accessible again
    let (status, _) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{block_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_restore_block_recursive() {
    let app = test_app().await;
    let (parent_id, _) = create_test_block(&app, "", "rest_rec_p", "", "content").await;
    create_test_block(&app, "rest_rec_p", "rest_rec_c", "", "content").await;

    // Delete recursively
    json_request(
        &app,
        http::Method::DELETE,
        &format!("/api/blocks/{parent_id}/recursive"),
        None,
    )
    .await;

    // Restore recursively
    let (status, body) = json_request(
        &app,
        http::Method::POST,
        &format!("/api/blocks/{parent_id}/restore-recursive"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["restored"], 2);
}

#[tokio::test]
async fn test_list_blocks_search() {
    let app = test_app().await;
    create_test_block(&app, "", "unique_search_target", "", "content").await;

    let (status, body) = json_request(
        &app,
        http::Method::GET,
        "/api/blocks?search=unique_search_target",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().unwrap();
    assert!(arr.iter().any(|b| b["name"] == "unique_search_target"));
}

#[tokio::test]
async fn test_list_orphans() {
    let app = test_app().await;
    let (parent_id, _) = create_test_block(&app, "", "orph_parent", "", "content").await;
    create_test_block(&app, "orph_parent", "orph_child", "", "content").await;

    // Delete only the parent (non-recursive) — child becomes orphan
    // First need to use recursive delete to remove parent only... actually
    // the API rejects non-recursive delete when children exist.
    // So we use the store directly — but since we only have the Router,
    // let's test orphan listing with a different approach.
    // We'll delete the parent recursively and then restore just the child.

    // Delete parent recursively
    json_request(
        &app,
        http::Method::DELETE,
        &format!("/api/blocks/{parent_id}/recursive"),
        None,
    )
    .await;

    // Restore just the child (but not the parent)
    // We don't know the child's ID from the API... let's create a scenario differently.
    // Instead, create the orphan scenario differently:
    // Create parent and child, then move child to root, delete parent, move child back
    // Actually, the simplest is to just verify the endpoint returns 200 with an array.

    let (status, body) = json_request(&app, http::Method::GET, "/api/blocks/orphans", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());
}

// =============================================================================
// Atom Endpoints
// =============================================================================

#[tokio::test]
async fn test_get_atom() {
    let app = test_app().await;
    let (_, lineage_id) = create_test_block(&app, "", "atom_test", "atom content", "content").await;

    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/atoms/{lineage_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["content_template"], "atom content");
    assert_eq!(body["content_type"], "content");
    assert!(body["content_hash"].is_string());
}

#[tokio::test]
async fn test_get_atom_rendered() {
    let app = test_app().await;
    let (_, lineage_id) =
        create_test_block(&app, "", "rendered_test", "plain text", "content").await;

    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/atoms/{lineage_id}/rendered"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["content"], "plain text");
    assert_eq!(body["content_type"], "content");
}

#[tokio::test]
async fn test_update_atom() {
    let app = test_app().await;
    let (_, lineage_id) = create_test_block(&app, "", "edit_atom", "original", "content").await;

    let (status, body) = json_request(
        &app,
        http::Method::PUT,
        &format!("/api/atoms/{lineage_id}"),
        Some(json!({ "content": "updated content" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["content_template"], "updated content");
    assert!(
        body["predecessor_id"].is_string(),
        "should have predecessor"
    );
}

#[tokio::test]
async fn test_get_atom_not_found() {
    let app = test_app().await;
    let fake_id = Uuid::now_v7();
    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/atoms/{fake_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

// =============================================================================
// Edge Endpoints
// =============================================================================

#[tokio::test]
async fn test_create_edge() {
    let app = test_app().await;
    let (_, lin1) = create_test_block(&app, "", "edge_from", "", "content").await;
    let (_, lin2) = create_test_block(&app, "", "edge_to", "", "content").await;

    let (status, body) = json_request(
        &app,
        http::Method::POST,
        "/api/edges",
        Some(json!({
            "from_lineage_id": lin1,
            "to_lineage_id": lin2,
            "edge_type": "related",
            "properties": {"weight": 1}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["edge_type"], "related");
    assert_eq!(body["from_lineage_id"], lin1.to_string());
    assert_eq!(body["to_lineage_id"], lin2.to_string());
}

#[tokio::test]
async fn test_create_edge_duplicate_409() {
    let app = test_app().await;
    let (_, lin1) = create_test_block(&app, "", "dup_edge_from", "", "content").await;
    let (_, lin2) = create_test_block(&app, "", "dup_edge_to", "", "content").await;

    let edge_body = json!({
        "from_lineage_id": lin1,
        "to_lineage_id": lin2,
        "edge_type": "dup_test",
    });

    let (status, _) = json_request(
        &app,
        http::Method::POST,
        "/api/edges",
        Some(edge_body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = json_request(&app, http::Method::POST, "/api/edges", Some(edge_body)).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_delete_edge() {
    let app = test_app().await;
    let (_, lin1) = create_test_block(&app, "", "del_edge_from", "", "content").await;
    let (_, lin2) = create_test_block(&app, "", "del_edge_to", "", "content").await;

    let (_, body) = json_request(
        &app,
        http::Method::POST,
        "/api/edges",
        Some(json!({
            "from_lineage_id": lin1,
            "to_lineage_id": lin2,
            "edge_type": "deletable",
        })),
    )
    .await;
    let edge_id = body["id"].as_str().unwrap();

    let (status, _) = json_request(
        &app,
        http::Method::DELETE,
        &format!("/api/edges/{edge_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_delete_edge_not_found() {
    let app = test_app().await;
    let fake_id = Uuid::now_v7();
    let (status, _) = json_request(
        &app,
        http::Method::DELETE,
        &format!("/api/edges/{fake_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// =============================================================================
// Graph Endpoints
// =============================================================================

#[tokio::test]
async fn test_get_atom_backlinks() {
    let app = test_app().await;
    let (_, target_lineage) = create_test_block(&app, "", "bl_target", "target", "content").await;

    // Create a linker that references the target
    // We need to use the store's create_block_with_content for template links,
    // but since we're testing via HTTP, we'll just verify the endpoint works
    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/atoms/{target_lineage}/backlinks"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());
}

#[tokio::test]
async fn test_get_atom_graph() {
    let app = test_app().await;
    let (_, lineage_id) = create_test_block(&app, "", "graph_test", "content", "content").await;

    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/atoms/{lineage_id}/graph"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["atom"].is_object());
    assert!(body["backlinks"].is_array());
    assert!(body["outlinks"].is_array());
    assert!(body["edges"].is_object());
    assert!(body["edges"]["outgoing"].is_array());
    assert!(body["edges"]["incoming"].is_array());
}

// =============================================================================
// Links / Schemas
// =============================================================================

#[tokio::test]
async fn test_resolve_link() {
    let app = test_app().await;
    create_test_block(&app, "", "resolve_target", "", "content").await;

    let (status, body) = json_request(
        &app,
        http::Method::POST,
        "/api/resolve",
        Some(json!({ "path": "resolve_target" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["lineage_id"].is_string());
    assert!(body["block_id"].is_string());
    assert_eq!(body["namespace"], "resolve_target");
}

#[tokio::test]
async fn test_list_schemas() {
    let app = test_app().await;

    // Create a schema block
    create_test_block(&app, "types", "task", "", "schema").await;

    let (status, body) = json_request(&app, http::Method::GET, "/api/schemas", None).await;
    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().unwrap();
    assert!(arr.iter().any(|s| s["name"] == "task"));
}

#[tokio::test]
async fn test_resolve_schema() {
    let app = test_app().await;

    // Create a schema
    create_test_block(&app, "types", "project", "", "schema").await;

    let (status, body) = json_request(
        &app,
        http::Method::POST,
        "/api/schemas/resolve",
        Some(json!({ "type_name": "project" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "project");
    assert!(body["block_id"].is_string());
}

// =============================================================================
// Export / Import
// =============================================================================

#[tokio::test]
async fn test_export_block_tree() {
    let app = test_app().await;
    let (parent_id, _) = create_test_block(&app, "", "export_root", "", "content").await;
    create_test_block(
        &app,
        "export_root",
        "export_child",
        "child content",
        "content",
    )
    .await;

    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{parent_id}/export"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["format"], "yap-tree-v2");
    let nodes = body["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 2);
}

#[tokio::test]
async fn test_import_block_tree() {
    let app = test_app().await;
    let (src_id, _) = create_test_block(&app, "", "imp_src", "", "content").await;
    create_test_block(&app, "imp_src", "imp_child", "data", "content").await;

    // Export
    let (_, tree) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{src_id}/export"),
        None,
    )
    .await;

    // Create destination
    let (dst_id, _) = create_test_block(&app, "", "imp_dst", "", "content").await;

    // Import
    let (status, body) = json_request(
        &app,
        http::Method::POST,
        &format!("/api/blocks/{dst_id}/import?mode=copy"),
        Some(tree),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(body["created"].as_u64().unwrap() >= 2);
}

// =============================================================================
// Debug
// =============================================================================

#[tokio::test]
async fn test_debug_logs() {
    let app = test_app().await;

    let (status, body) =
        json_request(&app, http::Method::GET, "/api/debug/logs?since=0", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());
}

// =============================================================================
// List Queries
// =============================================================================

#[tokio::test]
async fn test_list_blocks_by_content_type() {
    let app = test_app().await;
    create_test_block(&app, "", "ct_block", "", "custom_ct").await;

    let (status, body) = json_request(
        &app,
        http::Method::GET,
        "/api/blocks?content_type=custom_ct",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().unwrap();
    assert!(arr.iter().any(|b| b["name"] == "ct_block"));
}

#[tokio::test]
async fn test_list_blocks_by_namespace() {
    let app = test_app().await;
    create_test_block(&app, "nsq", "deep_block", "", "content").await;

    let (status, body) =
        json_request(&app, http::Method::GET, "/api/blocks?namespace=nsq", None).await;
    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().unwrap();
    assert!(!arr.is_empty());
}

// =============================================================================
// Error Format
// =============================================================================

#[tokio::test]
async fn test_error_response_format() {
    let app = test_app().await;
    let fake_id = Uuid::now_v7();
    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{fake_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // Verify the standard error shape
    assert!(body.is_object());
    assert!(
        body.get("error").is_some(),
        "error response should have 'error' field"
    );
    assert!(body["error"].is_string(), "error field should be a string");
}

// =============================================================================
// 404 Paths — untested endpoints hitting get_block/get_atom guards
// =============================================================================

#[tokio::test]
async fn test_update_block_not_found() {
    let app = test_app().await;
    let fake_id = Uuid::now_v7();
    let (status, body) = json_request(
        &app,
        http::Method::PUT,
        &format!("/api/blocks/{fake_id}"),
        Some(json!({ "name": "nope" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn test_update_atom_not_found() {
    let app = test_app().await;
    let fake_id = Uuid::now_v7();
    let (status, body) = json_request(
        &app,
        http::Method::PUT,
        &format!("/api/atoms/{fake_id}"),
        Some(json!({ "content": "nope" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn test_move_block_not_found() {
    let app = test_app().await;
    let fake_id = Uuid::now_v7();
    let (status, body) = json_request(
        &app,
        http::Method::POST,
        &format!("/api/blocks/{fake_id}/move"),
        Some(json!({ "parent_id": null })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn test_get_children_not_found() {
    let app = test_app().await;
    let fake_id = Uuid::now_v7();
    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{fake_id}/children"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn test_get_backlinks_not_found() {
    let app = test_app().await;
    let fake_id = Uuid::now_v7();
    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/atoms/{fake_id}/backlinks"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn test_get_edges_not_found() {
    let app = test_app().await;
    let fake_id = Uuid::now_v7();
    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/atoms/{fake_id}/edges"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn test_get_graph_not_found() {
    let app = test_app().await;
    let fake_id = Uuid::now_v7();
    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/atoms/{fake_id}/graph"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn test_get_references_not_found() {
    let app = test_app().await;
    let fake_id = Uuid::now_v7();
    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/atoms/{fake_id}/references"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn test_restore_block_not_found() {
    let app = test_app().await;
    let fake_id = Uuid::now_v7();
    let (status, body) = json_request(
        &app,
        http::Method::POST,
        &format!("/api/blocks/{fake_id}/restore"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn test_export_block_not_found() {
    let app = test_app().await;
    let fake_id = Uuid::now_v7();
    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{fake_id}/export"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn test_delete_recursive_not_found() {
    let app = test_app().await;
    let fake_id = Uuid::now_v7();
    let (status, body) = json_request(
        &app,
        http::Method::DELETE,
        &format!("/api/blocks/{fake_id}/recursive"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

// =============================================================================
// Validation / Conflict Paths
// =============================================================================

#[tokio::test]
async fn test_resolve_link_not_found() {
    let app = test_app().await;
    let (status, body) = json_request(
        &app,
        http::Method::POST,
        "/api/resolve",
        Some(json!({ "path": "nonexistent::deep::path" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn test_resolve_link_empty_path() {
    let app = test_app().await;
    let (status, body) = json_request(
        &app,
        http::Method::POST,
        "/api/resolve",
        Some(json!({ "path": "" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn test_resolve_schema_not_found() {
    let app = test_app().await;
    let (status, body) = json_request(
        &app,
        http::Method::POST,
        "/api/schemas/resolve",
        Some(json!({ "type_name": "nonexistent_schema_xyz" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn test_create_block_duplicate_name() {
    let app = test_app().await;

    // Create the first block
    create_test_block(&app, "", "dup_name_test", "", "content").await;

    // Attempt to create another root block with the same name
    let (status, body) = json_request(
        &app,
        http::Method::POST,
        "/api/blocks",
        Some(json!({
            "namespace": "",
            "name": "dup_name_test",
            "content": "",
            "content_type": "content",
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "duplicate name should 409: {:?}",
        body
    );
    assert!(body["error"].is_string());
}

// =============================================================================
// API-level soft-delete cascading (P2.9)
// =============================================================================

#[tokio::test]
async fn test_delete_block_edges_survive_api() {
    let app = test_app().await;

    // Create two blocks with an edge between their lineages
    let (block_a_id, lin_a) = create_test_block(&app, "", "edge_surv_a", "", "content").await;
    let (_block_b_id, lin_b) = create_test_block(&app, "", "edge_surv_b", "", "content").await;

    // Create an edge from A to B
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
    assert_eq!(status, StatusCode::CREATED);

    // Delete block A
    let (status, _) = json_request(
        &app,
        http::Method::DELETE,
        &format!("/api/blocks/{block_a_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Query edges on block B — the edge should still be present
    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/atoms/{lin_b}/edges"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let incoming = body["incoming"].as_array().unwrap();
    assert!(
        incoming
            .iter()
            .any(|e| e["from_lineage_id"] == lin_a.to_string()),
        "Edge from deleted block's lineage should still exist on the other side"
    );
}

#[tokio::test]
async fn test_orphan_after_partial_restore() {
    let app = test_app().await;

    // Create parent with a child
    let (parent_id, _) = create_test_block(&app, "", "orph_p_rest", "", "content").await;
    let (child_id, _) = create_test_block(&app, "orph_p_rest", "orph_c_rest", "", "content").await;

    // Recursive delete parent+child
    let (status, _) = json_request(
        &app,
        http::Method::DELETE,
        &format!("/api/blocks/{parent_id}/recursive"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Restore only the child (not the parent)
    let (status, _) = json_request(
        &app,
        http::Method::POST,
        &format!("/api/blocks/{child_id}/restore"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Child should now appear in orphans (parent still deleted)
    let (status, body) = json_request(&app, http::Method::GET, "/api/blocks/orphans", None).await;
    assert_eq!(status, StatusCode::OK);

    let orphans = body.as_array().unwrap();
    assert!(
        orphans.iter().any(|o| o["id"] == child_id.to_string()),
        "Restored child with deleted parent should appear as orphan"
    );
}

// =============================================================================
// API-level link resolution (P2.10)
// =============================================================================

#[tokio::test]
async fn test_resolve_link_deleted_namespace() {
    let app = test_app().await;

    // Create ns::child
    create_test_block(&app, "", "del_ns_root", "", "content").await;
    create_test_block(&app, "del_ns_root", "del_ns_child", "", "content").await;

    // Verify resolve works before deletion
    let (status, _) = json_request(
        &app,
        http::Method::POST,
        "/api/resolve",
        Some(json!({ "path": "del_ns_root::del_ns_child" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Delete the root recursively (which also deletes children)
    let (status, body) = json_request(&app, http::Method::GET, "/api/blocks/roots", None).await;
    assert_eq!(status, StatusCode::OK);
    let roots = body.as_array().unwrap();
    let root = roots.iter().find(|r| r["name"] == "del_ns_root").unwrap();
    let root_id = root["id"].as_str().unwrap();

    let (status, _) = json_request(
        &app,
        http::Method::DELETE,
        &format!("/api/blocks/{root_id}/recursive"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Resolve should now 404
    let (status, body) = json_request(
        &app,
        http::Method::POST,
        "/api/resolve",
        Some(json!({ "path": "del_ns_root::del_ns_child" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn test_resolve_link_after_move() {
    let app = test_app().await;

    // Create source::target
    create_test_block(&app, "", "mv_src_ns", "", "content").await;
    let (target_id, _) = create_test_block(&app, "mv_src_ns", "mv_target", "", "content").await;

    // Create destination namespace
    let (dst_id, _) = create_test_block(&app, "", "mv_dst_ns", "", "content").await;

    // Verify old path resolves
    let (status, _) = json_request(
        &app,
        http::Method::POST,
        "/api/resolve",
        Some(json!({ "path": "mv_src_ns::mv_target" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Move target to new parent
    let (status, _) = json_request(
        &app,
        http::Method::POST,
        &format!("/api/blocks/{target_id}/move"),
        Some(json!({ "parent_id": dst_id })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Old path should now 404
    let (status, _) = json_request(
        &app,
        http::Method::POST,
        "/api/resolve",
        Some(json!({ "path": "mv_src_ns::mv_target" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // New path should resolve
    let (status, body) = json_request(
        &app,
        http::Method::POST,
        "/api/resolve",
        Some(json!({ "path": "mv_dst_ns::mv_target" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["lineage_id"].is_string());
}

// =============================================================================
// Property Keys Endpoint
// =============================================================================

#[tokio::test]
async fn test_get_property_keys() {
    let app = test_app().await;
    let (parent_id, _) = create_test_block(&app, "", "pk_parent", "", "content").await;

    // Create children with properties
    json_request(
        &app,
        http::Method::POST,
        "/api/blocks",
        Some(json!({
            "namespace": "pk_parent",
            "name": "pk_child1",
            "content": "",
            "content_type": "content",
            "properties": { "priority": "high", "status": "open" }
        })),
    )
    .await;

    json_request(
        &app,
        http::Method::POST,
        "/api/blocks",
        Some(json!({
            "namespace": "pk_parent",
            "name": "pk_child2",
            "content": "",
            "content_type": "content",
            "properties": { "status": "closed", "assignee": "alice" }
        })),
    )
    .await;

    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{parent_id}/property-keys"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let keys = body.as_array().unwrap();
    let key_strs: Vec<&str> = keys.iter().filter_map(|k| k.as_str()).collect();
    assert!(key_strs.contains(&"priority"), "should contain 'priority'");
    assert!(key_strs.contains(&"status"), "should contain 'status'");
    assert!(key_strs.contains(&"assignee"), "should contain 'assignee'");
}

#[tokio::test]
async fn test_get_property_keys_not_found() {
    let app = test_app().await;
    let fake_id = Uuid::now_v7();
    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{fake_id}/property-keys"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

// =============================================================================
// Subtree Graph Endpoint
// =============================================================================

#[tokio::test]
async fn test_subtree_graph_empty() {
    let app = test_app().await;

    let (status, body) = json_request(
        &app,
        http::Method::POST,
        "/api/graph/subtree",
        Some(json!({ "lineage_ids": [] })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["content_links"].as_array().unwrap().len(), 0);
    assert_eq!(body["edges"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_subtree_graph_with_edges() {
    let app = test_app().await;
    let (_, lin1) = create_test_block(&app, "", "sg_a", "", "content").await;
    let (_, lin2) = create_test_block(&app, "", "sg_b", "", "content").await;
    let (_, lin3) = create_test_block(&app, "", "sg_c", "", "content").await;

    // Create edge between lin1 and lin2
    json_request(
        &app,
        http::Method::POST,
        "/api/edges",
        Some(json!({
            "from_lineage_id": lin1,
            "to_lineage_id": lin2,
            "edge_type": "related",
        })),
    )
    .await;

    let (status, body) = json_request(
        &app,
        http::Method::POST,
        "/api/graph/subtree",
        Some(json!({ "lineage_ids": [lin1, lin2, lin3] })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let edges = body["edges"].as_array().unwrap();
    assert_eq!(
        edges.len(),
        1,
        "should find the one edge between lin1 and lin2"
    );
    assert_eq!(edges[0]["from_lineage_id"], lin1.to_string());
    assert_eq!(edges[0]["to_lineage_id"], lin2.to_string());
}

#[tokio::test]
async fn test_subtree_graph_too_many_ids() {
    let app = test_app().await;

    // Build a list of 1001 fake UUIDs
    let ids: Vec<Uuid> = (0..1001).map(|_| Uuid::now_v7()).collect();

    let (status, body) = json_request(
        &app,
        http::Method::POST,
        "/api/graph/subtree",
        Some(json!({ "lineage_ids": ids })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("Too many"));
}

// =============================================================================
// Atom Edges Happy Path
// =============================================================================

#[tokio::test]
async fn test_get_atom_edges_with_data() {
    let app = test_app().await;
    let (_, lin_a) = create_test_block(&app, "", "ae_a", "", "content").await;
    let (_, lin_b) = create_test_block(&app, "", "ae_b", "", "content").await;
    let (_, lin_c) = create_test_block(&app, "", "ae_c", "", "content").await;

    // Edge from A→B
    json_request(
        &app,
        http::Method::POST,
        "/api/edges",
        Some(json!({
            "from_lineage_id": lin_a,
            "to_lineage_id": lin_b,
            "edge_type": "outgoing_test",
        })),
    )
    .await;

    // Edge from C→A
    json_request(
        &app,
        http::Method::POST,
        "/api/edges",
        Some(json!({
            "from_lineage_id": lin_c,
            "to_lineage_id": lin_a,
            "edge_type": "incoming_test",
        })),
    )
    .await;

    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/atoms/{lin_a}/edges"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let outgoing = body["outgoing"].as_array().unwrap();
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0]["edge_type"], "outgoing_test");
    assert_eq!(outgoing[0]["to_lineage_id"], lin_b.to_string());

    let incoming = body["incoming"].as_array().unwrap();
    assert_eq!(incoming.len(), 1);
    assert_eq!(incoming[0]["edge_type"], "incoming_test");
    assert_eq!(incoming[0]["from_lineage_id"], lin_c.to_string());
}

// =============================================================================
// Atom References Happy Path
// =============================================================================

#[tokio::test]
async fn test_get_atom_references_with_data() {
    let app = test_app().await;
    let (_, lin_target) = create_test_block(&app, "", "ref_target", "", "content").await;
    let (_, lin_src1) = create_test_block(&app, "", "ref_src1", "", "content").await;
    let (_, lin_src2) = create_test_block(&app, "", "ref_src2", "", "content").await;

    // Create edges pointing to target
    json_request(
        &app,
        http::Method::POST,
        "/api/edges",
        Some(json!({
            "from_lineage_id": lin_src1,
            "to_lineage_id": lin_target,
            "edge_type": "refs",
        })),
    )
    .await;
    json_request(
        &app,
        http::Method::POST,
        "/api/edges",
        Some(json!({
            "from_lineage_id": lin_src2,
            "to_lineage_id": lin_target,
            "edge_type": "refs",
        })),
    )
    .await;

    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/atoms/{lin_target}/references"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let refs = body.as_array().unwrap();
    assert_eq!(refs.len(), 2);
    let ref_lineages: Vec<&str> = refs
        .iter()
        .filter_map(|r| r["lineage_id"].as_str())
        .collect();
    assert!(ref_lineages.contains(&lin_src1.to_string().as_str()));
    assert!(ref_lineages.contains(&lin_src2.to_string().as_str()));
}

// =============================================================================
// Atom Rendered 404
// =============================================================================

#[tokio::test]
async fn test_get_atom_rendered_not_found() {
    let app = test_app().await;
    let fake_id = Uuid::now_v7();
    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/atoms/{fake_id}/rendered"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

// =============================================================================
// Update Atom — content_type and properties
// =============================================================================

#[tokio::test]
async fn test_update_atom_content_type() {
    let app = test_app().await;
    let (_, lineage_id) = create_test_block(&app, "", "ct_upd", "original", "content").await;

    let (status, body) = json_request(
        &app,
        http::Method::PUT,
        &format!("/api/atoms/{lineage_id}"),
        Some(json!({
            "content": "updated",
            "content_type": "markdown",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["content_type"], "markdown");
    assert_eq!(body["content_template"], "updated");
}

#[tokio::test]
async fn test_update_atom_properties() {
    let app = test_app().await;
    let (_, lineage_id) = create_test_block(&app, "", "prop_upd", "content", "content").await;

    let (status, body) = json_request(
        &app,
        http::Method::PUT,
        &format!("/api/atoms/{lineage_id}"),
        Some(json!({
            "content": "content",
            "properties": { "custom_key": "custom_value" },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["properties"]["custom_key"], "custom_value");
}

// =============================================================================
// Update Block — position
// =============================================================================

#[tokio::test]
async fn test_update_block_position() {
    let app = test_app().await;
    let (block_id, _) = create_test_block(&app, "", "pos_upd", "", "content").await;

    let (status, body) = json_request(
        &app,
        http::Method::PUT,
        &format!("/api/blocks/{block_id}"),
        Some(json!({ "position": "a080" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["position"], "a080");
}

// =============================================================================
// Move Block — to root
// =============================================================================

#[tokio::test]
async fn test_move_block_to_root() {
    let app = test_app().await;
    let (parent_id, _) = create_test_block(&app, "", "mv_root_p", "", "content").await;
    let (child_id, _) = create_test_block(&app, "mv_root_p", "mv_root_c", "", "content").await;

    // Move child to root (parent_id = null)
    let (status, body) = json_request(
        &app,
        http::Method::POST,
        &format!("/api/blocks/{child_id}/move"),
        Some(json!({ "parent_id": null })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["parent_id"].is_null(),
        "parent_id should be null after move to root"
    );

    // Verify the child appears in roots
    let (_, roots) = json_request(&app, http::Method::GET, "/api/blocks/roots", None).await;
    let root_names: Vec<&str> = roots
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|b| b["name"].as_str())
        .collect();
    assert!(
        root_names.contains(&"mv_root_c"),
        "moved block should appear in roots"
    );

    // Verify old parent has no children
    let (_, children) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{parent_id}/children"),
        None,
    )
    .await;
    assert_eq!(children.as_array().unwrap().len(), 0);
}

// =============================================================================
// Restore Recursive — 404
// =============================================================================

#[tokio::test]
async fn test_restore_recursive_nonexistent_restores_zero() {
    let app = test_app().await;
    let fake_id = Uuid::now_v7();
    let (status, body) = json_request(
        &app,
        http::Method::POST,
        &format!("/api/blocks/{fake_id}/restore-recursive"),
        None,
    )
    .await;
    // Bulk restore is a no-op for nonexistent IDs — returns 200 with count 0
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["restored"], 0);
}

// =============================================================================
// List Blocks — lineage_id filter and default (no params)
// =============================================================================

#[tokio::test]
async fn test_list_blocks_by_lineage_id() {
    let app = test_app().await;
    let (_, lineage_id) = create_test_block(&app, "", "lin_filter", "", "content").await;

    let (status, body) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks?lineage_id={lineage_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["lineage_id"], lineage_id.to_string());
}

#[tokio::test]
async fn test_list_blocks_no_params_returns_roots() {
    let app = test_app().await;
    create_test_block(&app, "", "np_root1", "", "content").await;
    create_test_block(&app, "", "np_root2", "", "content").await;
    create_test_block(&app, "np_root1", "np_child", "", "content").await;

    let (status, body) = json_request(&app, http::Method::GET, "/api/blocks", None).await;
    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().unwrap();
    let names: Vec<&str> = arr.iter().filter_map(|b| b["name"].as_str()).collect();
    assert!(names.contains(&"np_root1"));
    assert!(names.contains(&"np_root2"));
    // Child should not appear in root listing
    assert!(
        !names.contains(&"np_child"),
        "child blocks should not appear in no-params listing"
    );
}

// =============================================================================
// Create Block — with position and properties
// =============================================================================

#[tokio::test]
async fn test_create_block_with_position() {
    let app = test_app().await;
    create_test_block(&app, "", "pos_parent", "", "content").await;

    let (status, body) = json_request(
        &app,
        http::Method::POST,
        "/api/blocks",
        Some(json!({
            "namespace": "pos_parent",
            "name": "pos_child",
            "content": "",
            "content_type": "content",
            "position": "a080",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Verify the position was set by reading the block
    let block_id: Uuid = body["block_id"].as_str().unwrap().parse().unwrap();
    let (_, block) = json_request(
        &app,
        http::Method::GET,
        &format!("/api/blocks/{block_id}"),
        None,
    )
    .await;
    assert_eq!(block["position"], "a080");
}

// =============================================================================
// Resolve Link — relative paths
// =============================================================================

#[tokio::test]
async fn test_resolve_link_relative_child() {
    let app = test_app().await;
    create_test_block(&app, "", "rel_parent", "", "content").await;
    create_test_block(&app, "rel_parent", "rel_child", "", "content").await;

    // Resolve ./rel_child from rel_parent context
    let (status, body) = json_request(
        &app,
        http::Method::POST,
        "/api/resolve",
        Some(json!({ "path": "./rel_child", "from_namespace": "rel_parent" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["namespace"], "rel_parent::rel_child");
}

#[tokio::test]
async fn test_resolve_link_relative_sibling() {
    let app = test_app().await;
    create_test_block(&app, "", "sib_parent", "", "content").await;
    create_test_block(&app, "sib_parent", "sib_a", "", "content").await;
    create_test_block(&app, "sib_parent", "sib_b", "", "content").await;

    // Resolve ../sib_b from sib_parent::sib_a context
    let (status, body) = json_request(
        &app,
        http::Method::POST,
        "/api/resolve",
        Some(json!({ "path": "../sib_b", "from_namespace": "sib_parent::sib_a" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["namespace"], "sib_parent::sib_b");
}

#[tokio::test]
async fn test_resolve_schema_deleted() {
    let app = test_app().await;

    // Create a schema under types
    let (schema_id, _) = create_test_block(&app, "types", "del_schema_test", "", "schema").await;

    // Verify schema resolves
    let (status, _) = json_request(
        &app,
        http::Method::POST,
        "/api/schemas/resolve",
        Some(json!({ "type_name": "del_schema_test" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Delete the schema block
    let (status, _) = json_request(
        &app,
        http::Method::DELETE,
        &format!("/api/blocks/{schema_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Resolve should now 404
    let (status, body) = json_request(
        &app,
        http::Method::POST,
        "/api/schemas/resolve",
        Some(json!({ "type_name": "del_schema_test" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}
