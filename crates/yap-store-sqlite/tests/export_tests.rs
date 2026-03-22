//! Integration tests for tree export/import against SQLite.
//!
//! Ported from crates/yap-store-pg/tests/export_tests.rs.
//! Each test gets an in-memory SQLite DB — no teardown needed.
//!
//! Run with: cargo test -p yap-store-sqlite --test export_tests -- --nocapture

use std::collections::HashSet;
use uuid::Uuid;

use yap_core::Store;
use yap_core::export::{
    ExportOptions, ImportMode, ImportOptions, MatchStrategy, compute_export_hash, export_tree,
    import_tree,
};
use yap_store_sqlite::{SqliteStore, run_migrations};

fn default_export_options() -> ExportOptions {
    ExportOptions::default()
}

// =============================================================================
// Setup
// =============================================================================

async fn setup_store() -> SqliteStore {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("connect to in-memory SQLite");

    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&pool)
        .await
        .expect("enable foreign keys");

    run_migrations(&pool).await.expect("run migrations");
    SqliteStore::new(pool)
}

/// Create a unique run block under test::export for test isolation.
async fn setup_root(db: &SqliteStore, label: &str) -> Uuid {
    let parent_id = db
        .ensure_namespace_block("test::export")
        .await
        .expect("ensure test::export namespace");
    let name = format!("{}_{}", label, Uuid::now_v7().simple());
    let (block, _) = db
        .create_block(
            Some(parent_id),
            &name,
            "",
            "namespace",
            &serde_json::json!({}),
        )
        .await
        .expect("create test root");
    block.id
}

// =============================================================================
// Tests
// =============================================================================

/// Pure function test — no DB required.
#[test]
fn test_export_hash_stable() {
    let h1 = compute_export_hash("content", "hello world", &[]);
    let h2 = compute_export_hash("content", "hello world", &[]);
    assert_eq!(h1, h2, "same inputs → same hash");

    let h3 = compute_export_hash("content", "different content", &[]);
    assert_ne!(h1, h3, "different content → different hash");

    let h4 = compute_export_hash("todo", "hello world", &[]);
    assert_ne!(h1, h4, "different content_type → different hash");

    let h5 = compute_export_hash("content", "See {0} and {1}", &[0, 1]);
    let h6 = compute_export_hash("content", "See {0} and {1}", &[1, 0]);
    assert_eq!(
        h5, h6,
        "sorted internal IDs → same hash regardless of input order"
    );

    let h7 = compute_export_hash("content", "See {0} and {1}", &[0, 2]);
    assert_ne!(h5, h7, "different link targets → different hash");
}

#[tokio::test]
async fn test_export_basic() {
    let db = setup_store().await;
    let root_id = setup_root(&db, "export_basic").await;

    let (child1, _) = db
        .create_block(
            Some(root_id),
            "child1",
            "Content of child1",
            "content",
            &serde_json::json!({}),
        )
        .await
        .unwrap();
    let (child2, _) = db
        .create_block(
            Some(root_id),
            "child2",
            "Content of child2",
            "content",
            &serde_json::json!({}),
        )
        .await
        .unwrap();
    let _ = child2;

    let tree = export_tree(&db, root_id, &default_export_options())
        .await
        .expect("export failed");

    assert_eq!(tree.nodes.len(), 3);
    assert_eq!(tree.format, "yap-tree-v2");

    // v2 hashes should be present and non-empty
    for node in &tree.nodes {
        assert!(
            !node.content_identity_hash.is_empty(),
            "content_identity_hash should be set"
        );
        assert!(!node.merkle_hash.is_empty(), "merkle_hash should be set");
    }
    assert!(
        !tree.topology_hash.is_empty(),
        "topology_hash should be set"
    );

    let root_node = tree.nodes.iter().find(|n| n.local_id == 0).unwrap();
    assert!(root_node.parent_local_id.is_none());

    let children: Vec<_> = tree
        .nodes
        .iter()
        .filter(|n| n.parent_local_id == Some(0))
        .collect();
    assert_eq!(children.len(), 2);

    let names: HashSet<_> = children.iter().map(|n| n.name.as_str()).collect();
    assert!(names.contains("child1"));
    assert!(names.contains("child2"));

    assert_eq!(root_node.children_local_ids.len(), 2);

    let c1_node = tree.nodes.iter().find(|n| n.name == "child1").unwrap();
    assert!(c1_node.children_local_ids.is_empty());

    let _ = child1;
}

#[tokio::test]
async fn test_export_internal_links() {
    let db = setup_store().await;
    let root_id = setup_root(&db, "export_internal").await;

    let (target_block, target_atom) = db
        .create_block(
            Some(root_id),
            "target",
            "Target content",
            "content",
            &serde_json::json!({}),
        )
        .await
        .unwrap();

    let template = "See {0} for details.".to_string();
    let links = vec![target_block.lineage_id];
    let (linker_block, _) = db
        .create_block_with_content(
            Some(root_id),
            "linker",
            &template,
            &links,
            "content",
            &serde_json::json!({}),
        )
        .await
        .unwrap();

    let tree = export_tree(&db, root_id, &default_export_options())
        .await
        .expect("export failed");

    let linker_node = tree.nodes.iter().find(|n| n.name == "linker").unwrap();
    let target_node = tree.nodes.iter().find(|n| n.name == "target").unwrap();

    assert_eq!(linker_node.internal_links.len(), 1);
    assert!(linker_node.external_links.is_empty());
    assert_eq!(
        linker_node.internal_links[0].target_local_id,
        target_node.local_id
    );
    assert_eq!(linker_node.internal_links[0].placeholder_index, 0);

    let _ = (linker_block, target_atom);
}

#[tokio::test]
async fn test_export_external_links() {
    let db = setup_store().await;
    let root_id = setup_root(&db, "export_external").await;
    let ext_root_id = setup_root(&db, "export_external_ext").await;

    let (ext_block, _) = db
        .create_block(
            Some(ext_root_id),
            "external-note",
            "External content",
            "content",
            &serde_json::json!({}),
        )
        .await
        .unwrap();

    let template = "References {0}".to_string();
    let links = vec![ext_block.lineage_id];
    let (node_block, _) = db
        .create_block_with_content(
            Some(root_id),
            "referencer",
            &template,
            &links,
            "content",
            &serde_json::json!({}),
        )
        .await
        .unwrap();

    let tree = export_tree(&db, root_id, &default_export_options())
        .await
        .expect("export failed");

    let ref_node = tree.nodes.iter().find(|n| n.name == "referencer").unwrap();
    assert!(ref_node.internal_links.is_empty());
    assert_eq!(ref_node.external_links.len(), 1);
    assert_eq!(ref_node.external_links[0].placeholder_index, 0);
    assert!(
        ref_node.external_links[0]
            .target_path
            .contains("external-note")
    );

    let _ = node_block;
}

#[tokio::test]
async fn test_import_copy() {
    let db = setup_store().await;
    let src_root_id = setup_root(&db, "import_copy_src").await;
    let dst_root_id = setup_root(&db, "import_copy_dst").await;

    let (child_a, _) = db
        .create_block(
            Some(src_root_id),
            "node-a",
            "Node A",
            "content",
            &serde_json::json!({}),
        )
        .await
        .unwrap();
    let template = "Links to {0}".to_string();
    let links_b = vec![child_a.lineage_id];
    let (child_b, _) = db
        .create_block_with_content(
            Some(src_root_id),
            "node-b",
            &template,
            &links_b,
            "content",
            &serde_json::json!({}),
        )
        .await
        .unwrap();

    let tree = export_tree(&db, src_root_id, &default_export_options())
        .await
        .unwrap();
    let result = import_tree(
        &db,
        &tree,
        Some(dst_root_id),
        ImportOptions::from_mode(ImportMode::Copy),
    )
    .await
    .unwrap();

    assert_eq!(result.created, 3);
    assert_eq!(result.skipped, 0);
    assert!(result.root_block_id.is_some());

    let new_root_id = result.root_block_id.unwrap();
    let new_root = db.get_block(new_root_id).await.unwrap();
    assert_eq!(new_root.parent_id, Some(dst_root_id));

    let new_children = db.get_block_children(new_root_id).await.unwrap();
    assert_eq!(new_children.len(), 2);

    let new_b = new_children.iter().find(|c| c.name == "node-b").unwrap();
    let new_b_atom = db.get_atom(new_b.lineage_id).await.unwrap();
    assert_eq!(new_b_atom.links.len(), 1);

    let new_a = new_children.iter().find(|c| c.name == "node-a").unwrap();
    assert_eq!(new_b_atom.links[0], new_a.lineage_id);

    let _ = child_b;
}

#[tokio::test]
async fn test_import_merge() {
    let db = setup_store().await;
    let src_root_id = setup_root(&db, "import_merge_src").await;
    let dst_root_id = setup_root(&db, "import_merge_dst").await;

    db.create_block(
        Some(src_root_id),
        "note",
        "Some note",
        "content",
        &serde_json::json!({}),
    )
    .await
    .unwrap();

    let tree = export_tree(&db, src_root_id, &default_export_options())
        .await
        .unwrap();
    let result = import_tree(
        &db,
        &tree,
        Some(dst_root_id),
        ImportOptions::from_mode(ImportMode::Merge),
    )
    .await
    .unwrap();

    assert_eq!(result.created, 2);
    assert_eq!(result.skipped, 0);

    let new_root_id = result.root_block_id.unwrap();
    let new_root_block = db.get_block(new_root_id).await.unwrap();
    let new_root_atom = db.get_atom(new_root_block.lineage_id).await.unwrap();
    // v2 auto strategy uses ContentIdentity, so metadata key is _import_content_hash
    assert!(
        new_root_atom
            .properties
            .get("_import_content_hash")
            .is_some(),
        "v2 merge should inject _import_content_hash"
    );

    let children = db.get_block_children(new_root_id).await.unwrap();
    let note = children.iter().find(|c| c.name == "note").unwrap();
    let note_atom = db.get_atom(note.lineage_id).await.unwrap();
    assert!(
        note_atom.properties.get("_import_content_hash").is_some(),
        "v2 merge should inject _import_content_hash"
    );
}

#[tokio::test]
async fn test_import_merge_idempotent() {
    let db = setup_store().await;
    let src_root_id = setup_root(&db, "idempotent_src").await;
    let dst_root_id = setup_root(&db, "idempotent_dst").await;

    db.create_block(
        Some(src_root_id),
        "alpha",
        "Alpha",
        "content",
        &serde_json::json!({}),
    )
    .await
    .unwrap();
    db.create_block(
        Some(src_root_id),
        "beta",
        "Beta",
        "content",
        &serde_json::json!({}),
    )
    .await
    .unwrap();

    let tree = export_tree(&db, src_root_id, &default_export_options())
        .await
        .unwrap();

    let r1 = import_tree(
        &db,
        &tree,
        Some(dst_root_id),
        ImportOptions::from_mode(ImportMode::Merge),
    )
    .await
    .unwrap();
    assert_eq!(r1.created, 3);
    assert_eq!(r1.skipped, 0);

    let r2 = import_tree(
        &db,
        &tree,
        Some(dst_root_id),
        ImportOptions::from_mode(ImportMode::Merge),
    )
    .await
    .unwrap();
    assert_eq!(r2.created, 0, "second import should create nothing");
    assert_eq!(r2.skipped, 3, "second import should skip all 3 nodes");
}

#[tokio::test]
async fn test_import_merge_resolves_external() {
    let db = setup_store().await;
    let src_root_id = setup_root(&db, "resolve_ext_src").await;
    let dst_root_id = setup_root(&db, "resolve_ext_dst").await;

    let (ext_block, _) = db
        .create_block(
            Some(dst_root_id),
            "existing-target",
            "Target",
            "content",
            &serde_json::json!({}),
        )
        .await
        .unwrap();

    let template = "See {0}".to_string();
    let links = vec![ext_block.lineage_id];
    db.create_block_with_content(
        Some(src_root_id),
        "referencer",
        &template,
        &links,
        "content",
        &serde_json::json!({}),
    )
    .await
    .unwrap();

    let tree = export_tree(&db, src_root_id, &default_export_options())
        .await
        .unwrap();

    let ref_node = tree.nodes.iter().find(|n| n.name == "referencer").unwrap();
    assert_eq!(ref_node.external_links.len(), 1);

    let result = import_tree(
        &db,
        &tree,
        Some(dst_root_id),
        ImportOptions::from_mode(ImportMode::Merge),
    )
    .await
    .unwrap();

    assert!(result.failed_external_links.is_empty());

    let new_root_id = result.root_block_id.unwrap();
    let new_children = db.get_block_children(new_root_id).await.unwrap();
    let new_ref = new_children
        .iter()
        .find(|c| c.name == "referencer")
        .unwrap();
    let new_ref_atom = db.get_atom(new_ref.lineage_id).await.unwrap();

    assert_eq!(new_ref_atom.links.len(), 1);
    assert_eq!(new_ref_atom.links[0], ext_block.lineage_id);
}

#[tokio::test]
async fn test_roundtrip() {
    let db = setup_store().await;
    let src_root_id = setup_root(&db, "roundtrip_src").await;
    let dst_root_id = setup_root(&db, "roundtrip_dst").await;

    let (node_a, _) = db
        .create_block(
            Some(src_root_id),
            "node-a",
            "Node A",
            "content",
            &serde_json::json!({}),
        )
        .await
        .unwrap();
    db.create_block_with_content(
        Some(src_root_id),
        "node-b",
        "Links to {0}",
        &[node_a.lineage_id],
        "content",
        &serde_json::json!({}),
    )
    .await
    .unwrap();

    let tree1 = export_tree(&db, src_root_id, &default_export_options())
        .await
        .unwrap();

    let result = import_tree(
        &db,
        &tree1,
        Some(dst_root_id),
        ImportOptions::from_mode(ImportMode::Copy),
    )
    .await
    .unwrap();
    let new_root_id = result.root_block_id.unwrap();

    let tree2 = export_tree(&db, new_root_id, &default_export_options())
        .await
        .unwrap();

    assert_eq!(tree1.nodes.len(), tree2.nodes.len());

    let names1: HashSet<_> = tree1.nodes.iter().map(|n| n.name.as_str()).collect();
    let names2: HashSet<_> = tree2.nodes.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names1, names2);

    let internal1: usize = tree1.nodes.iter().map(|n| n.internal_links.len()).sum();
    let internal2: usize = tree2.nodes.iter().map(|n| n.internal_links.len()).sum();
    assert_eq!(internal1, internal2);

    for name in &names1 {
        let n1 = tree1.nodes.iter().find(|n| n.name == *name).unwrap();
        let n2 = tree2.nodes.iter().find(|n| n.name == *name).unwrap();
        assert_eq!(n1.content_template, n2.content_template);
        assert_eq!(n1.content_type, n2.content_type);
        assert_eq!(
            n1.content_identity_hash, n2.content_identity_hash,
            "content_identity_hash mismatch for {}",
            name
        );
        assert_eq!(
            n1.merkle_hash, n2.merkle_hash,
            "merkle_hash mismatch for {}",
            name
        );
    }
}

// =============================================================================
// v2 Hash Tests
// =============================================================================

#[tokio::test]
async fn test_export_v2_has_all_hashes() {
    let db = setup_store().await;
    let root_id = setup_root(&db, "v2_hashes").await;

    db.create_block(
        Some(root_id),
        "child",
        "content",
        "content",
        &serde_json::json!({}),
    )
    .await
    .unwrap();

    let tree = export_tree(&db, root_id, &default_export_options())
        .await
        .unwrap();

    assert_eq!(tree.format, "yap-tree-v2");
    assert!(!tree.topology_hash.is_empty());
    for node in &tree.nodes {
        assert!(!node.content_identity_hash.is_empty());
        assert!(!node.merkle_hash.is_empty());
    }
}

#[tokio::test]
async fn test_merkle_hash_changes_with_child_added() {
    let db = setup_store().await;
    let root_id = setup_root(&db, "merkle_child").await;

    db.create_block(
        Some(root_id),
        "alpha",
        "Alpha",
        "content",
        &serde_json::json!({}),
    )
    .await
    .unwrap();

    let tree1 = export_tree(&db, root_id, &default_export_options())
        .await
        .unwrap();
    let root_merkle_1 = &tree1.nodes[0].merkle_hash;

    db.create_block(
        Some(root_id),
        "beta",
        "Beta",
        "content",
        &serde_json::json!({}),
    )
    .await
    .unwrap();

    let tree2 = export_tree(&db, root_id, &default_export_options())
        .await
        .unwrap();
    let root_merkle_2 = &tree2.nodes[0].merkle_hash;

    assert_ne!(
        root_merkle_1, root_merkle_2,
        "Adding a child should change parent's merkle hash"
    );
}

#[tokio::test]
async fn test_merkle_hash_stable_across_exports() {
    let db = setup_store().await;
    let root_id = setup_root(&db, "merkle_stable").await;

    db.create_block(
        Some(root_id),
        "x",
        "X content",
        "content",
        &serde_json::json!({}),
    )
    .await
    .unwrap();

    let tree1 = export_tree(&db, root_id, &default_export_options())
        .await
        .unwrap();
    let tree2 = export_tree(&db, root_id, &default_export_options())
        .await
        .unwrap();

    for (n1, n2) in tree1.nodes.iter().zip(tree2.nodes.iter()) {
        assert_eq!(
            n1.merkle_hash, n2.merkle_hash,
            "Merkle hash should be stable across exports for node {}",
            n1.name
        );
    }
    assert_eq!(tree1.topology_hash, tree2.topology_hash);
}

#[tokio::test]
async fn test_content_identity_hash_includes_schema_props() {
    let db = setup_store().await;
    let root_id = setup_root(&db, "ci_schema_props").await;

    let props1 = serde_json::json!({"fields": [{"name": "title", "type": "string"}]});
    db.create_block(Some(root_id), "my-type", "", "schema", &props1)
        .await
        .unwrap();

    let tree1 = export_tree(&db, root_id, &default_export_options())
        .await
        .unwrap();
    let schema_node1 = tree1.nodes.iter().find(|n| n.name == "my-type").unwrap();

    // Create again with different properties in a new tree
    let root_id2 = setup_root(&db, "ci_schema_props2").await;
    let props2 = serde_json::json!({"fields": [{"name": "body", "type": "content"}]});
    db.create_block(Some(root_id2), "my-type", "", "schema", &props2)
        .await
        .unwrap();

    let tree2 = export_tree(&db, root_id2, &default_export_options())
        .await
        .unwrap();
    let schema_node2 = tree2.nodes.iter().find(|n| n.name == "my-type").unwrap();

    assert_ne!(
        schema_node1.content_identity_hash, schema_node2.content_identity_hash,
        "Schema with different properties should have different content_identity_hash"
    );
}

#[tokio::test]
async fn test_content_identity_hash_excludes_text_props() {
    let db = setup_store().await;
    let root_id = setup_root(&db, "ci_text_props").await;

    let props1 = serde_json::json!({"color": "red"});
    db.create_block(Some(root_id), "note", "Hello", "content", &props1)
        .await
        .unwrap();

    let tree1 = export_tree(&db, root_id, &default_export_options())
        .await
        .unwrap();
    let note1 = tree1.nodes.iter().find(|n| n.name == "note").unwrap();

    let root_id2 = setup_root(&db, "ci_text_props2").await;
    let props2 = serde_json::json!({"color": "blue"});
    db.create_block(Some(root_id2), "note", "Hello", "content", &props2)
        .await
        .unwrap();

    let tree2 = export_tree(&db, root_id2, &default_export_options())
        .await
        .unwrap();
    let note2 = tree2.nodes.iter().find(|n| n.name == "note").unwrap();

    assert_eq!(
        note1.content_identity_hash, note2.content_identity_hash,
        "Text nodes with different properties should have same content_identity_hash"
    );
}

#[tokio::test]
async fn test_import_v1_compat() {
    let db = setup_store().await;
    let dst_root_id = setup_root(&db, "v1_compat_dst").await;

    // Manually construct a v1-format tree
    let tree = yap_core::export::ExportTree {
        format: "yap-tree-v1".to_string(),
        exported_at: chrono::Utc::now(),
        source_namespace: "test".to_string(),
        nodes: vec![yap_core::export::ExportNode {
            local_id: 0,
            name: "v1-root".to_string(),
            content_type: "content".to_string(),
            content_template: "v1 content".to_string(),
            internal_links: vec![],
            external_links: vec![],
            properties: serde_json::json!({}),
            export_hash: compute_export_hash("content", "v1 content", &[]),
            parent_local_id: None,
            position: "a0".to_string(),
            children_local_ids: vec![],
            content_identity_hash: String::new(),
            merkle_hash: String::new(),
        }],
        edges: vec![],
        topology_hash: String::new(),
    };

    // Auto strategy should select ExportHash for v1 format
    let result = import_tree(
        &db,
        &tree,
        Some(dst_root_id),
        ImportOptions::from_mode(ImportMode::Merge),
    )
    .await
    .unwrap();

    assert_eq!(result.created, 1);

    // Verify _import_hash (v1 metadata key) was injected
    let root_id = result.root_block_id.unwrap();
    let root_block = db.get_block(root_id).await.unwrap();
    let root_atom = db.get_atom(root_block.lineage_id).await.unwrap();
    assert!(
        root_atom.properties.get("_import_hash").is_some(),
        "v1 compat should use _import_hash metadata"
    );

    // Second import should skip (idempotent via _import_hash)
    let r2 = import_tree(
        &db,
        &tree,
        Some(dst_root_id),
        ImportOptions::from_mode(ImportMode::Merge),
    )
    .await
    .unwrap();
    assert_eq!(r2.skipped, 1);
}

#[tokio::test]
async fn test_import_content_identity_match() {
    let db = setup_store().await;
    let src_root_id = setup_root(&db, "ci_match_src").await;
    let dst_root_id = setup_root(&db, "ci_match_dst").await;

    db.create_block(
        Some(src_root_id),
        "note",
        "Same content",
        "content",
        &serde_json::json!({}),
    )
    .await
    .unwrap();

    // Create same content under dst with same name
    db.create_block(
        Some(dst_root_id),
        "note",
        "Same content",
        "content",
        &serde_json::json!({}),
    )
    .await
    .unwrap();

    let tree = export_tree(&db, src_root_id, &default_export_options())
        .await
        .unwrap();

    let options = ImportOptions {
        mode: ImportMode::Merge,
        match_strategy: MatchStrategy::ContentIdentity,
        global_link: false,
        replace_existing: false,
    };
    let result = import_tree(&db, &tree, Some(dst_root_id), options)
        .await
        .unwrap();

    // Root node and child — see PG test for explanation of matching logic
    assert_eq!(result.created, 2);
}

#[tokio::test]
async fn test_import_merkle_match() {
    let db = setup_store().await;
    let src_root_id = setup_root(&db, "merkle_match_src").await;
    let dst_root_id = setup_root(&db, "merkle_match_dst").await;

    db.create_block(
        Some(src_root_id),
        "child",
        "Content",
        "content",
        &serde_json::json!({}),
    )
    .await
    .unwrap();

    let tree = export_tree(&db, src_root_id, &default_export_options())
        .await
        .unwrap();

    // First import (copy mode to seed dst)
    let r1 = import_tree(
        &db,
        &tree,
        Some(dst_root_id),
        ImportOptions::from_mode(ImportMode::Copy),
    )
    .await
    .unwrap();
    assert_eq!(r1.created, 2);

    // Second import with Merkle strategy should skip since subtree matches
    let options = ImportOptions {
        mode: ImportMode::Merge,
        match_strategy: MatchStrategy::Merkle,
        global_link: false,
        replace_existing: false,
    };
    let r2 = import_tree(&db, &tree, Some(dst_root_id), options)
        .await
        .unwrap();
    assert_eq!(r2.skipped, 2, "Merkle should skip entire matching subtree");
    assert_eq!(r2.created, 0);
}

#[tokio::test]
async fn test_import_merkle_no_match_on_child_change() {
    let db = setup_store().await;
    let src_root_id = setup_root(&db, "merkle_nomatch_src").await;
    let dst_root_id = setup_root(&db, "merkle_nomatch_dst").await;

    db.create_block(
        Some(src_root_id),
        "child",
        "Original",
        "content",
        &serde_json::json!({}),
    )
    .await
    .unwrap();

    let tree = export_tree(&db, src_root_id, &default_export_options())
        .await
        .unwrap();

    // Import first version via copy
    import_tree(
        &db,
        &tree,
        Some(dst_root_id),
        ImportOptions::from_mode(ImportMode::Copy),
    )
    .await
    .unwrap();

    // Now create modified source and re-export
    let src_root_id2 = setup_root(&db, "merkle_nomatch_src2").await;
    db.create_block(
        Some(src_root_id2),
        "child",
        "Modified",
        "content",
        &serde_json::json!({}),
    )
    .await
    .unwrap();

    let tree2 = export_tree(&db, src_root_id2, &default_export_options())
        .await
        .unwrap();

    // Import with Merkle strategy should NOT match (content changed)
    let options = ImportOptions {
        mode: ImportMode::Merge,
        match_strategy: MatchStrategy::Merkle,
        global_link: false,
        replace_existing: false,
    };
    let result = import_tree(&db, &tree2, Some(dst_root_id), options)
        .await
        .unwrap();
    assert_eq!(
        result.created, 2,
        "Modified content should not match via merkle"
    );
}

#[tokio::test]
async fn test_import_topology_match_skips_all() {
    let db = setup_store().await;
    let src_root_id = setup_root(&db, "topo_match_src").await;
    let dst_root_id = setup_root(&db, "topo_match_dst").await;

    let (node_a, _) = db
        .create_block(
            Some(src_root_id),
            "a",
            "A",
            "content",
            &serde_json::json!({}),
        )
        .await
        .unwrap();
    db.create_block_with_content(
        Some(src_root_id),
        "b",
        "Links to {0}",
        &[node_a.lineage_id],
        "content",
        &serde_json::json!({}),
    )
    .await
    .unwrap();

    let tree = export_tree(&db, src_root_id, &default_export_options())
        .await
        .unwrap();

    // Seed via copy
    import_tree(
        &db,
        &tree,
        Some(dst_root_id),
        ImportOptions::from_mode(ImportMode::Copy),
    )
    .await
    .unwrap();

    // Now import with topology strategy — should skip entire tree
    let options = ImportOptions {
        mode: ImportMode::Merge,
        match_strategy: MatchStrategy::Topology,
        global_link: false,
        replace_existing: false,
    };
    let result = import_tree(&db, &tree, Some(dst_root_id), options)
        .await
        .unwrap();
    assert_eq!(
        result.skipped,
        tree.nodes.len(),
        "Topology match should skip entire import"
    );
    assert_eq!(result.created, 0);
}

#[tokio::test]
async fn test_import_topology_no_match_falls_back() {
    let db = setup_store().await;
    let src_root_id = setup_root(&db, "topo_fallback_src").await;
    let dst_root_id = setup_root(&db, "topo_fallback_dst").await;

    let (node_a, _) = db
        .create_block(
            Some(src_root_id),
            "a",
            "A",
            "content",
            &serde_json::json!({}),
        )
        .await
        .unwrap();
    db.create_block_with_content(
        Some(src_root_id),
        "b",
        "Links to {0}",
        &[node_a.lineage_id],
        "content",
        &serde_json::json!({}),
    )
    .await
    .unwrap();

    let tree = export_tree(&db, src_root_id, &default_export_options())
        .await
        .unwrap();

    // Seed a different tree under dst (same structure but different content)
    db.create_block(
        Some(dst_root_id),
        "different",
        "Different",
        "content",
        &serde_json::json!({}),
    )
    .await
    .unwrap();

    // Topology won't match the existing children, falls back to per-node Merkle
    let options = ImportOptions {
        mode: ImportMode::Merge,
        match_strategy: MatchStrategy::Topology,
        global_link: false,
        replace_existing: false,
    };
    let result = import_tree(&db, &tree, Some(dst_root_id), options)
        .await
        .unwrap();
    assert!(
        result.created > 0,
        "No topology match should fall back and create nodes"
    );
}

/// Regression: importing the same export file multiple times should dedup correctly
/// even when the parent already has other namespace children with the same empty content.
#[tokio::test]
async fn test_import_merge_idempotent_with_siblings() {
    let db = setup_store().await;
    let src_root_id = setup_root(&db, "sibling_src").await;
    let dst_root_id = setup_root(&db, "sibling_dst").await;

    // Create a tree with children
    db.create_block(
        Some(src_root_id),
        "child-a",
        "Content A",
        "content",
        &serde_json::json!({}),
    )
    .await
    .unwrap();
    db.create_block(
        Some(src_root_id),
        "child-b",
        "Content B",
        "content",
        &serde_json::json!({}),
    )
    .await
    .unwrap();

    // Create a pre-existing namespace sibling under dst that has the same content_type
    // and empty content as src_root (common case: both are namespace blocks).
    db.create_block(
        Some(dst_root_id),
        "other-ns",
        "",
        "namespace",
        &serde_json::json!({}),
    )
    .await
    .unwrap();

    let tree = export_tree(&db, src_root_id, &default_export_options())
        .await
        .unwrap();

    // First import — should create the full tree
    let r1 = import_tree(
        &db,
        &tree,
        Some(dst_root_id),
        ImportOptions::from_mode(ImportMode::Merge),
    )
    .await
    .unwrap();
    assert_eq!(r1.created, 3, "first import creates root + 2 children");

    // Second import — should skip everything (not create duplicates)
    let r2 = import_tree(
        &db,
        &tree,
        Some(dst_root_id),
        ImportOptions::from_mode(ImportMode::Merge),
    )
    .await
    .unwrap();
    assert_eq!(r2.skipped, 3, "second import should skip all 3 nodes");
    assert_eq!(r2.created, 0, "second import should create nothing");

    // Third import — still idempotent
    let r3 = import_tree(
        &db,
        &tree,
        Some(dst_root_id),
        ImportOptions::from_mode(ImportMode::Merge),
    )
    .await
    .unwrap();
    assert_eq!(r3.skipped, 3, "third import should skip all 3 nodes");
    assert_eq!(r3.created, 0, "third import should create nothing");
}

/// Regression: global_link must check name, not just content hash.
///
/// Without the name check, all empty namespace blocks have identical content
/// hashes, so global_link would match any namespace lineage (e.g. "settings")
/// and create blocks with the wrong name, causing duplicate conflicts.
#[tokio::test]
async fn test_import_global_link_namespace_no_false_match() {
    use yap_core::export::{ExportNode, ExportTree};

    let db = setup_store().await;

    // Bootstrap creates `types` and `settings` — both are namespace-like blocks
    // with empty content, so their content hashes match any empty namespace.
    yap_core::bootstrap::bootstrap(&db, &[]).await.unwrap();

    // Build an export tree with namespace nodes that have DIFFERENT names from
    // anything in the DB. All have empty content (same hash as types/settings).
    let tree = ExportTree {
        format: "yap-tree-v2".to_string(),
        exported_at: chrono::Utc::now(),
        source_namespace: "research".to_string(),
        nodes: vec![
            ExportNode {
                local_id: 0,
                name: "research".to_string(),
                content_type: "namespace".to_string(),
                content_template: String::new(),
                internal_links: vec![],
                external_links: vec![],
                properties: serde_json::json!({"name": "research"}),
                export_hash: String::new(),
                parent_local_id: None,
                position: "80".to_string(),
                children_local_ids: vec![1, 2],
                content_identity_hash: String::new(),
                merkle_hash: String::new(),
            },
            ExportNode {
                local_id: 1,
                name: "ml".to_string(),
                content_type: "namespace".to_string(),
                content_template: String::new(),
                internal_links: vec![],
                external_links: vec![],
                properties: serde_json::json!({"name": "ml"}),
                export_hash: String::new(),
                parent_local_id: Some(0),
                position: "80".to_string(),
                children_local_ids: vec![],
                content_identity_hash: String::new(),
                merkle_hash: String::new(),
            },
            ExportNode {
                local_id: 2,
                name: "plt".to_string(),
                content_type: "namespace".to_string(),
                content_template: String::new(),
                internal_links: vec![],
                external_links: vec![],
                properties: serde_json::json!({"name": "plt"}),
                export_hash: String::new(),
                parent_local_id: Some(0),
                position: "8180".to_string(),
                children_local_ids: vec![],
                content_identity_hash: String::new(),
                merkle_hash: String::new(),
            },
        ],
        edges: vec![],
        topology_hash: String::new(),
    };

    // Import with global_link — should NOT confuse "research" with "types"
    // or "settings" despite identical empty-content hashes.
    let opts = ImportOptions {
        mode: ImportMode::Merge,
        match_strategy: MatchStrategy::Auto,
        global_link: true,
        replace_existing: false,
    };
    let result = import_tree(&db, &tree, None, opts).await.unwrap();

    // All 3 nodes should be freshly created (no name matches in existing DB).
    assert_eq!(result.created, 3, "should create research + ml + plt");
    assert_eq!(result.linked, 0, "should not link to settings/types");

    // Verify the root block has the correct name.
    let root_id = result.root_block_id.unwrap();
    let root_block = db.get_block(root_id).await.unwrap();
    assert_eq!(root_block.name, "research");
}
