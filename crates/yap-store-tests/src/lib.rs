//! Shared backend-agnostic integration tests for Store implementations.
//!
//! This crate is test-only. It defines test functions that accept `&dyn Store`,
//! then runs each test against both SqliteStore and PgStore via a macro.

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;
    use yap_core::Store;
    use yap_core::models::{CreateAtom, CreateEdge};

    // =========================================================================
    // Store factories
    // =========================================================================

    async fn create_sqlite_store() -> Box<dyn Store> {
        use yap_store_sqlite::{SqliteStore, run_migrations};

        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("connect to in-memory SQLite");

        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&pool)
            .await
            .expect("enable foreign keys");

        run_migrations(&pool).await.expect("run migrations");
        Box::new(SqliteStore::new(pool))
    }

    async fn create_pg_store() -> Box<dyn Store> {
        use yap_store_pg::{PgStore, run_migrations};

        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for PG tests");

        let db = PgStore::connect(&url).await.expect("connect to PG");
        run_migrations(db.pool()).await.expect("run PG migrations");

        // Clean all tables for a fresh test
        sqlx::query("DELETE FROM edges")
            .execute(db.pool())
            .await
            .ok();
        sqlx::query("DELETE FROM blocks")
            .execute(db.pool())
            .await
            .ok();
        sqlx::query("DELETE FROM lineages")
            .execute(db.pool())
            .await
            .ok();
        sqlx::query("DELETE FROM atoms")
            .execute(db.pool())
            .await
            .ok();

        Box::new(db)
    }

    // =========================================================================
    // Test macro — generates _sqlite and _pg variants
    // =========================================================================

    macro_rules! store_tests {
        ($($name:ident),* $(,)?) => {
            $(
                paste::paste! {
                    #[tokio::test]
                    async fn [<$name _sqlite>]() {
                        let store = create_sqlite_store().await;
                        $name(&*store).await;
                    }

                    #[tokio::test]
                    #[ignore] // Requires DATABASE_URL
                    async fn [<$name _pg>]() {
                        let store = create_pg_store().await;
                        $name(&*store).await;
                    }
                }
            )*
        };
    }

    // =========================================================================
    // Register all tests
    // =========================================================================

    store_tests! {
        test_health_check,
        test_atom_create_and_get,
        test_atom_links_roundtrip,
        test_content_hash_parity,
        test_lineage_versioning,
        test_block_crud,
        test_block_delete_restore,
        test_block_recursive_delete,
        test_namespace_computation,
        test_search_blocks,
        test_backlinks,
        test_backlinks_return_lineage_id,
        test_count_backlinks,
        test_edges_crud,
        test_edge_duplicate_conflict,
        test_block_duplicate_conflict,
        test_move_block,
        test_is_move_safe,
        test_list_blocks_by_content_type,
        test_orphaned_blocks,
        test_fractional_index_ordering,
        test_root_blocks,
        test_blocks_for_lineage,
        test_create_block_consistency,
        test_edit_lineage_consistency,
        test_restore_block_recursive,
        test_restore_recursive_count,
        test_create_block_for_lineage_duplicate_conflict,
        // Store default method tests
        test_find_block_by_namespace_simple,
        test_find_block_by_namespace_nested,
        test_find_block_by_namespace_not_found,
        test_find_block_by_namespace_empty,
        test_find_block_by_namespace_partial,
        test_ensure_namespace_creates_hierarchy,
        test_ensure_namespace_idempotent,
        test_ensure_namespace_single_segment,
        test_ensure_namespace_empty_string,
        test_ensure_namespace_block_returns_leaf,
        test_resolve_namespace_to_lineage,
        test_resolve_namespace_to_lineage_missing,
        test_resolve_link_to_lineage_absolute,
        test_resolve_link_to_lineage_relative,
        test_resolve_link_to_lineage_parent,
        test_resolve_link_to_lineage_not_found,
        test_get_canonical_path,
        test_get_canonical_path_no_blocks,
        test_get_link_display_info,
        test_resolve_schema_walk_up,
        test_resolve_schema_not_found,
        // P2.7: Fractional index ordering
        test_fractional_index_50_children,
        test_fractional_index_after_move,
        test_fractional_index_explicit_position,
        // P2.9: Soft-delete cascading
        test_delete_block_lineage_survives,
        test_delete_block_edges_survive,
        test_delete_block_children_become_orphans,
        test_delete_recursive_edges_survive,
        test_orphan_not_listed_if_self_deleted,
        test_restore_block_no_cascade,
        test_recursive_delete_partial_restore,
        test_move_block_to_deleted_parent,
        // P2.10: Link resolution against DB
        test_resolve_link_deleted_leaf,
        test_resolve_link_deleted_intermediate,
        test_resolve_link_after_block_move,
        test_resolve_link_after_restore,
        test_resolve_link_same_name_different_parents,
        test_resolve_schema_deleted_falls_through,
        test_find_block_by_namespace_deleted_root,
    }

    // =========================================================================
    // Test implementations
    // =========================================================================

    async fn test_health_check(store: &dyn Store) {
        assert!(store.health_check().await.unwrap());
    }

    async fn test_atom_create_and_get(store: &dyn Store) {
        let create = CreateAtom {
            content_type: "content".to_string(),
            content_template: "Hello world".to_string(),
            links: vec![],
            properties: json!({"key": "value"}),
        };

        let (atom, lineage) = store.create_atom(&create).await.unwrap();
        assert_eq!(atom.content_type, "content");
        assert_eq!(atom.content_template, "Hello world");
        assert_eq!(atom.properties, json!({"key": "value"}));
        assert_eq!(lineage.version, 1);
        assert_eq!(lineage.id, atom.id);

        let fetched = store.get_atom(lineage.id).await.unwrap();
        assert_eq!(fetched.id, atom.id);
        assert_eq!(fetched.content_template, "Hello world");
    }

    async fn test_atom_links_roundtrip(store: &dyn Store) {
        let link1 = Uuid::now_v7();
        let link2 = Uuid::now_v7();

        let create = CreateAtom {
            content_type: "content".to_string(),
            content_template: "See {0} and {1}".to_string(),
            links: vec![link1, link2],
            properties: json!({}),
        };

        let (atom, lineage) = store.create_atom(&create).await.unwrap();
        assert_eq!(atom.links, vec![link1, link2]);

        let fetched = store.get_atom(lineage.id).await.unwrap();
        assert_eq!(
            fetched.links,
            vec![link1, link2],
            "links order must be preserved"
        );
    }

    async fn test_content_hash_parity(store: &dyn Store) {
        let create = CreateAtom {
            content_type: "content".to_string(),
            content_template: "hash test content".to_string(),
            links: vec![],
            properties: json!({}),
        };

        let (atom, _) = store.create_atom(&create).await.unwrap();

        // Compute expected hash using the same function
        let expected = yap_core::hash::compute_content_hash("content", "hash test content", &[]);
        assert_eq!(atom.content_hash, expected);
        assert!(!atom.content_hash.is_empty());
    }

    async fn test_lineage_versioning(store: &dyn Store) {
        let create = CreateAtom {
            content_type: "content".to_string(),
            content_template: "v1".to_string(),
            links: vec![],
            properties: json!({}),
        };

        let (atom1, lineage1) = store.create_atom(&create).await.unwrap();
        assert_eq!(lineage1.version, 1);

        let (atom2, lineage2) = store
            .edit_lineage(lineage1.id, "content", "v2", &[], &json!({}))
            .await
            .unwrap();

        assert_eq!(lineage2.version, 2);
        assert_eq!(lineage2.id, lineage1.id);
        assert_eq!(atom2.predecessor_id, Some(atom1.id));
        assert_eq!(atom2.content_template, "v2");

        // Current atom should be v2
        let current = store.get_atom(lineage1.id).await.unwrap();
        assert_eq!(current.id, atom2.id);
    }

    async fn test_block_crud(store: &dyn Store) {
        let props = json!({});

        let (root, root_atom) = store
            .create_block_with_content(None, "root", "content", &[], "content", &props)
            .await
            .unwrap();

        assert_eq!(root.name, "root");
        assert!(root.parent_id.is_none());
        assert_eq!(root_atom.content_template, "content");

        let fetched = store.get_block(root.id).await.unwrap();
        assert_eq!(fetched.id, root.id);

        let (child, _) = store
            .create_block_with_content(Some(root.id), "child", "", &[], "", &props)
            .await
            .unwrap();

        let children = store.get_block_children(root.id).await.unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].id, child.id);
    }

    async fn test_block_delete_restore(store: &dyn Store) {
        let props = json!({});
        let (block, _) = store
            .create_block_with_content(None, "deltest", "", &[], "", &props)
            .await
            .unwrap();

        let deleted = store.delete_block(block.id).await.unwrap();
        assert!(deleted.deleted_at.is_some());

        // Normal get should fail
        assert!(store.get_block(block.id).await.is_err());

        // get_with_deleted should work
        let found = store.get_block_with_deleted(block.id).await.unwrap();
        assert!(found.deleted_at.is_some());

        let restored = store.restore_block(block.id).await.unwrap();
        assert!(restored.deleted_at.is_none());

        // Normal get should work again
        store.get_block(block.id).await.unwrap();
    }

    async fn test_block_recursive_delete(store: &dyn Store) {
        let props = json!({});
        let (root, _) = store
            .create_block_with_content(None, "rec_root", "", &[], "", &props)
            .await
            .unwrap();
        let (child, _) = store
            .create_block_with_content(Some(root.id), "rec_child", "", &[], "", &props)
            .await
            .unwrap();
        let (_gc, _) = store
            .create_block_with_content(Some(child.id), "rec_gc", "", &[], "", &props)
            .await
            .unwrap();

        let count = store.delete_block_recursive(root.id).await.unwrap();
        assert_eq!(count, 3);

        // All should be deleted
        assert!(store.get_block(root.id).await.is_err());
        assert!(store.get_block(child.id).await.is_err());
    }

    async fn test_namespace_computation(store: &dyn Store) {
        let props = json!({});
        let (root, _) = store
            .create_block_with_content(None, "research", "", &[], "", &props)
            .await
            .unwrap();
        let (child, _) = store
            .create_block_with_content(Some(root.id), "ml", "", &[], "", &props)
            .await
            .unwrap();
        let (gc, _) = store
            .create_block_with_content(Some(child.id), "attention", "", &[], "", &props)
            .await
            .unwrap();

        assert_eq!(store.compute_namespace(root.id).await.unwrap(), "research");
        assert_eq!(
            store.compute_namespace(child.id).await.unwrap(),
            "research::ml"
        );
        assert_eq!(
            store.compute_namespace(gc.id).await.unwrap(),
            "research::ml::attention"
        );
    }

    async fn test_search_blocks(store: &dyn Store) {
        let props = json!({});
        store
            .create_block_with_content(None, "searchable", "", &[], "", &props)
            .await
            .unwrap();

        let results = store.search_blocks("searchable").await.unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().any(|b| b.name == "searchable"));
    }

    async fn test_backlinks(store: &dyn Store) {
        let props = json!({});

        // Create a target
        let (_, target_lin) = store
            .create_atom(&CreateAtom {
                content_type: "".to_string(),
                content_template: "target".to_string(),
                links: vec![],
                properties: props.clone(),
            })
            .await
            .unwrap();

        // Create a block that links to the target
        let (linker_block, _) = store
            .create_block_with_content(
                None,
                "bl_linker",
                "ref {0}",
                &[target_lin.id],
                "content",
                &props,
            )
            .await
            .unwrap();

        let backlinks = store.get_backlinks(target_lin.id).await.unwrap();
        assert_eq!(backlinks.len(), 1);
        assert!(backlinks[0].atom.links.contains(&target_lin.id));
        // lineage_id should be the linker's lineage, not its atom id
        assert_eq!(backlinks[0].lineage_id, linker_block.lineage_id);
    }

    async fn test_count_backlinks(store: &dyn Store) {
        let props = json!({});

        let (_, target_lin) = store
            .create_atom(&CreateAtom {
                content_type: "".to_string(),
                content_template: "".to_string(),
                links: vec![],
                properties: props.clone(),
            })
            .await
            .unwrap();

        store
            .create_block_with_content(None, "cb_linker", "{0}", &[target_lin.id], "", &props)
            .await
            .unwrap();

        let count = store.count_backlinks(target_lin.id).await.unwrap();
        assert_eq!(count, 1);
    }

    async fn test_edges_crud(store: &dyn Store) {
        let props = json!({});

        let (_, lin1) = store
            .create_atom(&CreateAtom {
                content_type: "".to_string(),
                content_template: "".to_string(),
                links: vec![],
                properties: props.clone(),
            })
            .await
            .unwrap();

        let (_, lin2) = store
            .create_atom(&CreateAtom {
                content_type: "".to_string(),
                content_template: "".to_string(),
                links: vec![],
                properties: props.clone(),
            })
            .await
            .unwrap();

        let edge = store
            .create_edge(&CreateEdge {
                from_lineage_id: lin1.id,
                to_lineage_id: lin2.id,
                edge_type: "related".to_string(),
                properties: json!({"weight": 1}),
            })
            .await
            .unwrap();

        assert_eq!(edge.edge_type, "related");
        assert_eq!(edge.properties, json!({"weight": 1}));

        let fetched = store.get_edge(edge.id).await.unwrap();
        assert_eq!(fetched.id, edge.id);

        let from = store.get_edges_from(lin1.id).await.unwrap();
        assert_eq!(from.len(), 1);

        let to = store.get_edges_to(lin2.id).await.unwrap();
        assert_eq!(to.len(), 1);

        let all = store.get_all_edges(lin1.id).await.unwrap();
        assert_eq!(all.len(), 1);

        let deleted = store.delete_edge(edge.id).await.unwrap();
        assert!(deleted.deleted_at.is_some());

        assert!(store.get_edge(edge.id).await.is_err());
    }

    async fn test_edge_duplicate_conflict(store: &dyn Store) {
        let props = json!({});

        let (_, lin1) = store
            .create_atom(&CreateAtom {
                content_type: "".to_string(),
                content_template: "".to_string(),
                links: vec![],
                properties: props.clone(),
            })
            .await
            .unwrap();

        let (_, lin2) = store
            .create_atom(&CreateAtom {
                content_type: "".to_string(),
                content_template: "".to_string(),
                links: vec![],
                properties: props.clone(),
            })
            .await
            .unwrap();

        store
            .create_edge(&CreateEdge {
                from_lineage_id: lin1.id,
                to_lineage_id: lin2.id,
                edge_type: "dup_test".to_string(),
                properties: props.clone(),
            })
            .await
            .unwrap();

        let result = store
            .create_edge(&CreateEdge {
                from_lineage_id: lin1.id,
                to_lineage_id: lin2.id,
                edge_type: "dup_test".to_string(),
                properties: props,
            })
            .await;

        assert!(matches!(result, Err(yap_core::Error::Conflict(_))));
    }

    async fn test_block_duplicate_conflict(store: &dyn Store) {
        let props = json!({});
        store
            .create_block_with_content(None, "dup_block", "", &[], "", &props)
            .await
            .unwrap();

        let result = store
            .create_block_with_content(None, "dup_block", "", &[], "", &props)
            .await;

        assert!(matches!(result, Err(yap_core::Error::Conflict(_))));
    }

    async fn test_move_block(store: &dyn Store) {
        let props = json!({});
        let (parent1, _) = store
            .create_block_with_content(None, "mv_p1", "", &[], "", &props)
            .await
            .unwrap();
        let (parent2, _) = store
            .create_block_with_content(None, "mv_p2", "", &[], "", &props)
            .await
            .unwrap();
        let (child, _) = store
            .create_block_with_content(Some(parent1.id), "mv_child", "", &[], "", &props)
            .await
            .unwrap();

        let moved = store
            .move_block(child.id, Some(parent2.id), None)
            .await
            .unwrap();
        assert_eq!(moved.parent_id, Some(parent2.id));

        let p1_children = store.get_block_children(parent1.id).await.unwrap();
        assert!(p1_children.is_empty());

        let p2_children = store.get_block_children(parent2.id).await.unwrap();
        assert_eq!(p2_children.len(), 1);
    }

    async fn test_is_move_safe(store: &dyn Store) {
        let props = json!({});
        let (parent, _) = store
            .create_block_with_content(None, "safe_parent", "", &[], "", &props)
            .await
            .unwrap();
        let (child, _) = store
            .create_block_with_content(Some(parent.id), "safe_child", "", &[], "", &props)
            .await
            .unwrap();

        // Moving parent under child = cycle
        assert!(!store.is_move_safe(parent.id, Some(child.id)).await.unwrap());

        // Moving to root = safe
        assert!(store.is_move_safe(child.id, None).await.unwrap());

        // Moving to self = unsafe
        assert!(
            !store
                .is_move_safe(parent.id, Some(parent.id))
                .await
                .unwrap()
        );
    }

    async fn test_list_blocks_by_content_type(store: &dyn Store) {
        let props = json!({});
        store
            .create_block_with_content(None, "ct_test", "", &[], "custom_type", &props)
            .await
            .unwrap();

        let results = store
            .list_blocks_by_content_type("custom_type")
            .await
            .unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().any(|b| b.name == "ct_test"));
    }

    async fn test_orphaned_blocks(store: &dyn Store) {
        let props = json!({});
        let (parent, _) = store
            .create_block_with_content(None, "orph_parent", "", &[], "", &props)
            .await
            .unwrap();
        let (_child, _) = store
            .create_block_with_content(Some(parent.id), "orph_child", "", &[], "", &props)
            .await
            .unwrap();

        // Delete parent only (not recursive)
        store.delete_block(parent.id).await.unwrap();

        let orphans = store.list_orphaned_blocks().await.unwrap();
        assert!(orphans.iter().any(|b| b.name == "orph_child"));
    }

    async fn test_fractional_index_ordering(store: &dyn Store) {
        let props = json!({});
        let (root, _) = store
            .create_block_with_content(None, "fi_root", "", &[], "", &props)
            .await
            .unwrap();

        for i in 0..5 {
            store
                .create_block_with_content(
                    Some(root.id),
                    &format!("fi_child_{i}"),
                    "",
                    &[],
                    "",
                    &props,
                )
                .await
                .unwrap();
        }

        let children = store.get_block_children(root.id).await.unwrap();
        assert_eq!(children.len(), 5);

        // Positions should be in ascending order
        for w in children.windows(2) {
            assert!(
                w[0].position < w[1].position,
                "positions not ordered: {} >= {}",
                w[0].position,
                w[1].position,
            );
        }
    }

    async fn test_root_blocks(store: &dyn Store) {
        let props = json!({});
        let (root, _) = store
            .create_block_with_content(None, "rb_test", "", &[], "", &props)
            .await
            .unwrap();

        let roots = store.get_root_blocks().await.unwrap();
        assert!(roots.iter().any(|b| b.id == root.id));
    }

    async fn test_blocks_for_lineage(store: &dyn Store) {
        let props = json!({});
        let (block, _) = store
            .create_block_with_content(None, "bfl_test", "", &[], "", &props)
            .await
            .unwrap();

        let blocks = store
            .get_blocks_for_lineage(block.lineage_id)
            .await
            .unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].id, block.id);
    }

    // Step 2: Verify backlinks return lineage_id (not atom id) after edit
    async fn test_backlinks_return_lineage_id(store: &dyn Store) {
        let props = json!({});

        // Create target block
        let (target_block, _) = store
            .create_block_with_content(None, "bl_target2", "target", &[], "", &props)
            .await
            .unwrap();

        // Create linker block pointing to target
        let (linker_block, _) = store
            .create_block_with_content(
                None,
                "bl_linker2",
                "see {0}",
                &[target_block.lineage_id],
                "content",
                &props,
            )
            .await
            .unwrap();

        // Edit the linker (so atom.id != lineage.id)
        let (new_atom, new_lineage) = store
            .edit_lineage(
                linker_block.lineage_id,
                "content",
                "see {0} updated",
                &[target_block.lineage_id],
                &props,
            )
            .await
            .unwrap();
        assert_ne!(
            new_atom.id, linker_block.lineage_id,
            "After edit, atom.id should differ from lineage.id"
        );
        assert_eq!(new_lineage.id, linker_block.lineage_id);

        // Verify backlinks return the stable lineage ID
        let backlinks = store.get_backlinks(target_block.lineage_id).await.unwrap();
        assert_eq!(backlinks.len(), 1);
        assert_eq!(
            backlinks[0].lineage_id, linker_block.lineage_id,
            "Backlink should return lineage_id, not atom.id"
        );
        assert_eq!(
            backlinks[0].atom.id, new_atom.id,
            "Backlink atom should be the current atom"
        );
    }

    // Step 3: Verify create_block_with_content creates consistent records
    async fn test_create_block_consistency(store: &dyn Store) {
        let props = json!({});
        let (block, atom) = store
            .create_block_with_content(None, "consist_test", "hello", &[], "content", &props)
            .await
            .unwrap();

        // atom.id == lineage.id == block.lineage_id at creation
        assert_eq!(block.lineage_id, atom.id);

        let lineage = store.get_lineage(block.lineage_id).await.unwrap();
        assert_eq!(lineage.current_id, atom.id);
        assert_eq!(lineage.version, 1);

        let fetched_atom = store.get_atom(block.lineage_id).await.unwrap();
        assert_eq!(fetched_atom.id, atom.id);
        assert_eq!(fetched_atom.content_template, "hello");
    }

    // Step 3: Verify edit_lineage consistency
    async fn test_edit_lineage_consistency(store: &dyn Store) {
        let props = json!({});
        let (block, atom_v1) = store
            .create_block_with_content(None, "edit_consist", "v1", &[], "content", &props)
            .await
            .unwrap();

        let (atom_v2, lineage_v2) = store
            .edit_lineage(block.lineage_id, "content", "v2", &[], &props)
            .await
            .unwrap();

        // Lineage ID should be stable
        assert_eq!(lineage_v2.id, block.lineage_id);
        // Current should point to new atom
        assert_eq!(lineage_v2.current_id, atom_v2.id);
        // Version incremented
        assert_eq!(lineage_v2.version, 2);
        // Old atom should be the predecessor
        assert_eq!(atom_v2.predecessor_id, Some(atom_v1.id));
        // New atom should have different ID
        assert_ne!(atom_v2.id, atom_v1.id);
    }

    // Step 4: Verify restore_block_recursive
    async fn test_restore_block_recursive(store: &dyn Store) {
        let props = json!({});
        let (root, _) = store
            .create_block_with_content(None, "rest_root", "", &[], "", &props)
            .await
            .unwrap();
        let (child, _) = store
            .create_block_with_content(Some(root.id), "rest_child", "", &[], "", &props)
            .await
            .unwrap();
        let (gc, _) = store
            .create_block_with_content(Some(child.id), "rest_gc", "", &[], "", &props)
            .await
            .unwrap();

        // Delete recursively
        let deleted = store.delete_block_recursive(root.id).await.unwrap();
        assert_eq!(deleted, 3);

        // All should be deleted
        assert!(store.get_block(root.id).await.is_err());
        assert!(store.get_block(child.id).await.is_err());
        assert!(store.get_block(gc.id).await.is_err());

        // Restore recursively
        let restored = store.restore_block_recursive(root.id).await.unwrap();
        assert_eq!(restored, 3);

        // All should be accessible again
        store.get_block(root.id).await.unwrap();
        store.get_block(child.id).await.unwrap();
        store.get_block(gc.id).await.unwrap();
    }

    // Step 4: Verify restore_recursive count
    async fn test_restore_recursive_count(store: &dyn Store) {
        let props = json!({});
        let (block, _) = store
            .create_block_with_content(None, "rest_count", "", &[], "", &props)
            .await
            .unwrap();

        // Calling restore on a non-deleted block should return 0
        let count = store.restore_block_recursive(block.id).await.unwrap();
        assert_eq!(count, 0);
    }

    // Step 8: Verify create_block_for_lineage rejects duplicate
    async fn test_create_block_for_lineage_duplicate_conflict(store: &dyn Store) {
        let props = json!({});
        let (parent, _) = store
            .create_block_with_content(None, "cbl_parent", "", &[], "", &props)
            .await
            .unwrap();
        let (block, _) = store
            .create_block_with_content(Some(parent.id), "cbl_child", "", &[], "", &props)
            .await
            .unwrap();

        // Try to create another block for the same lineage under the same parent
        let result = store
            .create_block_for_lineage(Some(parent.id), block.lineage_id)
            .await;
        assert!(
            matches!(result, Err(yap_core::Error::Conflict(_))),
            "Expected Conflict error for duplicate block, got {:?}",
            result
        );
    }

    // =========================================================================
    // Store default method tests
    // =========================================================================

    async fn test_find_block_by_namespace_simple(store: &dyn Store) {
        let props = json!({});
        store
            .create_block_with_content(None, "fbn_root", "", &[], "", &props)
            .await
            .unwrap();

        let found = store.find_block_by_namespace("fbn_root").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "fbn_root");
    }

    async fn test_find_block_by_namespace_nested(store: &dyn Store) {
        let props = json!({});
        let (a, _) = store
            .create_block_with_content(None, "fbn_a", "", &[], "", &props)
            .await
            .unwrap();
        let (b, _) = store
            .create_block_with_content(Some(a.id), "fbn_b", "", &[], "", &props)
            .await
            .unwrap();
        store
            .create_block_with_content(Some(b.id), "fbn_c", "", &[], "", &props)
            .await
            .unwrap();

        let found = store
            .find_block_by_namespace("fbn_a::fbn_b::fbn_c")
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "fbn_c");
    }

    async fn test_find_block_by_namespace_not_found(store: &dyn Store) {
        let found = store
            .find_block_by_namespace("nonexistent_xyz_987")
            .await
            .unwrap();
        assert!(found.is_none());
    }

    async fn test_find_block_by_namespace_empty(store: &dyn Store) {
        let found = store.find_block_by_namespace("").await.unwrap();
        assert!(found.is_none());
    }

    async fn test_find_block_by_namespace_partial(store: &dyn Store) {
        let props = json!({});
        let (a, _) = store
            .create_block_with_content(None, "fbn_partial_a", "", &[], "", &props)
            .await
            .unwrap();
        store
            .create_block_with_content(Some(a.id), "fbn_partial_b", "", &[], "", &props)
            .await
            .unwrap();

        // a::b exists
        let found = store
            .find_block_by_namespace("fbn_partial_a::fbn_partial_b")
            .await
            .unwrap();
        assert!(found.is_some());

        // a::b::c does NOT exist
        let not_found = store
            .find_block_by_namespace("fbn_partial_a::fbn_partial_b::fbn_partial_c")
            .await
            .unwrap();
        assert!(not_found.is_none());
    }

    async fn test_ensure_namespace_creates_hierarchy(store: &dyn Store) {
        let results = store.ensure_namespace("ens_a::ens_b::ens_c").await.unwrap();
        assert_eq!(results.len(), 3);
        // All should be newly created
        assert!(results[0].1, "ens_a should be created");
        assert!(results[1].1, "ens_b should be created");
        assert!(results[2].1, "ens_c should be created");

        // Verify the hierarchy exists
        let found = store
            .find_block_by_namespace("ens_a::ens_b::ens_c")
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, results[2].0);
    }

    async fn test_ensure_namespace_idempotent(store: &dyn Store) {
        let r1 = store
            .ensure_namespace("ens_idem_a::ens_idem_b")
            .await
            .unwrap();
        assert_eq!(r1.len(), 2);
        assert!(r1[0].1, "first call should create");
        assert!(r1[1].1, "first call should create");

        let r2 = store
            .ensure_namespace("ens_idem_a::ens_idem_b")
            .await
            .unwrap();
        assert_eq!(r2.len(), 2);
        assert!(!r2[0].1, "second call should not create");
        assert!(!r2[1].1, "second call should not create");

        // Same IDs
        assert_eq!(r1[0].0, r2[0].0);
        assert_eq!(r1[1].0, r2[1].0);
    }

    async fn test_ensure_namespace_single_segment(store: &dyn Store) {
        let results = store.ensure_namespace("ens_single").await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].1);

        let found = store.find_block_by_namespace("ens_single").await.unwrap();
        assert!(found.is_some());
    }

    async fn test_ensure_namespace_empty_string(store: &dyn Store) {
        let results = store.ensure_namespace("").await.unwrap();
        assert!(results.is_empty());
    }

    async fn test_ensure_namespace_block_returns_leaf(store: &dyn Store) {
        let leaf_id = store
            .ensure_namespace_block("enb_a::enb_b::enb_c")
            .await
            .unwrap();

        // The leaf should be the deepest block
        let found = store
            .find_block_by_namespace("enb_a::enb_b::enb_c")
            .await
            .unwrap();
        assert_eq!(found.unwrap().id, leaf_id);
    }

    async fn test_resolve_namespace_to_lineage(store: &dyn Store) {
        let props = json!({});
        let (block, _) = store
            .create_block_with_content(None, "rntl_root", "", &[], "", &props)
            .await
            .unwrap();

        let lineage = store
            .resolve_namespace_to_lineage("rntl_root")
            .await
            .unwrap();
        assert_eq!(lineage, Some(block.lineage_id));
    }

    async fn test_resolve_namespace_to_lineage_missing(store: &dyn Store) {
        let lineage = store
            .resolve_namespace_to_lineage("no_such_ns_xyz")
            .await
            .unwrap();
        assert!(lineage.is_none());
    }

    async fn test_resolve_link_to_lineage_absolute(store: &dyn Store) {
        let props = json!({});
        let (a, _) = store
            .create_block_with_content(None, "rll_abs_a", "", &[], "", &props)
            .await
            .unwrap();
        let (b, _) = store
            .create_block_with_content(Some(a.id), "rll_abs_b", "", &[], "", &props)
            .await
            .unwrap();

        let segments = vec!["rll_abs_a".to_string(), "rll_abs_b".to_string()];
        let result = store
            .resolve_link_to_lineage(&segments, false, 0, None)
            .await
            .unwrap();
        assert_eq!(result, Some(b.lineage_id));
    }

    async fn test_resolve_link_to_lineage_relative(store: &dyn Store) {
        let props = json!({});
        let (parent, _) = store
            .create_block_with_content(None, "rll_rel_parent", "", &[], "", &props)
            .await
            .unwrap();
        let (child, _) = store
            .create_block_with_content(Some(parent.id), "rll_rel_child", "", &[], "", &props)
            .await
            .unwrap();

        // Relative ./child from context rll_rel_parent
        let segments = vec!["rll_rel_child".to_string()];
        let result = store
            .resolve_link_to_lineage(&segments, true, 0, Some("rll_rel_parent"))
            .await
            .unwrap();
        assert_eq!(result, Some(child.lineage_id));
    }

    async fn test_resolve_link_to_lineage_parent(store: &dyn Store) {
        let props = json!({});
        let (root, _) = store
            .create_block_with_content(None, "rll_par_root", "", &[], "", &props)
            .await
            .unwrap();
        store
            .create_block_with_content(Some(root.id), "rll_par_child", "", &[], "", &props)
            .await
            .unwrap();
        let (sibling, _) = store
            .create_block_with_content(Some(root.id), "rll_par_sibling", "", &[], "", &props)
            .await
            .unwrap();

        // ../sibling from context rll_par_root::rll_par_child
        let segments = vec!["rll_par_sibling".to_string()];
        let result = store
            .resolve_link_to_lineage(&segments, true, 1, Some("rll_par_root::rll_par_child"))
            .await
            .unwrap();
        assert_eq!(result, Some(sibling.lineage_id));
    }

    async fn test_resolve_link_to_lineage_not_found(store: &dyn Store) {
        let segments = vec!["nonexistent_link_target_xyz".to_string()];
        let result = store
            .resolve_link_to_lineage(&segments, false, 0, None)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    async fn test_get_canonical_path(store: &dyn Store) {
        let props = json!({});
        let (root, _) = store
            .create_block_with_content(None, "gcp_root", "", &[], "", &props)
            .await
            .unwrap();
        let (child, _) = store
            .create_block_with_content(Some(root.id), "gcp_child", "", &[], "", &props)
            .await
            .unwrap();

        let path = store.get_canonical_path(child.lineage_id).await.unwrap();
        assert_eq!(path, Some("gcp_root::gcp_child".to_string()));
    }

    async fn test_get_canonical_path_no_blocks(store: &dyn Store) {
        // Create a standalone atom/lineage with no block referencing it
        let create = CreateAtom {
            content_type: "content".to_string(),
            content_template: "orphan".to_string(),
            links: vec![],
            properties: json!({}),
        };
        let (_, lineage) = store.create_atom(&create).await.unwrap();

        let path = store.get_canonical_path(lineage.id).await.unwrap();
        assert!(path.is_none());
    }

    async fn test_get_link_display_info(store: &dyn Store) {
        let props = json!({"custom": "value"});
        let (root, _) = store
            .create_block_with_content(None, "gldi_root", "", &[], "", &json!({}))
            .await
            .unwrap();
        let (block, _) = store
            .create_block_with_content(
                Some(root.id),
                "gldi_child",
                "content",
                &[],
                "content",
                &props,
            )
            .await
            .unwrap();

        let info = store.get_link_display_info(block.lineage_id).await.unwrap();
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.namespace, "gldi_root::gldi_child");
        assert_eq!(info.content_type, "content");
        assert_eq!(info.lineage_id, block.lineage_id);
    }

    async fn test_resolve_schema_walk_up(store: &dyn Store) {
        let props = json!({});
        // Create types::task schema
        store.ensure_namespace_block("types").await.unwrap();
        let types_block = store
            .find_block_by_namespace("types")
            .await
            .unwrap()
            .unwrap();
        store
            .create_block_with_content(Some(types_block.id), "rs_task", "", &[], "schema", &props)
            .await
            .unwrap();

        // Resolve "rs_task" from a nested context
        let result = store
            .resolve_schema("rs_task", Some("research::ml"))
            .await
            .unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "rs_task");
    }

    async fn test_resolve_schema_not_found(store: &dyn Store) {
        let result = store
            .resolve_schema("nonexistent_type_xyz", None)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    // =========================================================================
    // P2.7: Fractional index ordering
    // =========================================================================

    async fn test_fractional_index_50_children(store: &dyn Store) {
        let props = json!({});
        let (root, _) = store
            .create_block_with_content(None, "fi50_root", "", &[], "", &props)
            .await
            .unwrap();

        for i in 0..50 {
            store
                .create_block_with_content(
                    Some(root.id),
                    &format!("fi50_child_{:02}", i),
                    "",
                    &[],
                    "",
                    &props,
                )
                .await
                .unwrap();
        }

        let children = store.get_block_children(root.id).await.unwrap();
        assert_eq!(children.len(), 50);

        // Positions must be strictly ascending
        for w in children.windows(2) {
            assert!(
                w[0].position < w[1].position,
                "Position ordering violated at {} ({}) >= {} ({})",
                w[0].name,
                w[0].position,
                w[1].name,
                w[1].position,
            );
        }
    }

    async fn test_fractional_index_after_move(store: &dyn Store) {
        let props = json!({});
        let (parent, _) = store
            .create_block_with_content(None, "fimv_parent", "", &[], "", &props)
            .await
            .unwrap();

        // Create three children: a, b, c
        let (a, _) = store
            .create_block_with_content(Some(parent.id), "fimv_a", "", &[], "", &props)
            .await
            .unwrap();
        let (_b, _) = store
            .create_block_with_content(Some(parent.id), "fimv_b", "", &[], "", &props)
            .await
            .unwrap();
        let (_c, _) = store
            .create_block_with_content(Some(parent.id), "fimv_c", "", &[], "", &props)
            .await
            .unwrap();

        // Move 'a' to end (no explicit position → gets next position after last child)
        // First move out, then back in to get a fresh end position
        let (other_parent, _) = store
            .create_block_with_content(None, "fimv_other", "", &[], "", &props)
            .await
            .unwrap();
        store
            .move_block(a.id, Some(other_parent.id), None)
            .await
            .unwrap();
        store.move_block(a.id, Some(parent.id), None).await.unwrap();

        let children = store.get_block_children(parent.id).await.unwrap();
        assert_eq!(children.len(), 3);

        // 'a' should now be last
        assert_eq!(
            children[2].id, a.id,
            "After move to end, 'a' should be the last child"
        );
        // Positions must still be strictly ascending
        for w in children.windows(2) {
            assert!(
                w[0].position < w[1].position,
                "Position ordering violated: {} >= {}",
                w[0].position,
                w[1].position,
            );
        }
    }

    async fn test_fractional_index_explicit_position(store: &dyn Store) {
        let props = json!({});
        let (parent, _) = store
            .create_block_with_content(None, "fiep_parent", "", &[], "", &props)
            .await
            .unwrap();

        // Create three children
        let (a, _) = store
            .create_block_with_content(Some(parent.id), "fiep_a", "", &[], "", &props)
            .await
            .unwrap();
        let (b, _) = store
            .create_block_with_content(Some(parent.id), "fiep_b", "", &[], "", &props)
            .await
            .unwrap();
        let (c, _) = store
            .create_block_with_content(Some(parent.id), "fiep_c", "", &[], "", &props)
            .await
            .unwrap();

        let children_before = store.get_block_children(parent.id).await.unwrap();
        assert_eq!(children_before.len(), 3);
        assert_eq!(children_before[0].id, a.id);
        assert_eq!(children_before[1].id, b.id);
        assert_eq!(children_before[2].id, c.id);

        // Use update_block to give 'c' a position before 'a'
        // Pick a position string that lexicographically precedes a's position
        let before_a = {
            // Use a position that is definitely before 'a's position
            // A position of "0" or a string starting with "0" should come first
            let a_pos = &children_before[0].position;
            // Prepend a character that is lexicographically smaller
            let mut early_pos = String::new();
            // Use first char minus 1, or just "0" which sorts before typical fractional indices
            for c in a_pos.chars() {
                if c > '0' {
                    early_pos.push((c as u8 - 1) as char);
                    early_pos.push_str(&a_pos[early_pos.len()..]);
                    break;
                } else {
                    early_pos.push(c);
                }
            }
            if early_pos.is_empty() || early_pos >= *a_pos {
                // Fallback: just use a very small string
                "0".to_string()
            } else {
                early_pos
            }
        };

        store
            .update_block(
                c.id,
                &yap_core::models::UpdateBlock {
                    name: None,
                    position: Some(before_a.clone()),
                },
            )
            .await
            .unwrap();

        let children_after = store.get_block_children(parent.id).await.unwrap();
        assert_eq!(children_after.len(), 3);

        // 'c' should now be first (before 'a')
        assert_eq!(
            children_after[0].id, c.id,
            "After explicit position update, 'c' should be first"
        );
        assert_eq!(children_after[1].id, a.id);
        assert_eq!(children_after[2].id, b.id);
    }

    // =========================================================================
    // P2.9: Soft-delete cascading
    // =========================================================================

    async fn test_delete_block_lineage_survives(store: &dyn Store) {
        let props = json!({});
        let (block, _atom) = store
            .create_block_with_content(None, "dbl_surv", "lineage content", &[], "content", &props)
            .await
            .unwrap();

        let lineage_id = block.lineage_id;

        // Delete the block
        store.delete_block(block.id).await.unwrap();

        // Block should be gone from normal get
        assert!(store.get_block(block.id).await.is_err());

        // But the lineage and atom should still be accessible
        let atom = store.get_atom(lineage_id).await.unwrap();
        assert_eq!(atom.content_template, "lineage content");

        let lineage = store.get_lineage(lineage_id).await.unwrap();
        assert_eq!(lineage.id, lineage_id);
    }

    async fn test_delete_block_edges_survive(store: &dyn Store) {
        let props = json!({});

        // Create two blocks
        let (block1, _) = store
            .create_block_with_content(None, "dbe_block1", "", &[], "", &props)
            .await
            .unwrap();
        let (block2, _) = store
            .create_block_with_content(None, "dbe_block2", "", &[], "", &props)
            .await
            .unwrap();

        // Create edge between their lineages
        let edge = store
            .create_edge(&CreateEdge {
                from_lineage_id: block1.lineage_id,
                to_lineage_id: block2.lineage_id,
                edge_type: "dbe_related".to_string(),
                properties: props.clone(),
            })
            .await
            .unwrap();

        // Delete block1
        store.delete_block(block1.id).await.unwrap();

        // Edge should still be queryable
        let fetched = store.get_edge(edge.id).await.unwrap();
        assert_eq!(fetched.id, edge.id);

        let from_edges = store.get_edges_from(block1.lineage_id).await.unwrap();
        assert_eq!(from_edges.len(), 1);
        assert_eq!(from_edges[0].id, edge.id);

        let to_edges = store.get_edges_to(block2.lineage_id).await.unwrap();
        assert_eq!(to_edges.len(), 1);
        assert_eq!(to_edges[0].id, edge.id);
    }

    async fn test_delete_block_children_become_orphans(store: &dyn Store) {
        let props = json!({});
        let (parent, _) = store
            .create_block_with_content(None, "dbc_parent", "", &[], "", &props)
            .await
            .unwrap();
        let (child1, _) = store
            .create_block_with_content(Some(parent.id), "dbc_child1", "", &[], "", &props)
            .await
            .unwrap();
        let (child2, _) = store
            .create_block_with_content(Some(parent.id), "dbc_child2", "", &[], "", &props)
            .await
            .unwrap();

        // Delete parent only (not recursive)
        store.delete_block(parent.id).await.unwrap();

        let orphans = store.list_orphaned_blocks().await.unwrap();
        let orphan_ids: Vec<Uuid> = orphans.iter().map(|b| b.id).collect();
        assert!(
            orphan_ids.contains(&child1.id),
            "child1 should be an orphan after parent deletion"
        );
        assert!(
            orphan_ids.contains(&child2.id),
            "child2 should be an orphan after parent deletion"
        );
    }

    async fn test_delete_recursive_edges_survive(store: &dyn Store) {
        let props = json!({});

        // Build subtree: root -> child -> grandchild
        let (root, _) = store
            .create_block_with_content(None, "dre_root", "", &[], "", &props)
            .await
            .unwrap();
        let (child, _) = store
            .create_block_with_content(Some(root.id), "dre_child", "", &[], "", &props)
            .await
            .unwrap();
        let (gc, _) = store
            .create_block_with_content(Some(child.id), "dre_gc", "", &[], "", &props)
            .await
            .unwrap();

        // Create an external block to link to
        let (external, _) = store
            .create_block_with_content(None, "dre_external", "", &[], "", &props)
            .await
            .unwrap();

        // Create edges from child and grandchild lineages
        let edge1 = store
            .create_edge(&CreateEdge {
                from_lineage_id: child.lineage_id,
                to_lineage_id: external.lineage_id,
                edge_type: "dre_link1".to_string(),
                properties: props.clone(),
            })
            .await
            .unwrap();
        let edge2 = store
            .create_edge(&CreateEdge {
                from_lineage_id: gc.lineage_id,
                to_lineage_id: external.lineage_id,
                edge_type: "dre_link2".to_string(),
                properties: props.clone(),
            })
            .await
            .unwrap();

        // Recursively delete the subtree
        let count = store.delete_block_recursive(root.id).await.unwrap();
        assert_eq!(count, 3);

        // Edges on deleted blocks' lineages should still be queryable
        let from1 = store.get_edges_from(child.lineage_id).await.unwrap();
        assert_eq!(from1.len(), 1);
        assert_eq!(from1[0].id, edge1.id);

        let from2 = store.get_edges_from(gc.lineage_id).await.unwrap();
        assert_eq!(from2.len(), 1);
        assert_eq!(from2[0].id, edge2.id);

        let to_ext = store.get_edges_to(external.lineage_id).await.unwrap();
        assert_eq!(to_ext.len(), 2);
    }

    async fn test_orphan_not_listed_if_self_deleted(store: &dyn Store) {
        let props = json!({});
        let (parent, _) = store
            .create_block_with_content(None, "onl_parent", "", &[], "", &props)
            .await
            .unwrap();
        let (child, _) = store
            .create_block_with_content(Some(parent.id), "onl_child", "", &[], "", &props)
            .await
            .unwrap();

        // Delete both parent and child
        store.delete_block(parent.id).await.unwrap();
        store.delete_block(child.id).await.unwrap();

        let orphans = store.list_orphaned_blocks().await.unwrap();
        let orphan_ids: Vec<Uuid> = orphans.iter().map(|b| b.id).collect();
        assert!(
            !orphan_ids.contains(&child.id),
            "A deleted child should NOT appear in orphans list"
        );
    }

    async fn test_restore_block_no_cascade(store: &dyn Store) {
        let props = json!({});
        let (parent, _) = store
            .create_block_with_content(None, "rbnc_parent", "", &[], "", &props)
            .await
            .unwrap();
        let (child, _) = store
            .create_block_with_content(Some(parent.id), "rbnc_child", "", &[], "", &props)
            .await
            .unwrap();

        // Delete both parent and child individually
        store.delete_block(child.id).await.unwrap();
        store.delete_block(parent.id).await.unwrap();

        // Restore parent only
        store.restore_block(parent.id).await.unwrap();

        // Parent should be accessible
        let restored_parent = store.get_block(parent.id).await.unwrap();
        assert!(restored_parent.deleted_at.is_none());

        // Child should still be deleted
        assert!(store.get_block(child.id).await.is_err());

        // Child should NOT appear in get_block_children
        let children = store.get_block_children(parent.id).await.unwrap();
        assert!(
            children.is_empty(),
            "Restored parent should have no visible children (child still deleted)"
        );
    }

    async fn test_recursive_delete_partial_restore(store: &dyn Store) {
        let props = json!({});

        // Build A -> B -> C
        let (a, _) = store
            .create_block_with_content(None, "rdpr_a", "", &[], "", &props)
            .await
            .unwrap();
        let (b, _) = store
            .create_block_with_content(Some(a.id), "rdpr_b", "", &[], "", &props)
            .await
            .unwrap();
        let (c, _) = store
            .create_block_with_content(Some(b.id), "rdpr_c", "", &[], "", &props)
            .await
            .unwrap();

        // Recursively delete A (deletes A, B, C)
        let count = store.delete_block_recursive(a.id).await.unwrap();
        assert_eq!(count, 3);

        // Restore B only
        store.restore_block(b.id).await.unwrap();

        // B should be accessible
        let restored_b = store.get_block(b.id).await.unwrap();
        assert!(restored_b.deleted_at.is_none());

        // B should appear in orphans (parent A is still deleted)
        let orphans = store.list_orphaned_blocks().await.unwrap();
        let orphan_ids: Vec<Uuid> = orphans.iter().map(|b| b.id).collect();
        assert!(
            orphan_ids.contains(&b.id),
            "Restored B should be in orphans since parent A is still deleted"
        );

        // C should still be deleted
        assert!(store.get_block(c.id).await.is_err());

        // A should still be deleted
        assert!(store.get_block(a.id).await.is_err());
    }

    async fn test_move_block_to_deleted_parent(store: &dyn Store) {
        let props = json!({});

        let (live_parent, _) = store
            .create_block_with_content(None, "mtdp_live", "", &[], "", &props)
            .await
            .unwrap();
        let (dead_parent, _) = store
            .create_block_with_content(None, "mtdp_dead", "", &[], "", &props)
            .await
            .unwrap();
        let (child, _) = store
            .create_block_with_content(Some(live_parent.id), "mtdp_child", "", &[], "", &props)
            .await
            .unwrap();

        // Delete the target parent
        store.delete_block(dead_parent.id).await.unwrap();

        // Move child to the deleted parent — store-level does NOT validate parent existence
        let moved = store
            .move_block(child.id, Some(dead_parent.id), None)
            .await
            .unwrap();
        assert_eq!(moved.parent_id, Some(dead_parent.id));

        // Block should now appear in orphans (parent is deleted)
        let orphans = store.list_orphaned_blocks().await.unwrap();
        let orphan_ids: Vec<Uuid> = orphans.iter().map(|b| b.id).collect();
        assert!(
            orphan_ids.contains(&child.id),
            "Block moved to deleted parent should appear in orphans"
        );
    }

    // =========================================================================
    // P2.10: Link resolution against DB
    // =========================================================================

    async fn test_resolve_link_deleted_leaf(store: &dyn Store) {
        let props = json!({});
        let (root, _) = store
            .create_block_with_content(None, "rld_root", "", &[], "", &props)
            .await
            .unwrap();
        let (leaf, _) = store
            .create_block_with_content(Some(root.id), "rld_leaf", "", &[], "", &props)
            .await
            .unwrap();

        // Verify resolution works before delete
        let before = store
            .find_block_by_namespace("rld_root::rld_leaf")
            .await
            .unwrap();
        assert!(before.is_some());
        assert_eq!(before.unwrap().lineage_id, leaf.lineage_id);

        // Delete the leaf
        store.delete_block(leaf.id).await.unwrap();

        // Resolution should now return None
        let after = store
            .find_block_by_namespace("rld_root::rld_leaf")
            .await
            .unwrap();
        assert!(
            after.is_none(),
            "Deleted leaf block should not resolve via namespace"
        );
    }

    async fn test_resolve_link_deleted_intermediate(store: &dyn Store) {
        let props = json!({});

        // Build a::b::c
        let (a, _) = store
            .create_block_with_content(None, "rldi_a", "", &[], "", &props)
            .await
            .unwrap();
        let (b, _) = store
            .create_block_with_content(Some(a.id), "rldi_b", "", &[], "", &props)
            .await
            .unwrap();
        let (_c, _) = store
            .create_block_with_content(Some(b.id), "rldi_c", "", &[], "", &props)
            .await
            .unwrap();

        // Delete intermediate node b
        store.delete_block(b.id).await.unwrap();

        // Resolution of a::b::c should return None because b is deleted
        let result = store
            .find_block_by_namespace("rldi_a::rldi_b::rldi_c")
            .await
            .unwrap();
        assert!(
            result.is_none(),
            "Path through deleted intermediate should not resolve"
        );
    }

    async fn test_resolve_link_after_block_move(store: &dyn Store) {
        let props = json!({});

        // Build old_parent -> child
        let (old_parent, _) = store
            .create_block_with_content(None, "rlam_old", "", &[], "", &props)
            .await
            .unwrap();
        let (new_parent, _) = store
            .create_block_with_content(None, "rlam_new", "", &[], "", &props)
            .await
            .unwrap();
        let (child, _) = store
            .create_block_with_content(Some(old_parent.id), "rlam_child", "", &[], "", &props)
            .await
            .unwrap();

        // Old path resolves
        let old_result = store
            .find_block_by_namespace("rlam_old::rlam_child")
            .await
            .unwrap();
        assert!(old_result.is_some());

        // Move child to new_parent
        store
            .move_block(child.id, Some(new_parent.id), None)
            .await
            .unwrap();

        // Old path should no longer resolve
        let old_after = store
            .find_block_by_namespace("rlam_old::rlam_child")
            .await
            .unwrap();
        assert!(
            old_after.is_none(),
            "Old path should not resolve after move"
        );

        // New path should resolve
        let new_after = store
            .find_block_by_namespace("rlam_new::rlam_child")
            .await
            .unwrap();
        assert!(new_after.is_some());
        assert_eq!(new_after.unwrap().lineage_id, child.lineage_id);
    }

    async fn test_resolve_link_after_restore(store: &dyn Store) {
        let props = json!({});

        let (root, _) = store
            .create_block_with_content(None, "rlar_root", "", &[], "", &props)
            .await
            .unwrap();
        let (child, _) = store
            .create_block_with_content(Some(root.id), "rlar_child", "", &[], "", &props)
            .await
            .unwrap();

        // Delete
        store.delete_block(child.id).await.unwrap();
        let during = store
            .find_block_by_namespace("rlar_root::rlar_child")
            .await
            .unwrap();
        assert!(during.is_none(), "Should not resolve while deleted");

        // Restore
        store.restore_block(child.id).await.unwrap();
        let after = store
            .find_block_by_namespace("rlar_root::rlar_child")
            .await
            .unwrap();
        assert!(after.is_some(), "Should resolve again after restore");
        assert_eq!(after.unwrap().lineage_id, child.lineage_id);
    }

    async fn test_resolve_link_same_name_different_parents(store: &dyn Store) {
        let props = json!({});

        let (x, _) = store
            .create_block_with_content(None, "rlsn_x", "", &[], "", &props)
            .await
            .unwrap();
        let (y, _) = store
            .create_block_with_content(None, "rlsn_y", "", &[], "", &props)
            .await
            .unwrap();
        let (x_item, _) = store
            .create_block_with_content(Some(x.id), "rlsn_item", "", &[], "", &props)
            .await
            .unwrap();
        let (y_item, _) = store
            .create_block_with_content(Some(y.id), "rlsn_item", "", &[], "", &props)
            .await
            .unwrap();

        let resolved_x = store
            .find_block_by_namespace("rlsn_x::rlsn_item")
            .await
            .unwrap();
        let resolved_y = store
            .find_block_by_namespace("rlsn_y::rlsn_item")
            .await
            .unwrap();

        assert!(resolved_x.is_some());
        assert!(resolved_y.is_some());

        let x_lineage = resolved_x.unwrap().lineage_id;
        let y_lineage = resolved_y.unwrap().lineage_id;

        assert_ne!(
            x_lineage, y_lineage,
            "Same-name blocks under different parents should have different lineage IDs"
        );
        assert_eq!(x_lineage, x_item.lineage_id);
        assert_eq!(y_lineage, y_item.lineage_id);
    }

    async fn test_resolve_schema_deleted_falls_through(store: &dyn Store) {
        let props = json!({});

        // Create root-level types::rsdf_task schema
        store.ensure_namespace_block("types").await.unwrap();
        let root_types = store
            .find_block_by_namespace("types")
            .await
            .unwrap()
            .unwrap();
        let (root_schema, _) = store
            .create_block_with_content(Some(root_types.id), "rsdf_task", "", &[], "schema", &props)
            .await
            .unwrap();

        // Create a namespace with a local override: myns::types::rsdf_task
        store
            .ensure_namespace_block("rsdf_myns::types")
            .await
            .unwrap();
        let myns_types = store
            .find_block_by_namespace("rsdf_myns::types")
            .await
            .unwrap()
            .unwrap();
        let (override_schema, _) = store
            .create_block_with_content(Some(myns_types.id), "rsdf_task", "", &[], "schema", &props)
            .await
            .unwrap();

        // From context rsdf_myns, should resolve to the override
        let resolved = store
            .resolve_schema("rsdf_task", Some("rsdf_myns"))
            .await
            .unwrap();
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().id, override_schema.id);

        // Delete the override
        store.delete_block(override_schema.id).await.unwrap();

        // Now should fall through to the root schema
        let resolved_after = store
            .resolve_schema("rsdf_task", Some("rsdf_myns"))
            .await
            .unwrap();
        assert!(resolved_after.is_some());
        assert_eq!(
            resolved_after.unwrap().id,
            root_schema.id,
            "After deleting override, should fall through to root-level schema"
        );
    }

    async fn test_find_block_by_namespace_deleted_root(store: &dyn Store) {
        let props = json!({});

        // Build a::b::c
        let (a, _) = store
            .create_block_with_content(None, "fbnd_a", "", &[], "", &props)
            .await
            .unwrap();
        let (b, _) = store
            .create_block_with_content(Some(a.id), "fbnd_b", "", &[], "", &props)
            .await
            .unwrap();
        let (_c, _) = store
            .create_block_with_content(Some(b.id), "fbnd_c", "", &[], "", &props)
            .await
            .unwrap();

        // Delete root
        store.delete_block(a.id).await.unwrap();

        // find_block_by_namespace should return None
        let result = store
            .find_block_by_namespace("fbnd_a::fbnd_b::fbnd_c")
            .await
            .unwrap();
        assert!(
            result.is_none(),
            "Path starting from deleted root should not resolve"
        );
    }
}
