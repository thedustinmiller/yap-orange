//! Tests for server startup initialization: ensure_meta_schema and ensure_settings.
//!
//! Uses in-memory SQLite for fast, isolated tests.
//! Run with: cargo test -p yap-server --test startup_tests -- --nocapture

use std::sync::Arc;

use yap_core::Store;
use yap_store_sqlite::{SqliteStore, run_migrations};

async fn setup_store() -> Arc<dyn Store> {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("connect to in-memory SQLite");

    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&pool)
        .await
        .expect("enable foreign keys");

    run_migrations(&pool).await.expect("run migrations");
    Arc::new(SqliteStore::new(pool))
}

// =============================================================================
// ensure_meta_schema
// =============================================================================

#[tokio::test]
async fn test_ensure_meta_schema_creates_types() {
    let db = setup_store().await;

    yap_server::ensure_meta_schema(db.as_ref()).await.unwrap();

    let types_block = db
        .find_block_by_namespace("types")
        .await
        .unwrap()
        .expect("types block should exist");

    let atom = db.get_atom(types_block.lineage_id).await.unwrap();
    assert_eq!(atom.content_type, "type_registry");
}

#[tokio::test]
async fn test_ensure_meta_schema_creates_schema() {
    let db = setup_store().await;

    yap_server::ensure_meta_schema(db.as_ref()).await.unwrap();

    let schema_block = db
        .find_block_by_namespace("types::schema")
        .await
        .unwrap()
        .expect("types::schema block should exist");

    let atom = db.get_atom(schema_block.lineage_id).await.unwrap();
    assert_eq!(atom.content_type, "schema");

    // Verify it has the expected meta-fields
    let fields = atom
        .properties
        .get("fields")
        .expect("schema should have fields");
    let fields_arr = fields.as_array().expect("fields should be an array");
    let field_names: Vec<&str> = fields_arr
        .iter()
        .filter_map(|f| f.get("name").and_then(|n| n.as_str()))
        .collect();
    assert!(field_names.contains(&"name"), "should have 'name' field");
    assert!(field_names.contains(&"type"), "should have 'type' field");
    assert!(
        field_names.contains(&"required"),
        "should have 'required' field"
    );
}

#[tokio::test]
async fn test_ensure_meta_schema_idempotent() {
    let db = setup_store().await;

    yap_server::ensure_meta_schema(db.as_ref()).await.unwrap();
    yap_server::ensure_meta_schema(db.as_ref()).await.unwrap();

    // Should still have exactly one types block and one types::schema block
    let types_block = db
        .find_block_by_namespace("types")
        .await
        .unwrap()
        .expect("types block should exist");

    let children = db.get_block_children(types_block.id).await.unwrap();
    let schema_children: Vec<_> = children.iter().filter(|c| c.name == "schema").collect();
    assert_eq!(
        schema_children.len(),
        1,
        "should have exactly one schema child"
    );
}

// =============================================================================
// ensure_settings
// =============================================================================

#[tokio::test]
async fn test_ensure_settings_creates_ui() {
    let db = setup_store().await;

    yap_server::ensure_settings(db.as_ref()).await.unwrap();

    let settings_block = db
        .find_block_by_namespace("settings::ui")
        .await
        .unwrap()
        .expect("settings::ui block should exist");

    let atom = db.get_atom(settings_block.lineage_id).await.unwrap();
    assert_eq!(atom.content_type, "setting");
    assert_eq!(
        atom.properties.get("theme").and_then(|v| v.as_str()),
        Some("dark")
    );
    assert_eq!(
        atom.properties.get("font_size").and_then(|v| v.as_i64()),
        Some(13)
    );
}

#[tokio::test]
async fn test_ensure_settings_idempotent() {
    let db = setup_store().await;

    yap_server::ensure_settings(db.as_ref()).await.unwrap();
    yap_server::ensure_settings(db.as_ref()).await.unwrap();

    let settings_block = db
        .find_block_by_namespace("settings")
        .await
        .unwrap()
        .expect("settings block should exist");

    let children = db.get_block_children(settings_block.id).await.unwrap();
    let ui_children: Vec<_> = children.iter().filter(|c| c.name == "ui").collect();
    assert_eq!(ui_children.len(), 1, "should have exactly one ui child");
}

#[tokio::test]
async fn test_ensure_settings_preserves_existing() {
    let db = setup_store().await;

    yap_server::ensure_settings(db.as_ref()).await.unwrap();

    // Modify the settings
    let settings_block = db
        .find_block_by_namespace("settings::ui")
        .await
        .unwrap()
        .unwrap();

    let modified_props = serde_json::json!({
        "name": "ui",
        "theme": "light",
        "font_size": 16,
        "custom_key": "custom_value"
    });
    db.edit_lineage(
        settings_block.lineage_id,
        "setting",
        "",
        &[],
        &modified_props,
    )
    .await
    .unwrap();

    // Call ensure_settings again — should not overwrite
    yap_server::ensure_settings(db.as_ref()).await.unwrap();

    // Settings should still be the modified version
    let settings_block = db
        .find_block_by_namespace("settings::ui")
        .await
        .unwrap()
        .unwrap();
    let atom = db.get_atom(settings_block.lineage_id).await.unwrap();
    assert_eq!(
        atom.properties.get("theme").and_then(|v| v.as_str()),
        Some("light"),
        "ensure_settings should not overwrite existing settings"
    );
    assert_eq!(
        atom.properties.get("custom_key").and_then(|v| v.as_str()),
        Some("custom_value"),
        "custom settings should be preserved"
    );
}

// =============================================================================
// Additional ensure_meta_schema tests
// =============================================================================

#[tokio::test]
async fn test_ensure_meta_schema_upgrades_content_type() {
    let db = setup_store().await;

    // Manually create `types` — this gives it content_type="namespace"
    db.ensure_namespace_block("types").await.unwrap();

    let types_block = db.find_block_by_namespace("types").await.unwrap().unwrap();
    let atom_before = db.get_atom(types_block.lineage_id).await.unwrap();
    assert_eq!(
        atom_before.content_type, "namespace",
        "precondition: types starts as namespace"
    );

    // Now call ensure_meta_schema which should upgrade it
    yap_server::ensure_meta_schema(db.as_ref()).await.unwrap();

    let atom_after = db.get_atom(types_block.lineage_id).await.unwrap();
    assert_eq!(
        atom_after.content_type, "type_registry",
        "types block should be upgraded to type_registry"
    );
}

#[tokio::test]
async fn test_ensure_meta_schema_schema_has_all_five_fields() {
    let db = setup_store().await;

    yap_server::ensure_meta_schema(db.as_ref()).await.unwrap();

    let schema_block = db
        .find_block_by_namespace("types::schema")
        .await
        .unwrap()
        .expect("types::schema should exist");

    let atom = db.get_atom(schema_block.lineage_id).await.unwrap();
    let fields = atom
        .properties
        .get("fields")
        .expect("schema should have fields");
    let fields_arr = fields.as_array().expect("fields should be an array");

    // Exactly 5 fields
    assert_eq!(fields_arr.len(), 5, "schema should have exactly 5 fields");

    // Verify field names
    let field_names: Vec<&str> = fields_arr
        .iter()
        .filter_map(|f| f.get("name").and_then(|n| n.as_str()))
        .collect();
    assert_eq!(
        field_names,
        vec!["name", "type", "options", "required", "target_type"],
        "field names should match in order"
    );

    // Verify field types
    let field_types: Vec<&str> = fields_arr
        .iter()
        .filter_map(|f| f.get("type").and_then(|t| t.as_str()))
        .collect();
    assert_eq!(
        field_types,
        vec!["string", "enum", "text", "boolean", "string"],
        "field types should match in order"
    );

    // Verify the "type" field has 7 enum options
    let type_field = fields_arr
        .iter()
        .find(|f| f.get("name").and_then(|n| n.as_str()) == Some("type"))
        .expect("should have a 'type' field");
    let options = type_field
        .get("options")
        .expect("type field should have options")
        .as_array()
        .expect("options should be an array");
    let option_strs: Vec<&str> = options.iter().filter_map(|o| o.as_str()).collect();
    assert_eq!(
        option_strs,
        vec!["string", "number", "boolean", "date", "enum", "ref", "text"],
        "type field should have 7 enum options"
    );
}

// =============================================================================
// Additional ensure_settings tests
// =============================================================================

#[tokio::test]
async fn test_ensure_settings_default_values_complete() {
    let db = setup_store().await;

    yap_server::ensure_settings(db.as_ref()).await.unwrap();

    let ui_block = db
        .find_block_by_namespace("settings::ui")
        .await
        .unwrap()
        .expect("settings::ui should exist");

    let atom = db.get_atom(ui_block.lineage_id).await.unwrap();

    // Check default values
    assert_eq!(
        atom.properties.get("theme").and_then(|v| v.as_str()),
        Some("dark"),
        "default theme should be dark"
    );
    assert_eq!(
        atom.properties.get("font_size").and_then(|v| v.as_i64()),
        Some(13),
        "default font_size should be 13"
    );

    // The only keys should be "theme", "font_size", and the auto-injected "name"
    let obj = atom
        .properties
        .as_object()
        .expect("properties should be an object");
    let non_name_keys: Vec<&String> = obj.keys().filter(|k| k.as_str() != "name").collect();
    assert_eq!(
        non_name_keys.len(),
        2,
        "should have exactly 2 non-name keys (theme and font_size), got: {:?}",
        non_name_keys
    );
    assert!(obj.contains_key("theme"), "should have theme key");
    assert!(obj.contains_key("font_size"), "should have font_size key");
}

#[tokio::test]
async fn test_ensure_settings_creates_ui_when_settings_preexists() {
    let db = setup_store().await;

    // Manually create the settings namespace first
    db.ensure_namespace_block("settings").await.unwrap();

    // Verify settings::ui does NOT exist yet
    assert!(
        db.find_block_by_namespace("settings::ui")
            .await
            .unwrap()
            .is_none(),
        "precondition: settings::ui should not exist yet"
    );

    // Now call ensure_settings
    yap_server::ensure_settings(db.as_ref()).await.unwrap();

    // settings::ui should now exist
    let ui_block = db
        .find_block_by_namespace("settings::ui")
        .await
        .unwrap()
        .expect("settings::ui should exist after ensure_settings");

    let atom = db.get_atom(ui_block.lineage_id).await.unwrap();
    assert_eq!(
        atom.content_type, "setting",
        "content_type should be 'setting'"
    );
    assert_eq!(
        atom.properties.get("theme").and_then(|v| v.as_str()),
        Some("dark"),
        "default theme should be dark"
    );
    assert_eq!(
        atom.properties.get("font_size").and_then(|v| v.as_i64()),
        Some(13),
        "default font_size should be 13"
    );
}

// =============================================================================
// Full startup sequence
// =============================================================================

#[tokio::test]
async fn test_full_startup_sequence() {
    let db = setup_store().await;

    // Run both startup functions
    yap_server::ensure_meta_schema(db.as_ref()).await.unwrap();
    yap_server::ensure_settings(db.as_ref()).await.unwrap();

    // All 4 blocks should exist
    let types_block = db
        .find_block_by_namespace("types")
        .await
        .unwrap()
        .expect("types should exist");
    let _schema_block = db
        .find_block_by_namespace("types::schema")
        .await
        .unwrap()
        .expect("types::schema should exist");
    let settings_block = db
        .find_block_by_namespace("settings")
        .await
        .unwrap()
        .expect("settings should exist");
    let _ui_block = db
        .find_block_by_namespace("settings::ui")
        .await
        .unwrap()
        .expect("settings::ui should exist");

    // Call both again — should be idempotent
    yap_server::ensure_meta_schema(db.as_ref()).await.unwrap();
    yap_server::ensure_settings(db.as_ref()).await.unwrap();

    // Verify no duplicate children
    let types_children = db.get_block_children(types_block.id).await.unwrap();
    let schema_children: Vec<_> = types_children
        .iter()
        .filter(|c| c.name == "schema")
        .collect();
    assert_eq!(
        schema_children.len(),
        1,
        "types should still have exactly 1 child named 'schema'"
    );

    let settings_children = db.get_block_children(settings_block.id).await.unwrap();
    let ui_children: Vec<_> = settings_children
        .iter()
        .filter(|c| c.name == "ui")
        .collect();
    assert_eq!(
        ui_children.len(),
        1,
        "settings should still have exactly 1 child named 'ui'"
    );
}

#[tokio::test]
async fn test_ensure_meta_schema_creates_schema_when_types_preexists() {
    let db = setup_store().await;

    // Manually create the types namespace first
    db.ensure_namespace_block("types").await.unwrap();

    // Verify types::schema does NOT exist yet
    assert!(
        db.find_block_by_namespace("types::schema")
            .await
            .unwrap()
            .is_none(),
        "precondition: types::schema should not exist yet"
    );

    // Verify types block has content_type="namespace"
    let types_block = db.find_block_by_namespace("types").await.unwrap().unwrap();
    let atom_before = db.get_atom(types_block.lineage_id).await.unwrap();
    assert_eq!(atom_before.content_type, "namespace");

    // Now call ensure_meta_schema
    yap_server::ensure_meta_schema(db.as_ref()).await.unwrap();

    // types::schema should now exist
    let schema_block = db
        .find_block_by_namespace("types::schema")
        .await
        .unwrap()
        .expect("types::schema should exist after ensure_meta_schema");

    let schema_atom = db.get_atom(schema_block.lineage_id).await.unwrap();
    assert_eq!(schema_atom.content_type, "schema");

    // types block should have been upgraded
    let atom_after = db.get_atom(types_block.lineage_id).await.unwrap();
    assert_eq!(
        atom_after.content_type, "type_registry",
        "types content_type should be upgraded to type_registry"
    );
}

#[tokio::test]
async fn test_ensure_meta_schema_idempotent_no_extra_atoms() {
    let db = setup_store().await;

    // First call
    yap_server::ensure_meta_schema(db.as_ref()).await.unwrap();

    let types_block = db.find_block_by_namespace("types").await.unwrap().unwrap();
    let lineage_v1 = db.get_lineage(types_block.lineage_id).await.unwrap();
    let v1 = lineage_v1.version;

    // Second call — should be a no-op since content_type is already "type_registry"
    yap_server::ensure_meta_schema(db.as_ref()).await.unwrap();

    let lineage_v2 = db.get_lineage(types_block.lineage_id).await.unwrap();
    let v2 = lineage_v2.version;

    assert_eq!(
        v1, v2,
        "Second ensure_meta_schema should not create extra atom versions (v1={}, v2={})",
        v1, v2
    );
}
