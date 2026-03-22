//! SQLite backend for yap-orange.
//!
//! Implements `Store` for `SqliteStore`. All SQLite-specific SQL lives here.
//! Key differences from PgStore:
//! - `atom_links` junction table instead of `UUID[]`
//! - Bind parameters use `?1` instead of `$1`
//! - UUIDs and timestamps bind as strings
//! - `LIKE` instead of `ILIKE` (SQLite LIKE is case-insensitive for ASCII)
//! - `SELECT EXISTS(...)` returns integer 0/1
//!
//! After the name->properties migration, block names live in
//! `json_extract(atoms.properties, '$.name')`. Every block SELECT joins through
//! lineages->atoms to populate the name field.

mod rows;

use async_trait::async_trait;
use chrono::Utc;
use fractional_index::FractionalIndex;
use sqlx::SqlitePool;
use uuid::Uuid;

use yap_core::Store;
use yap_core::error::{Error, Result};
use yap_core::models::{Atom, Backlink, Block, CreateAtom, CreateEdge, Edge, Lineage, UpdateBlock};

use rows::{AtomRow, BacklinkRow, BlockRow, EdgeRow, LineageRow};

// =============================================================================
// Error mapping
// =============================================================================

/// Convert a sqlx error to a yap-core Error.
/// SQLite unique constraint (2067) and PK constraint (1555) -> Error::Conflict.
#[allow(clippy::needless_pass_by_value)] // used as .map_err(map_err)
fn map_err(e: sqlx::Error) -> Error {
    match &e {
        sqlx::Error::Database(dbe) => {
            let code = dbe.code();
            match code.as_deref() {
                Some("2067") | Some("1555") => Error::Conflict(dbe.message().to_string()),
                _ => Error::Database(e.to_string()),
            }
        }
        _ => Error::Database(e.to_string()),
    }
}

trait SqliteExt<T> {
    fn sq(self) -> Result<T>;
}

impl<T> SqliteExt<T> for std::result::Result<T, sqlx::Error> {
    fn sq(self) -> Result<T> {
        self.map_err(map_err)
    }
}

// =============================================================================
// Hash
// =============================================================================

fn content_hash(content_type: &str, template: &str, links: &[Uuid]) -> String {
    yap_core::hash::compute_content_hash(content_type, template, links)
}

// =============================================================================
// Helpers
// =============================================================================

/// Load links for an atom from the junction table.
async fn load_atom_links<'e, E>(executor: E, atom_id: &str) -> Result<Vec<Uuid>>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT lineage_id FROM atom_links WHERE atom_id = ?1 ORDER BY position ASC",
    )
    .bind(atom_id)
    .fetch_all(executor)
    .await
    .sq()?;

    Ok(rows
        .into_iter()
        .map(|(s,)| Uuid::parse_str(&s).unwrap_or_default())
        .collect())
}

/// Insert links into the atom_links junction table.
/// Takes `&mut SqliteConnection` (not `impl Executor`) because it executes N queries.
async fn insert_atom_links(
    conn: &mut sqlx::SqliteConnection,
    atom_id: &str,
    links: &[Uuid],
) -> Result<()> {
    for (i, link) in links.iter().enumerate() {
        sqlx::query("INSERT INTO atom_links (atom_id, lineage_id, position) VALUES (?1, ?2, ?3)")
            .bind(atom_id)
            .bind(link.to_string())
            .bind(i as i32)
            .execute(&mut *conn)
            .await
            .sq()?;
    }
    Ok(())
}

// =============================================================================
// Reusable query helpers — generic over Executor (pool or transaction)
// =============================================================================

async fn sq_find_by_parent_name<'e, E>(
    executor: E,
    parent_id: Option<Uuid>,
    name: &str,
) -> Result<Option<Block>>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let row: Option<BlockRow> = match parent_id {
        Some(pid) => {
            sqlx::query_as(&format!(
                "{} WHERE b.parent_id = ?1 AND json_extract(a.properties, '$.name') = ?2 AND b.deleted_at IS NULL AND l.deleted_at IS NULL",
                BLOCK_SELECT
            ))
            .bind(pid.to_string())
            .bind(name)
            .fetch_optional(executor)
            .await
            .sq()?
        }
        None => {
            sqlx::query_as(&format!(
                "{} WHERE b.parent_id IS NULL AND json_extract(a.properties, '$.name') = ?1 AND b.deleted_at IS NULL AND l.deleted_at IS NULL",
                BLOCK_SELECT
            ))
            .bind(name)
            .fetch_optional(executor)
            .await
            .sq()?
        }
    };
    Ok(row.map(Block::from))
}

async fn sq_next_position<'e, E>(executor: E, parent_id: Option<Uuid>) -> Result<String>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let last_pos: Option<String> = match parent_id {
        Some(pid) => sqlx::query_scalar(
            r#"
                SELECT position FROM blocks
                WHERE parent_id = ?1 AND deleted_at IS NULL
                ORDER BY position DESC
                LIMIT 1
                "#,
        )
        .bind(pid.to_string())
        .fetch_optional(executor)
        .await
        .sq()?,
        None => sqlx::query_scalar(
            r#"
                SELECT position FROM blocks
                WHERE parent_id IS NULL AND deleted_at IS NULL
                ORDER BY position DESC
                LIMIT 1
                "#,
        )
        .fetch_optional(executor)
        .await
        .sq()?,
    };

    match last_pos {
        Some(prev) => {
            let prev_fi = FractionalIndex::from_string(&prev)
                .map_err(|e| Error::Internal(format!("fractional index error: {}", e)))?;
            let next = FractionalIndex::new_after(&prev_fi);
            Ok(next.to_string())
        }
        None => Ok(FractionalIndex::default().to_string()),
    }
}

async fn sq_fetch_block<'e, E>(executor: E, id: Uuid) -> Result<Block>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let row: Option<BlockRow> = sqlx::query_as(&format!(
        "{} WHERE b.id = ?1 AND b.deleted_at IS NULL",
        BLOCK_SELECT
    ))
    .bind(id.to_string())
    .fetch_optional(executor)
    .await
    .sq()?;

    row.map(Block::from)
        .ok_or_else(|| Error::NotFound(format!("Block {} not found", id)))
}

async fn sq_fetch_lineage<'e, E>(executor: E, lineage_id: Uuid) -> Result<Lineage>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let row: Option<LineageRow> =
        sqlx::query_as("SELECT * FROM lineages WHERE id = ?1 AND deleted_at IS NULL")
            .bind(lineage_id.to_string())
            .fetch_optional(executor)
            .await
            .sq()?;

    row.map(Lineage::from)
        .ok_or_else(|| Error::NotFound(format!("Lineage {} not found", lineage_id)))
}

/// Fetch atom + links inside a transaction (needs reborrow for two queries).
async fn sq_fetch_atom_tx(conn: &mut sqlx::SqliteConnection, lineage_id: Uuid) -> Result<Atom> {
    let row: Option<AtomRow> = sqlx::query_as(
        r#"
        SELECT a.* FROM atoms a
        JOIN lineages l ON l.current_id = a.id
        WHERE l.id = ?1 AND l.deleted_at IS NULL
        "#,
    )
    .bind(lineage_id.to_string())
    .fetch_optional(&mut *conn)
    .await
    .sq()?;

    match row {
        Some(r) => {
            let links = load_atom_links(&mut *conn, &r.id).await?;
            Ok(r.into_atom(links))
        }
        None => Err(Error::NotFound(format!("Lineage {} not found", lineage_id))),
    }
}

// =============================================================================
// Common SQL fragments
// =============================================================================

/// Block SELECT with JOIN to get name from atom properties.
/// Returns columns: id, lineage_id, parent_id, name, position, deleted_at, created_at
const BLOCK_SELECT: &str = r#"
    SELECT b.id, b.lineage_id, b.parent_id,
           COALESCE(json_extract(a.properties, '$.name'), '') as name,
           b.position, b.deleted_at, b.created_at
    FROM blocks b
    JOIN lineages l ON b.lineage_id = l.id
    JOIN atoms a ON l.current_id = a.id
"#;

// =============================================================================
// SqliteStore
// =============================================================================

/// SQLite-backed implementation of `Store`.
#[derive(Debug, Clone)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Connect to a SQLite database and enable WAL mode + foreign keys.
    pub async fn connect(url: &str) -> Result<Self> {
        let pool = SqlitePool::connect(url)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        sqlx::query("PRAGMA journal_mode=WAL")
            .execute(&pool)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&pool)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        Ok(Self { pool })
    }
}

/// Run the SQLite migration (creates tables if they don't exist).
pub async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    sqlx::migrate!("../../migrations/sqlite")
        .run(pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))
}

// =============================================================================
// Store implementation
// =============================================================================

#[async_trait]
impl Store for SqliteStore {
    // -------------------------------------------------------------------------
    // Health
    // -------------------------------------------------------------------------

    async fn health_check(&self) -> Result<bool> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map(|_| true)
            .sq()
    }

    // -------------------------------------------------------------------------
    // Admin
    // -------------------------------------------------------------------------

    async fn is_empty(&self) -> Result<bool> {
        let count: (i32,) = sqlx::query_as("SELECT COUNT(*) FROM atoms")
            .fetch_one(&self.pool)
            .await
            .sq()?;
        Ok(count.0 == 0)
    }

    async fn clear_all_data(&self) -> Result<()> {
        // DELETE in FK-safe order (no TRUNCATE in SQLite).
        // atom_links references atoms; blocks/edges reference lineages.
        sqlx::query("DELETE FROM edges")
            .execute(&self.pool)
            .await
            .sq()?;
        sqlx::query("DELETE FROM blocks")
            .execute(&self.pool)
            .await
            .sq()?;
        sqlx::query("DELETE FROM lineages")
            .execute(&self.pool)
            .await
            .sq()?;
        sqlx::query("DELETE FROM atom_links")
            .execute(&self.pool)
            .await
            .sq()?;
        sqlx::query("DELETE FROM atoms")
            .execute(&self.pool)
            .await
            .sq()?;
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Namespace / Path helpers (SQL)
    // -------------------------------------------------------------------------

    async fn compute_namespace(&self, block_id: Uuid) -> Result<String> {
        let rows: Vec<(String,)> = sqlx::query_as(
            r#"
            WITH RECURSIVE ancestors AS (
                SELECT b.id, b.parent_id,
                       COALESCE(json_extract(a.properties, '$.name'), '') as name,
                       0 AS depth
                FROM blocks b
                JOIN lineages l ON b.lineage_id = l.id
                JOIN atoms a ON l.current_id = a.id
                WHERE b.id = ?1
                UNION ALL
                SELECT b.id, b.parent_id,
                       COALESCE(json_extract(a.properties, '$.name'), '') as name,
                       anc.depth + 1
                FROM blocks b
                JOIN lineages l ON b.lineage_id = l.id
                JOIN atoms a ON l.current_id = a.id
                JOIN ancestors anc ON b.id = anc.parent_id
            )
            SELECT name FROM ancestors ORDER BY depth DESC
            "#,
        )
        .bind(block_id.to_string())
        .fetch_all(&self.pool)
        .await
        .sq()?;

        Ok(rows
            .into_iter()
            .map(|(n,)| n)
            .collect::<Vec<_>>()
            .join("::"))
    }

    async fn find_block_by_parent_and_name(
        &self,
        parent_id: Option<Uuid>,
        name: &str,
    ) -> Result<Option<Block>> {
        sq_find_by_parent_name(&self.pool, parent_id, name).await
    }

    async fn get_next_position(&self, parent_id: Option<Uuid>) -> Result<String> {
        sq_next_position(&self.pool, parent_id).await
    }

    // -------------------------------------------------------------------------
    // Atom + Lineage (SQL)
    // -------------------------------------------------------------------------

    async fn create_atom(&self, create: &CreateAtom) -> Result<(Atom, Lineage)> {
        let mut tx = self.pool.begin().await.sq()?;

        let id = Uuid::now_v7();
        let now = Utc::now();
        let hash = content_hash(
            &create.content_type,
            &create.content_template,
            &create.links,
        );
        let id_str = id.to_string();
        let now_str = now.to_rfc3339();
        let props = serde_json::to_string(&create.properties).unwrap_or_default();

        let atom_row: AtomRow = sqlx::query_as(
            r#"
            INSERT INTO atoms (id, content_type, content_template, properties, content_hash, predecessor_id, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)
            RETURNING *
            "#,
        )
        .bind(&id_str)
        .bind(&create.content_type)
        .bind(&create.content_template)
        .bind(&props)
        .bind(&hash)
        .bind(&now_str)
        .fetch_one(&mut *tx)
        .await
        .sq()?;

        insert_atom_links(&mut tx, &id_str, &create.links).await?;

        let lineage_row: LineageRow = sqlx::query_as(
            r#"
            INSERT INTO lineages (id, current_id, version, updated_at)
            VALUES (?1, ?2, 1, ?3)
            RETURNING *
            "#,
        )
        .bind(&id_str)
        .bind(&id_str)
        .bind(&now_str)
        .fetch_one(&mut *tx)
        .await
        .sq()?;

        tx.commit().await.sq()?;
        let atom = atom_row.into_atom(create.links.clone());
        Ok((atom, lineage_row.into()))
    }

    async fn get_atom(&self, lineage_id: Uuid) -> Result<Atom> {
        let row: Option<AtomRow> = sqlx::query_as(
            r#"
            SELECT a.* FROM atoms a
            JOIN lineages l ON l.current_id = a.id
            WHERE l.id = ?1 AND l.deleted_at IS NULL
            "#,
        )
        .bind(lineage_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .sq()?;

        match row {
            Some(r) => {
                let links = load_atom_links(&self.pool, &r.id).await?;
                Ok(r.into_atom(links))
            }
            None => Err(Error::NotFound(format!("Lineage {} not found", lineage_id))),
        }
    }

    async fn get_atom_by_id(&self, atom_id: Uuid) -> Result<Atom> {
        let row: Option<AtomRow> = sqlx::query_as("SELECT * FROM atoms WHERE id = ?1")
            .bind(atom_id.to_string())
            .fetch_optional(&self.pool)
            .await
            .sq()?;
        match row {
            Some(r) => {
                let links = load_atom_links(&self.pool, &r.id).await?;
                Ok(r.into_atom(links))
            }
            None => Err(Error::NotFound(format!("Atom {} not found", atom_id))),
        }
    }

    async fn get_lineage(&self, lineage_id: Uuid) -> Result<Lineage> {
        sq_fetch_lineage(&self.pool, lineage_id).await
    }

    async fn get_lineage_with_deleted(&self, lineage_id: Uuid) -> Result<Lineage> {
        let row: Option<LineageRow> = sqlx::query_as("SELECT * FROM lineages WHERE id = ?1")
            .bind(lineage_id.to_string())
            .fetch_optional(&self.pool)
            .await
            .sq()?;

        row.map(Lineage::from)
            .ok_or_else(|| Error::NotFound(format!("Lineage {} not found", lineage_id)))
    }

    async fn edit_lineage(
        &self,
        lineage_id: Uuid,
        content_type: &str,
        content_template: &str,
        links: &[Uuid],
        properties: &serde_json::Value,
    ) -> Result<(Atom, Lineage)> {
        let mut tx = self.pool.begin().await.sq()?;

        let lineage = sq_fetch_lineage(&mut *tx, lineage_id).await?;
        let new_atom_id = Uuid::now_v7();
        let now = Utc::now();
        let hash = content_hash(content_type, content_template, links);
        let new_id_str = new_atom_id.to_string();
        let now_str = now.to_rfc3339();
        let props = serde_json::to_string(properties).unwrap_or_default();

        let atom_row: AtomRow = sqlx::query_as(
            r#"
            INSERT INTO atoms (id, content_type, content_template, properties, content_hash, predecessor_id, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            RETURNING *
            "#,
        )
        .bind(&new_id_str)
        .bind(content_type)
        .bind(content_template)
        .bind(&props)
        .bind(&hash)
        .bind(lineage.current_id.to_string())
        .bind(&now_str)
        .fetch_one(&mut *tx)
        .await
        .sq()?;

        insert_atom_links(&mut tx, &new_id_str, links).await?;

        let updated_lineage: LineageRow = sqlx::query_as(
            r#"
            UPDATE lineages
            SET current_id = ?2, version = version + 1, updated_at = ?3
            WHERE id = ?1 AND deleted_at IS NULL
            RETURNING *
            "#,
        )
        .bind(lineage_id.to_string())
        .bind(&new_id_str)
        .bind(&now_str)
        .fetch_one(&mut *tx)
        .await
        .sq()?;

        tx.commit().await.sq()?;
        let atom = atom_row.into_atom(links.to_vec());
        Ok((atom, updated_lineage.into()))
    }

    async fn delete_lineage(&self, lineage_id: Uuid) -> Result<Lineage> {
        let now_str = Utc::now().to_rfc3339();
        let row: Option<LineageRow> = sqlx::query_as(
            r#"
            UPDATE lineages
            SET deleted_at = ?2
            WHERE id = ?1 AND deleted_at IS NULL
            RETURNING *
            "#,
        )
        .bind(lineage_id.to_string())
        .bind(&now_str)
        .fetch_optional(&self.pool)
        .await
        .sq()?;

        row.map(Lineage::from).ok_or_else(|| {
            Error::NotFound(format!(
                "Lineage {} not found or already deleted",
                lineage_id
            ))
        })
    }

    // -------------------------------------------------------------------------
    // Block (SQL) -- name lives in json_extract(atoms.properties, '$.name')
    // -------------------------------------------------------------------------

    async fn create_block_with_content(
        &self,
        parent_id: Option<Uuid>,
        name: &str,
        content_template: &str,
        links: &[Uuid],
        content_type: &str,
        properties: &serde_json::Value,
    ) -> Result<(Block, Atom)> {
        let mut tx = self.pool.begin().await.sq()?;

        if sq_find_by_parent_name(&mut *tx, parent_id, name)
            .await?
            .is_some()
        {
            return Err(Error::Conflict(format!(
                "Block '{}' already exists under this parent",
                name
            )));
        }

        let block_id = Uuid::now_v7();
        let atom_id = Uuid::now_v7();
        let now = Utc::now();
        let position = sq_next_position(&mut *tx, parent_id).await?;
        let hash = content_hash(content_type, content_template, links);
        let atom_id_str = atom_id.to_string();
        let block_id_str = block_id.to_string();
        let now_str = now.to_rfc3339();

        // Inject name into atom properties
        let mut props = properties.clone();
        if let Some(obj) = props.as_object_mut() {
            obj.insert(
                "name".to_string(),
                serde_json::Value::String(name.to_string()),
            );
        }
        let props_str = serde_json::to_string(&props).unwrap_or_default();

        let atom_row: AtomRow = sqlx::query_as(
            r#"
            INSERT INTO atoms (id, content_type, content_template, properties, content_hash, predecessor_id, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)
            RETURNING *
            "#,
        )
        .bind(&atom_id_str)
        .bind(content_type)
        .bind(content_template)
        .bind(&props_str)
        .bind(&hash)
        .bind(&now_str)
        .fetch_one(&mut *tx)
        .await
        .sq()?;

        insert_atom_links(&mut tx, &atom_id_str, links).await?;

        sqlx::query_as::<_, LineageRow>(
            r#"
            INSERT INTO lineages (id, current_id, version, updated_at)
            VALUES (?1, ?2, 1, ?3)
            RETURNING *
            "#,
        )
        .bind(&atom_id_str)
        .bind(&atom_id_str)
        .bind(&now_str)
        .fetch_one(&mut *tx)
        .await
        .sq()?;

        sqlx::query(
            r#"
            INSERT INTO blocks (id, lineage_id, parent_id, position, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
        )
        .bind(&block_id_str)
        .bind(&atom_id_str)
        .bind(parent_id.map(|p| p.to_string()))
        .bind(&position)
        .bind(&now_str)
        .execute(&mut *tx)
        .await
        .sq()?;

        tx.commit().await.sq()?;

        let block = Block {
            id: block_id,
            lineage_id: atom_id,
            parent_id,
            name: name.to_string(),
            position,
            deleted_at: None,
            created_at: now,
        };

        let atom = atom_row.into_atom(links.to_vec());
        Ok((block, atom))
    }

    async fn get_block(&self, id: Uuid) -> Result<Block> {
        sq_fetch_block(&self.pool, id).await
    }

    async fn get_block_with_deleted(&self, id: Uuid) -> Result<Block> {
        let row: Option<BlockRow> = sqlx::query_as(&format!("{} WHERE b.id = ?1", BLOCK_SELECT))
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .sq()?;

        row.map(Block::from)
            .ok_or_else(|| Error::NotFound(format!("Block {} not found", id)))
    }

    async fn update_block(&self, id: Uuid, update: &UpdateBlock) -> Result<Block> {
        let mut tx = self.pool.begin().await.sq()?;

        let existing = sq_fetch_block(&mut *tx, id).await?;

        // If name is being updated, edit the atom properties
        if let Some(new_name) = &update.name {
            let atom = sq_fetch_atom_tx(&mut tx, existing.lineage_id).await?;
            let mut props = atom.properties.clone();
            if let Some(obj) = props.as_object_mut() {
                obj.insert(
                    "name".to_string(),
                    serde_json::Value::String(new_name.clone()),
                );
            }

            let lineage = sq_fetch_lineage(&mut *tx, existing.lineage_id).await?;
            let new_atom_id = Uuid::now_v7();
            let now = Utc::now();
            let hash = content_hash(&atom.content_type, &atom.content_template, &atom.links);
            let new_id_str = new_atom_id.to_string();
            let now_str = now.to_rfc3339();
            let props_str = serde_json::to_string(&props).unwrap_or_default();

            sqlx::query(
                r#"
                INSERT INTO atoms (id, content_type, content_template, properties, content_hash, predecessor_id, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
            )
            .bind(&new_id_str)
            .bind(&atom.content_type)
            .bind(&atom.content_template)
            .bind(&props_str)
            .bind(&hash)
            .bind(lineage.current_id.to_string())
            .bind(&now_str)
            .execute(&mut *tx)
            .await
            .sq()?;

            insert_atom_links(&mut tx, &new_id_str, &atom.links).await?;

            sqlx::query(
                "UPDATE lineages SET current_id = ?2, version = version + 1, updated_at = ?3 WHERE id = ?1 AND deleted_at IS NULL",
            )
            .bind(existing.lineage_id.to_string())
            .bind(&new_id_str)
            .bind(&now_str)
            .execute(&mut *tx)
            .await
            .sq()?;
        }

        // Update position if provided
        if let Some(new_position) = &update.position {
            sqlx::query("UPDATE blocks SET position = ?2 WHERE id = ?1 AND deleted_at IS NULL")
                .bind(id.to_string())
                .bind(new_position)
                .execute(&mut *tx)
                .await
                .sq()?;
        }

        let block = sq_fetch_block(&mut *tx, id).await?;
        tx.commit().await.sq()?;
        Ok(block)
    }

    async fn delete_block(&self, id: Uuid) -> Result<Block> {
        let now_str = Utc::now().to_rfc3339();
        let result =
            sqlx::query("UPDATE blocks SET deleted_at = ?2 WHERE id = ?1 AND deleted_at IS NULL")
                .bind(id.to_string())
                .bind(&now_str)
                .execute(&self.pool)
                .await
                .sq()?;

        if result.rows_affected() == 0 {
            return Err(Error::NotFound(format!(
                "Block {} not found or already deleted",
                id
            )));
        }

        self.get_block_with_deleted(id).await
    }

    async fn delete_block_recursive(&self, id: Uuid) -> Result<u64> {
        let _ = self.get_block(id).await?;
        let now_str = Utc::now().to_rfc3339();

        let result = sqlx::query(
            r#"
            WITH RECURSIVE tree AS (
                SELECT id FROM blocks WHERE id = ?1 AND deleted_at IS NULL
                UNION ALL
                SELECT b.id FROM blocks b
                JOIN tree t ON b.parent_id = t.id
                WHERE b.deleted_at IS NULL
            )
            UPDATE blocks SET deleted_at = ?2
            WHERE id IN (SELECT id FROM tree)
            "#,
        )
        .bind(id.to_string())
        .bind(&now_str)
        .execute(&self.pool)
        .await
        .sq()?;

        Ok(result.rows_affected())
    }

    async fn restore_block(&self, id: Uuid) -> Result<Block> {
        let result = sqlx::query(
            "UPDATE blocks SET deleted_at = NULL WHERE id = ?1 AND deleted_at IS NOT NULL",
        )
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .sq()?;

        if result.rows_affected() == 0 {
            return Err(Error::NotFound(format!(
                "Block {} not found or not deleted",
                id
            )));
        }

        self.get_block(id).await
    }

    async fn restore_block_recursive(&self, id: Uuid) -> Result<u64> {
        let result = sqlx::query(
            r#"
            WITH RECURSIVE tree AS (
                SELECT id FROM blocks WHERE id = ?1 AND deleted_at IS NOT NULL
                UNION ALL
                SELECT b.id FROM blocks b
                JOIN tree t ON b.parent_id = t.id
                WHERE b.deleted_at IS NOT NULL
            )
            UPDATE blocks SET deleted_at = NULL
            WHERE id IN (SELECT id FROM tree)
            "#,
        )
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .sq()?;

        Ok(result.rows_affected())
    }

    async fn get_block_children(&self, parent_id: Uuid) -> Result<Vec<Block>> {
        let rows: Vec<BlockRow> = sqlx::query_as(&format!(
            "{} WHERE b.parent_id = ?1 AND b.deleted_at IS NULL ORDER BY b.position ASC",
            BLOCK_SELECT
        ))
        .bind(parent_id.to_string())
        .fetch_all(&self.pool)
        .await
        .sq()?;

        Ok(rows.into_iter().map(Block::from).collect())
    }

    async fn get_root_blocks(&self) -> Result<Vec<Block>> {
        let rows: Vec<BlockRow> = sqlx::query_as(&format!(
            "{} WHERE b.parent_id IS NULL AND b.deleted_at IS NULL ORDER BY b.position ASC",
            BLOCK_SELECT
        ))
        .fetch_all(&self.pool)
        .await
        .sq()?;

        Ok(rows.into_iter().map(Block::from).collect())
    }

    async fn list_blocks_by_namespace(&self, namespace_prefix: &str) -> Result<Vec<Block>> {
        let root = self.find_block_by_namespace(namespace_prefix).await?;
        match root {
            Some(b) => {
                let rows: Vec<BlockRow> = sqlx::query_as(
                    r#"
                    WITH RECURSIVE subtree AS (
                        SELECT b.id, b.lineage_id, b.parent_id,
                               COALESCE(json_extract(a.properties, '$.name'), '') as name,
                               b.position, b.deleted_at, b.created_at
                        FROM blocks b
                        JOIN lineages l ON b.lineage_id = l.id
                        JOIN atoms a ON l.current_id = a.id
                        WHERE b.id = ?1 AND b.deleted_at IS NULL
                        UNION ALL
                        SELECT b.id, b.lineage_id, b.parent_id,
                               COALESCE(json_extract(a.properties, '$.name'), '') as name,
                               b.position, b.deleted_at, b.created_at
                        FROM blocks b
                        JOIN lineages l ON b.lineage_id = l.id
                        JOIN atoms a ON l.current_id = a.id
                        JOIN subtree s ON b.parent_id = s.id
                        WHERE b.deleted_at IS NULL
                    )
                    SELECT * FROM subtree ORDER BY position ASC
                    "#,
                )
                .bind(b.id.to_string())
                .fetch_all(&self.pool)
                .await
                .sq()?;

                Ok(rows.into_iter().map(Block::from).collect())
            }
            None => Ok(Vec::new()),
        }
    }

    async fn list_orphaned_blocks(&self) -> Result<Vec<Block>> {
        let rows: Vec<BlockRow> = sqlx::query_as(&format!(
            r#"
            {} WHERE b.deleted_at IS NULL
              AND b.parent_id IS NOT NULL
              AND NOT EXISTS (
                  SELECT 1 FROM blocks parent
                  WHERE parent.id = b.parent_id
                    AND parent.deleted_at IS NULL
              )
            ORDER BY COALESCE(json_extract(a.properties, '$.name'), '') ASC
            "#,
            BLOCK_SELECT
        ))
        .fetch_all(&self.pool)
        .await
        .sq()?;

        Ok(rows.into_iter().map(Block::from).collect())
    }

    async fn search_blocks(&self, query: &str) -> Result<Vec<Block>> {
        let pattern = format!("%{}%", query);
        let rows: Vec<BlockRow> = sqlx::query_as(
            r#"
            WITH RECURSIVE ns AS (
                SELECT b.id,
                       COALESCE(json_extract(a.properties, '$.name'), '') as name,
                       b.parent_id, b.lineage_id, b.position, b.deleted_at, b.created_at,
                       COALESCE(json_extract(a.properties, '$.name'), '') AS namespace
                FROM blocks b
                JOIN lineages l ON b.lineage_id = l.id
                JOIN atoms a ON l.current_id = a.id
                WHERE b.parent_id IS NULL AND b.deleted_at IS NULL AND l.deleted_at IS NULL
                UNION ALL
                SELECT b.id,
                       COALESCE(json_extract(a.properties, '$.name'), '') as name,
                       b.parent_id, b.lineage_id, b.position, b.deleted_at, b.created_at,
                       (ns.namespace || '::' || COALESCE(json_extract(a.properties, '$.name'), ''))
                FROM blocks b
                JOIN lineages l ON b.lineage_id = l.id
                JOIN atoms a ON l.current_id = a.id
                JOIN ns ON b.parent_id = ns.id
                WHERE b.deleted_at IS NULL AND l.deleted_at IS NULL
            )
            SELECT id, lineage_id, parent_id, name, position, deleted_at, created_at
            FROM ns
            WHERE name LIKE ?1 OR namespace LIKE ?1
            ORDER BY namespace ASC
            LIMIT 20
            "#,
        )
        .bind(&pattern)
        .fetch_all(&self.pool)
        .await
        .sq()?;

        Ok(rows.into_iter().map(Block::from).collect())
    }

    async fn list_blocks_by_content_type(&self, content_type: &str) -> Result<Vec<Block>> {
        let rows: Vec<BlockRow> = sqlx::query_as(&format!(
            r#"
            {} WHERE a.content_type = ?1
              AND b.deleted_at IS NULL
              AND l.deleted_at IS NULL
            ORDER BY COALESCE(json_extract(a.properties, '$.name'), '') ASC
            "#,
            BLOCK_SELECT
        ))
        .bind(content_type)
        .fetch_all(&self.pool)
        .await
        .sq()?;

        Ok(rows.into_iter().map(Block::from).collect())
    }

    async fn move_block(
        &self,
        block_id: Uuid,
        new_parent_id: Option<Uuid>,
        new_position: Option<String>,
    ) -> Result<Block> {
        let mut tx = self.pool.begin().await.sq()?;

        let _ = sq_fetch_block(&mut *tx, block_id).await?;
        let position = match new_position {
            Some(p) => p,
            None => sq_next_position(&mut *tx, new_parent_id).await?,
        };

        let result = sqlx::query(
            "UPDATE blocks SET parent_id = ?2, position = ?3 WHERE id = ?1 AND deleted_at IS NULL",
        )
        .bind(block_id.to_string())
        .bind(new_parent_id.map(|p| p.to_string()))
        .bind(&position)
        .execute(&mut *tx)
        .await
        .sq()?;

        if result.rows_affected() == 0 {
            return Err(Error::NotFound(format!(
                "Block {} not found or already deleted",
                block_id
            )));
        }

        let block = sq_fetch_block(&mut *tx, block_id).await?;
        tx.commit().await.sq()?;
        Ok(block)
    }

    async fn is_move_safe(&self, block_id: Uuid, new_parent_id: Option<Uuid>) -> Result<bool> {
        let Some(parent_id) = new_parent_id else {
            return Ok(true);
        };
        if parent_id == block_id {
            return Ok(false);
        }

        let is_ancestor: i32 = sqlx::query_scalar(
            r#"
            WITH RECURSIVE ancestors AS (
                SELECT id, parent_id FROM blocks WHERE id = ?1
                UNION ALL
                SELECT b.id, b.parent_id
                FROM blocks b
                JOIN ancestors a ON b.id = a.parent_id
            )
            SELECT CASE WHEN EXISTS (SELECT 1 FROM ancestors WHERE id = ?2) THEN 1 ELSE 0 END
            "#,
        )
        .bind(parent_id.to_string())
        .bind(block_id.to_string())
        .fetch_one(&self.pool)
        .await
        .sq()?;

        Ok(is_ancestor == 0)
    }

    async fn get_blocks_for_lineage(&self, lineage_id: Uuid) -> Result<Vec<Block>> {
        let rows: Vec<BlockRow> = sqlx::query_as(&format!(
            "{} WHERE b.lineage_id = ?1 AND b.deleted_at IS NULL ORDER BY b.created_at ASC",
            BLOCK_SELECT
        ))
        .bind(lineage_id.to_string())
        .fetch_all(&self.pool)
        .await
        .sq()?;

        Ok(rows.into_iter().map(Block::from).collect())
    }

    // -------------------------------------------------------------------------
    // New methods: property keys, hard link, content hash search
    // -------------------------------------------------------------------------

    async fn list_property_keys_in_subtree(&self, block_id: Uuid) -> Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            r#"
            WITH RECURSIVE subtree AS (
                SELECT id, lineage_id FROM blocks WHERE id = ?1 AND deleted_at IS NULL
                UNION ALL
                SELECT b.id, b.lineage_id FROM blocks b
                JOIN subtree s ON b.parent_id = s.id WHERE b.deleted_at IS NULL
            )
            SELECT DISTINCT je.key FROM subtree s
            JOIN lineages l ON s.lineage_id = l.id
            JOIN atoms a ON l.current_id = a.id,
            json_each(a.properties) AS je
            WHERE l.deleted_at IS NULL ORDER BY je.key
            "#,
        )
        .bind(block_id.to_string())
        .fetch_all(&self.pool)
        .await
        .sq()?;

        Ok(rows.into_iter().map(|(k,)| k).collect())
    }

    async fn create_block_for_lineage(
        &self,
        parent_id: Option<Uuid>,
        lineage_id: Uuid,
    ) -> Result<Block> {
        let mut tx = self.pool.begin().await.sq()?;

        let atom = sq_fetch_atom_tx(&mut tx, lineage_id).await?;
        let name = atom
            .properties
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if sq_find_by_parent_name(&mut *tx, parent_id, &name)
            .await?
            .is_some()
        {
            return Err(Error::Conflict(format!(
                "Block '{}' already exists under this parent",
                name
            )));
        }

        let block_id = Uuid::now_v7();
        let now = Utc::now();
        let position = sq_next_position(&mut *tx, parent_id).await?;
        let block_id_str = block_id.to_string();
        let now_str = now.to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO blocks (id, lineage_id, parent_id, position, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
        )
        .bind(&block_id_str)
        .bind(lineage_id.to_string())
        .bind(parent_id.map(|p| p.to_string()))
        .bind(&position)
        .bind(&now_str)
        .execute(&mut *tx)
        .await
        .sq()?;

        tx.commit().await.sq()?;

        Ok(Block {
            id: block_id,
            lineage_id,
            parent_id,
            name,
            position,
            deleted_at: None,
            created_at: now,
        })
    }

    async fn find_lineages_by_content_hash(&self, content_hash: &str) -> Result<Vec<Uuid>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT l.id FROM lineages l
            JOIN atoms a ON l.current_id = a.id
            WHERE a.content_hash = ?1 AND l.deleted_at IS NULL
            "#,
        )
        .bind(content_hash)
        .fetch_all(&self.pool)
        .await
        .sq()?;

        Ok(rows
            .into_iter()
            .map(|(s,)| Uuid::parse_str(&s).unwrap_or_default())
            .collect())
    }

    // -------------------------------------------------------------------------
    // Edge (SQL)
    // -------------------------------------------------------------------------

    async fn create_edge(&self, create: &CreateEdge) -> Result<Edge> {
        let mut tx = self.pool.begin().await.sq()?;

        let id = Uuid::now_v7();
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        let props = serde_json::to_string(&create.properties).unwrap_or_default();

        let existing: i32 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM edges
            WHERE from_lineage_id = ?1 AND to_lineage_id = ?2 AND edge_type = ?3 AND deleted_at IS NULL
            "#,
        )
        .bind(create.from_lineage_id.to_string())
        .bind(create.to_lineage_id.to_string())
        .bind(&create.edge_type)
        .fetch_one(&mut *tx)
        .await
        .sq()?;

        if existing > 0 {
            return Err(Error::Conflict(format!(
                "Edge of type '{}' already exists between {} and {}",
                create.edge_type, create.from_lineage_id, create.to_lineage_id
            )));
        }

        let row: EdgeRow = sqlx::query_as(
            r#"
            INSERT INTO edges (id, from_lineage_id, to_lineage_id, edge_type, properties, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            RETURNING *
            "#,
        )
        .bind(id.to_string())
        .bind(create.from_lineage_id.to_string())
        .bind(create.to_lineage_id.to_string())
        .bind(&create.edge_type)
        .bind(&props)
        .bind(&now_str)
        .fetch_one(&mut *tx)
        .await
        .sq()?;

        tx.commit().await.sq()?;
        Ok(row.into())
    }

    async fn get_edge(&self, id: Uuid) -> Result<Edge> {
        let row: Option<EdgeRow> =
            sqlx::query_as("SELECT * FROM edges WHERE id = ?1 AND deleted_at IS NULL")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await
                .sq()?;

        row.map(Edge::from)
            .ok_or_else(|| Error::NotFound(format!("Edge {} not found", id)))
    }

    async fn delete_edge(&self, id: Uuid) -> Result<Edge> {
        let now_str = Utc::now().to_rfc3339();
        let row: Option<EdgeRow> = sqlx::query_as(
            r#"
            UPDATE edges
            SET deleted_at = ?2
            WHERE id = ?1 AND deleted_at IS NULL
            RETURNING *
            "#,
        )
        .bind(id.to_string())
        .bind(&now_str)
        .fetch_optional(&self.pool)
        .await
        .sq()?;

        row.map(Edge::from)
            .ok_or_else(|| Error::NotFound(format!("Edge {} not found or already deleted", id)))
    }

    async fn get_edges_from(&self, lineage_id: Uuid) -> Result<Vec<Edge>> {
        let rows: Vec<EdgeRow> = sqlx::query_as(
            r#"
            SELECT * FROM edges
            WHERE from_lineage_id = ?1 AND deleted_at IS NULL
            ORDER BY edge_type ASC
            "#,
        )
        .bind(lineage_id.to_string())
        .fetch_all(&self.pool)
        .await
        .sq()?;

        Ok(rows.into_iter().map(Edge::from).collect())
    }

    async fn get_edges_to(&self, lineage_id: Uuid) -> Result<Vec<Edge>> {
        let rows: Vec<EdgeRow> = sqlx::query_as(
            r#"
            SELECT * FROM edges
            WHERE to_lineage_id = ?1 AND deleted_at IS NULL
            ORDER BY edge_type ASC
            "#,
        )
        .bind(lineage_id.to_string())
        .fetch_all(&self.pool)
        .await
        .sq()?;

        Ok(rows.into_iter().map(Edge::from).collect())
    }

    async fn get_all_edges(&self, lineage_id: Uuid) -> Result<Vec<Edge>> {
        let rows: Vec<EdgeRow> = sqlx::query_as(
            r#"
            SELECT * FROM edges
            WHERE (from_lineage_id = ?1 OR to_lineage_id = ?1) AND deleted_at IS NULL
            ORDER BY edge_type ASC
            "#,
        )
        .bind(lineage_id.to_string())
        .fetch_all(&self.pool)
        .await
        .sq()?;

        Ok(rows.into_iter().map(Edge::from).collect())
    }

    async fn get_edges_between(&self, lineage_ids: &[Uuid]) -> Result<Vec<Edge>> {
        if lineage_ids.is_empty() {
            return Ok(vec![]);
        }
        let placeholders: Vec<String> =
            (1..=lineage_ids.len()).map(|i| format!("?{}", i)).collect();
        let in_clause = placeholders.join(",");
        let n = lineage_ids.len();
        // Bind same set twice: once for from_lineage_id, once for to_lineage_id
        let from_placeholders: Vec<String> = (n + 1..=2 * n).map(|i| format!("?{}", i)).collect();
        let from_clause = from_placeholders.join(",");
        let sql = format!(
            "SELECT * FROM edges WHERE from_lineage_id IN ({}) AND to_lineage_id IN ({}) AND deleted_at IS NULL",
            in_clause, from_clause,
        );

        let mut query = sqlx::query_as::<_, EdgeRow>(&sql);
        for id in lineage_ids {
            query = query.bind(id.to_string());
        }
        for id in lineage_ids {
            query = query.bind(id.to_string());
        }

        let rows = query.fetch_all(&self.pool).await.sq()?;
        Ok(rows.into_iter().map(Edge::from).collect())
    }

    async fn get_content_links_between(&self, lineage_ids: &[Uuid]) -> Result<Vec<(Uuid, Uuid)>> {
        if lineage_ids.is_empty() {
            return Ok(vec![]);
        }
        let n = lineage_ids.len();
        let in1: Vec<String> = (1..=n).map(|i| format!("?{}", i)).collect();
        let in2: Vec<String> = (n + 1..=2 * n).map(|i| format!("?{}", i)).collect();
        let sql = format!(
            r#"SELECT DISTINCT l.id, al.lineage_id
            FROM lineages l
            JOIN atoms a ON l.current_id = a.id
            JOIN atom_links al ON al.atom_id = a.id
            WHERE l.id IN ({}) AND al.lineage_id IN ({}) AND l.deleted_at IS NULL"#,
            in1.join(","),
            in2.join(","),
        );

        let mut query = sqlx::query_as::<_, (String, String)>(&sql);
        for id in lineage_ids {
            query = query.bind(id.to_string());
        }
        for id in lineage_ids {
            query = query.bind(id.to_string());
        }

        let rows = query.fetch_all(&self.pool).await.sq()?;
        Ok(rows
            .into_iter()
            .filter_map(|(from, to)| {
                Some((Uuid::parse_str(&from).ok()?, Uuid::parse_str(&to).ok()?))
            })
            .collect())
    }

    // -------------------------------------------------------------------------
    // Graph / Link (SQL -- via atom_links junction table)
    // -------------------------------------------------------------------------

    async fn get_backlinks(&self, target_lineage_id: Uuid) -> Result<Vec<Backlink>> {
        let rows: Vec<BacklinkRow> = sqlx::query_as(
            r#"
            SELECT DISTINCT l.id AS lineage_id,
                   a.id AS atom_id, a.content_type, a.content_template,
                   a.properties, a.content_hash, a.predecessor_id,
                   a.created_at AS atom_created_at
            FROM atoms a
            JOIN lineages l ON l.current_id = a.id
            JOIN atom_links al ON al.atom_id = a.id
            WHERE al.lineage_id = ?1
              AND l.deleted_at IS NULL
            ORDER BY a.created_at DESC
            "#,
        )
        .bind(target_lineage_id.to_string())
        .fetch_all(&self.pool)
        .await
        .sq()?;

        let mut backlinks = Vec::with_capacity(rows.len());
        for r in rows {
            let links = load_atom_links(&self.pool, &r.atom_id).await?;
            backlinks.push(r.into_backlink(links));
        }
        Ok(backlinks)
    }

    async fn count_backlinks(&self, target_lineage_id: Uuid) -> Result<i64> {
        let count: i32 = sqlx::query_scalar(
            r#"
            SELECT COUNT(DISTINCT l.id) FROM atoms a
            JOIN lineages l ON l.current_id = a.id
            JOIN atom_links al ON al.atom_id = a.id
            WHERE al.lineage_id = ?1
              AND l.deleted_at IS NULL
            "#,
        )
        .bind(target_lineage_id.to_string())
        .fetch_one(&self.pool)
        .await
        .sq()?;

        Ok(count as i64)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_test_store() -> SqliteStore {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("connect to in-memory SQLite");

        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&pool)
            .await
            .expect("enable foreign keys");

        run_migrations(&pool).await.expect("run migrations");

        SqliteStore::new(pool)
    }

    #[tokio::test]
    async fn test_health_check() {
        let store = create_test_store().await;
        assert!(store.health_check().await.unwrap());
    }

    #[tokio::test]
    async fn test_create_and_get_atom() {
        let store = create_test_store().await;
        let link_target = Uuid::now_v7();

        let create = CreateAtom {
            content_type: "content".to_string(),
            content_template: "Hello {0}".to_string(),
            links: vec![link_target],
            properties: serde_json::json!({"key": "value"}),
        };

        let (atom, lineage) = store.create_atom(&create).await.unwrap();
        assert_eq!(atom.content_type, "content");
        assert_eq!(atom.links, vec![link_target]);
        assert_eq!(lineage.version, 1);

        let fetched = store.get_atom(lineage.id).await.unwrap();
        assert_eq!(fetched.id, atom.id);
        assert_eq!(fetched.links, vec![link_target]);
    }

    #[tokio::test]
    async fn test_create_block_and_children() {
        let store = create_test_store().await;
        let props = serde_json::json!({});

        let (root, _) = store
            .create_block_with_content(None, "root", "", &[], "", &props)
            .await
            .unwrap();

        let (child, _) = store
            .create_block_with_content(Some(root.id), "child", "content", &[], "content", &props)
            .await
            .unwrap();

        let children = store.get_block_children(root.id).await.unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].id, child.id);

        let roots = store.get_root_blocks().await.unwrap();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].id, root.id);
    }

    #[tokio::test]
    async fn test_compute_namespace() {
        let store = create_test_store().await;
        let props = serde_json::json!({});

        let (root, _) = store
            .create_block_with_content(None, "research", "", &[], "", &props)
            .await
            .unwrap();

        let (child, _) = store
            .create_block_with_content(Some(root.id), "ml", "", &[], "", &props)
            .await
            .unwrap();

        let ns = store.compute_namespace(child.id).await.unwrap();
        assert_eq!(ns, "research::ml");
    }

    #[tokio::test]
    async fn test_edit_lineage() {
        let store = create_test_store().await;
        let create = CreateAtom {
            content_type: "content".to_string(),
            content_template: "v1".to_string(),
            links: vec![],
            properties: serde_json::json!({}),
        };

        let (_, lineage) = store.create_atom(&create).await.unwrap();

        let (atom2, lineage2) = store
            .edit_lineage(lineage.id, "content", "v2", &[], &serde_json::json!({}))
            .await
            .unwrap();

        assert_eq!(atom2.content_template, "v2");
        assert_eq!(lineage2.version, 2);
        assert_eq!(atom2.predecessor_id, Some(lineage.current_id));
    }

    #[tokio::test]
    async fn test_delete_and_restore_block() {
        let store = create_test_store().await;
        let props = serde_json::json!({});

        let (block, _) = store
            .create_block_with_content(None, "test", "", &[], "", &props)
            .await
            .unwrap();

        let deleted = store.delete_block(block.id).await.unwrap();
        assert!(deleted.deleted_at.is_some());

        // Should not be found via normal get
        assert!(store.get_block(block.id).await.is_err());

        // Should be found with deleted
        let found = store.get_block_with_deleted(block.id).await.unwrap();
        assert!(found.deleted_at.is_some());

        let restored = store.restore_block(block.id).await.unwrap();
        assert!(restored.deleted_at.is_none());
    }

    #[tokio::test]
    async fn test_duplicate_block_conflict() {
        let store = create_test_store().await;
        let props = serde_json::json!({});

        store
            .create_block_with_content(None, "unique", "", &[], "", &props)
            .await
            .unwrap();

        let result = store
            .create_block_with_content(None, "unique", "", &[], "", &props)
            .await;

        assert!(matches!(result, Err(Error::Conflict(_))));
    }

    #[tokio::test]
    async fn test_backlinks() {
        let store = create_test_store().await;
        let props = serde_json::json!({});

        // Create target
        let (_, target_lineage) = store
            .create_atom(&CreateAtom {
                content_type: "".to_string(),
                content_template: "target".to_string(),
                links: vec![],
                properties: props.clone(),
            })
            .await
            .unwrap();

        // Create block that links to target
        let (_, _) = store
            .create_block_with_content(
                None,
                "linker",
                "see {0}",
                &[target_lineage.id],
                "content",
                &props,
            )
            .await
            .unwrap();

        let backlinks = store.get_backlinks(target_lineage.id).await.unwrap();
        assert_eq!(backlinks.len(), 1);

        let count = store.count_backlinks(target_lineage.id).await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_search_blocks() {
        let store = create_test_store().await;
        let props = serde_json::json!({});

        store
            .create_block_with_content(None, "research", "", &[], "", &props)
            .await
            .unwrap();

        let results = store.search_blocks("research").await.unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "research");
    }

    #[tokio::test]
    async fn test_edges() {
        let store = create_test_store().await;
        let props = serde_json::json!({});

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
                properties: props.clone(),
            })
            .await
            .unwrap();

        assert_eq!(edge.edge_type, "related");

        let from = store.get_edges_from(lin1.id).await.unwrap();
        assert_eq!(from.len(), 1);

        let to = store.get_edges_to(lin2.id).await.unwrap();
        assert_eq!(to.len(), 1);

        // Duplicate should fail
        let dup = store
            .create_edge(&CreateEdge {
                from_lineage_id: lin1.id,
                to_lineage_id: lin2.id,
                edge_type: "related".to_string(),
                properties: props,
            })
            .await;
        assert!(matches!(dup, Err(Error::Conflict(_))));
    }

    #[tokio::test]
    async fn test_recursive_delete() {
        let store = create_test_store().await;
        let props = serde_json::json!({});

        let (root, _) = store
            .create_block_with_content(None, "root", "", &[], "", &props)
            .await
            .unwrap();

        let (child, _) = store
            .create_block_with_content(Some(root.id), "child", "", &[], "", &props)
            .await
            .unwrap();

        let (_grandchild, _) = store
            .create_block_with_content(Some(child.id), "grandchild", "", &[], "", &props)
            .await
            .unwrap();

        let count = store.delete_block_recursive(root.id).await.unwrap();
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn test_is_move_safe() {
        let store = create_test_store().await;
        let props = serde_json::json!({});

        let (parent, _) = store
            .create_block_with_content(None, "parent", "", &[], "", &props)
            .await
            .unwrap();

        let (child, _) = store
            .create_block_with_content(Some(parent.id), "child", "", &[], "", &props)
            .await
            .unwrap();

        // Moving parent under child would create cycle
        assert!(!store.is_move_safe(parent.id, Some(child.id)).await.unwrap());

        // Moving to root is always safe
        assert!(store.is_move_safe(child.id, None).await.unwrap());

        // Moving to self is not safe
        assert!(
            !store
                .is_move_safe(parent.id, Some(parent.id))
                .await
                .unwrap()
        );
    }
}
