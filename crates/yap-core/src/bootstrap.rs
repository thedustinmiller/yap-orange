//! Bootstrap logic for initialising a fresh yap-orange database.
//!
//! All functions are idempotent — safe to call on every startup.
//! They use only `dyn Store` trait methods and therefore work across
//! all backends (PostgreSQL, SQLite, WASM SQLite).

use crate::store::Store;

/// Ensure the `types::schema` meta-schema block exists and
/// the `types` namespace has `content_type = "type_registry"`.
pub async fn ensure_meta_schema(db: &dyn Store) -> anyhow::Result<()> {
    // Always ensure the types namespace exists and has the right content_type
    db.ensure_namespace_block("types")
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Upgrade the types block's content_type to "type_registry" if needed.
    // This is idempotent — edit_lineage creates a new atom snapshot only if
    // the content actually differs.
    ensure_block_content_type(db, "types", "type_registry", "types").await?;

    // Check if meta-schema already exists
    if db
        .find_block_by_namespace("types::schema")
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .is_some()
    {
        return Ok(());
    }

    tracing::info!("Creating types::schema meta-schema block...");

    let meta_fields = serde_json::json!({
        "fields": [
            { "name": "name",        "type": "string",  "required": true  },
            { "name": "type",        "type": "enum",    "required": true,
              "options": ["string", "number", "boolean", "date", "enum", "ref", "text"] },
            { "name": "options",     "type": "text",    "required": false },
            { "name": "required",    "type": "boolean", "required": false },
            { "name": "target_type", "type": "string",  "required": false }
        ]
    });

    let types_block = db
        .find_block_by_namespace("types")
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .ok_or_else(|| anyhow::anyhow!("types namespace not found after ensure"))?;

    db.create_block_with_content(
        Some(types_block.id),
        "schema",
        "",
        &[],
        "schema",
        &meta_fields,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{}", e))?;

    tracing::info!("Created types::schema meta-schema block");
    Ok(())
}

/// Ensure a block at the given namespace path has the expected content_type.
/// If the current atom has a different content_type, creates a new atom snapshot.
pub async fn ensure_block_content_type(
    db: &dyn Store,
    namespace: &str,
    expected_content_type: &str,
    content_template: &str,
) -> anyhow::Result<()> {
    let block = db
        .find_block_by_namespace(namespace)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let block = match block {
        Some(b) => b,
        None => return Ok(()), // Block doesn't exist yet, nothing to upgrade
    };

    // Check current content_type by fetching the atom
    let atom = db
        .get_atom(block.lineage_id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    if atom.content_type == expected_content_type {
        return Ok(()); // Already correct
    }

    tracing::info!(
        "Upgrading {namespace} content_type from '{}' to '{expected_content_type}'",
        atom.content_type
    );

    // Preserve existing properties (especially "name") when upgrading content_type
    db.edit_lineage(
        block.lineage_id,
        expected_content_type,
        content_template,
        &atom.links,
        &atom.properties,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(())
}

/// Ensure the `types::todo` schema block exists with status + time_ranges fields.
pub async fn ensure_todo_schema(db: &dyn Store) -> anyhow::Result<()> {
    if db
        .find_block_by_namespace("types::todo")
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .is_some()
    {
        return Ok(());
    }

    tracing::info!("Creating types::todo schema block...");

    let types_block = db
        .find_block_by_namespace("types")
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .ok_or_else(|| anyhow::anyhow!("types namespace not found"))?;

    let todo_fields = serde_json::json!({
        "fields": [
            { "name": "status",      "type": "enum", "required": true,
              "options": ["todo", "doing", "done"] },
            { "name": "description", "type": "text", "required": false },
            { "name": "time_ranges", "type": "text", "required": false }
        ]
    });

    db.create_block_with_content(
        Some(types_block.id),
        "todo",
        "",
        &[],
        "schema",
        &todo_fields,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{}", e))?;

    tracing::info!("Created types::todo schema block");
    Ok(())
}

/// Ensure the `types::person` schema block exists with name/email/birthday/notes fields.
pub async fn ensure_person_schema(db: &dyn Store) -> anyhow::Result<()> {
    if db
        .find_block_by_namespace("types::person")
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .is_some()
    {
        return Ok(());
    }

    tracing::info!("Creating types::person schema block...");

    let types_block = db
        .find_block_by_namespace("types")
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .ok_or_else(|| anyhow::anyhow!("types namespace not found"))?;

    let person_fields = serde_json::json!({
        "fields": [
            { "name": "name",     "type": "string",  "required": true  },
            { "name": "email",    "type": "string",  "required": false },
            { "name": "birthday", "type": "date",    "required": false },
            { "name": "notes",    "type": "text",    "required": false }
        ]
    });

    db.create_block_with_content(
        Some(types_block.id),
        "person",
        "",
        &[],
        "schema",
        &person_fields,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{}", e))?;

    tracing::info!("Created types::person schema block");
    Ok(())
}

/// Ensure the `settings::ui` block exists with default settings.
pub async fn ensure_settings(db: &dyn Store) -> anyhow::Result<()> {
    if db
        .find_block_by_namespace("settings::ui")
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .is_some()
    {
        return Ok(());
    }

    tracing::info!("Creating settings::ui block...");

    db.ensure_namespace_block("settings")
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let settings_block = db
        .find_block_by_namespace("settings")
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .ok_or_else(|| anyhow::anyhow!("settings namespace not found after ensure"))?;

    let default_settings = serde_json::json!({
        "theme": "dark",
        "font_size": 13
    });

    db.create_block_with_content(
        Some(settings_block.id),
        "ui",
        "",
        &[],
        "setting",
        &default_settings,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{}", e))?;

    tracing::info!("Created settings::ui block");
    Ok(())
}

/// Full bootstrap: meta-schema + settings + optional seed trees.
///
/// `seed_trees` is a slice of pre-exported trees to import at root level.
/// Pass `&[]` for a minimal bootstrap (meta-schema + settings only).
///
/// Seed trees are only imported on first run (when the database has no root
/// blocks). Subsequent startups skip seeding entirely for fast startup.
/// `ensure_meta_schema` and `ensure_settings` always run (they are
/// individually idempotent).
pub async fn bootstrap(
    db: &dyn Store,
    seed_trees: &[crate::export::ExportTree],
) -> anyhow::Result<()> {
    let roots_before = db
        .get_root_blocks()
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    ensure_meta_schema(db).await?;
    ensure_todo_schema(db).await?;
    ensure_person_schema(db).await?;
    ensure_settings(db).await?;

    if roots_before.is_empty() && !seed_trees.is_empty() {
        tracing::info!(
            "First run detected — importing {} seed tree(s)",
            seed_trees.len()
        );
        for tree in seed_trees {
            let ns = &tree.source_namespace;
            match crate::export::import_tree(
                db,
                tree,
                None,
                crate::export::ImportOptions::seed_defaults(),
            )
            .await
            {
                Ok(result) => {
                    tracing::info!(
                        "Seed '{}': {} created, {} skipped, {} edges",
                        ns,
                        result.created,
                        result.skipped,
                        result.edges_created,
                    );
                }
                Err(e) => {
                    tracing::warn!("Failed to import seed tree '{}': {}", ns, e);
                }
            }
        }
    }

    Ok(())
}
