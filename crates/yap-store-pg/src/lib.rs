//! PostgreSQL backend for yap-orange.
//!
//! Implements `Store` for `PgStore`. All PG-specific SQL lives here:
//! UUID[], @> (GIN-eligible containment), JSONB, ILIKE, WITH RECURSIVE,
//! and PG error code 23505 → `Error::Conflict` mapping.
//!
//! After the name→properties migration, block names live in
//! `atoms.properties->>'name'`. Every block SELECT joins through
//! lineages→atoms to populate the name field.

mod rows;

use async_trait::async_trait;
use chrono::Utc;
use fractional_index::FractionalIndex;
use sqlx::PgPool;
use uuid::Uuid;

use yap_core::Store;
use yap_core::error::{Error, Result};
use yap_core::models::{Atom, Backlink, Block, CreateAtom, CreateEdge, Edge, Lineage, UpdateBlock};

use rows::{AtomRow, BacklinkRow, BlockRow, EdgeRow, LineageRow};

// =============================================================================
// Error mapping
// =============================================================================

/// Convert a sqlx error to a yap-core Error.
/// PG unique-violation (23505) becomes Error::Conflict.
#[allow(clippy::needless_pass_by_value)] // used as .map_err(map_err)
fn map_err(e: sqlx::Error) -> Error {
    match &e {
        sqlx::Error::Database(dbe) if dbe.code().as_deref() == Some("23505") => {
            Error::Conflict(dbe.message().to_string())
        }
        _ => Error::Database(e.to_string()),
    }
}

// Convenience extension trait so we can write `.pg()?` instead of `.map_err(map_err)?`
trait PgExt<T> {
    fn pg(self) -> Result<T>;
}

impl<T> PgExt<T> for std::result::Result<T, sqlx::Error> {
    fn pg(self) -> Result<T> {
        self.map_err(map_err)
    }
}

// =============================================================================
// Hash (from yap-core::hash)
// =============================================================================

fn content_hash(content_type: &str, template: &str, links: &[Uuid]) -> String {
    yap_core::hash::compute_content_hash(content_type, template, links)
}

// =============================================================================
// PgStore
// =============================================================================

/// PostgreSQL-backed implementation of `Store`.
#[derive(Debug, Clone)]
pub struct PgStore {
    pool: PgPool,
}

impl PgStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn connect(url: &str) -> Result<Self> {
        PgPool::connect(url)
            .await
            .map(Self::new)
            .map_err(|e| Error::Database(e.to_string()))
    }
}

/// Run all pending PostgreSQL migrations.
pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("../../migrations/postgres")
        .run(pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))
}

// =============================================================================
// Common SQL fragments
// =============================================================================

/// Block SELECT with JOIN to get name from atom properties.
/// Returns columns: id, lineage_id, parent_id, name, position, deleted_at, created_at
const BLOCK_SELECT: &str = r#"
    SELECT b.id, b.lineage_id, b.parent_id,
           COALESCE(a.properties->>'name', '') as name,
           b.position, b.deleted_at, b.created_at
    FROM blocks b
    JOIN lineages l ON b.lineage_id = l.id
    JOIN atoms a ON l.current_id = a.id
"#;

// =============================================================================
// Reusable query helpers — generic over Executor (pool or transaction)
// =============================================================================

async fn find_by_parent_name<'e, E>(
    executor: E,
    parent_id: Option<Uuid>,
    name: &str,
) -> Result<Option<Block>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row: Option<BlockRow> = match parent_id {
        Some(pid) => {
            sqlx::query_as(&format!(
                "{} WHERE b.parent_id = $1 AND a.properties->>'name' = $2 AND b.deleted_at IS NULL AND l.deleted_at IS NULL",
                BLOCK_SELECT
            ))
            .bind(pid)
            .bind(name)
            .fetch_optional(executor)
            .await
            .pg()?
        }
        None => {
            sqlx::query_as(&format!(
                "{} WHERE b.parent_id IS NULL AND a.properties->>'name' = $1 AND b.deleted_at IS NULL AND l.deleted_at IS NULL",
                BLOCK_SELECT
            ))
            .bind(name)
            .fetch_optional(executor)
            .await
            .pg()?
        }
    };
    Ok(row.map(Block::from))
}

async fn next_position<'e, E>(executor: E, parent_id: Option<Uuid>) -> Result<String>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let last_pos: Option<String> = match parent_id {
        Some(pid) => sqlx::query_scalar(
            r#"
                SELECT position FROM blocks
                WHERE parent_id = $1 AND deleted_at IS NULL
                ORDER BY position DESC
                LIMIT 1
                "#,
        )
        .bind(pid)
        .fetch_optional(executor)
        .await
        .pg()?,
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
        .pg()?,
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

async fn fetch_block<'e, E>(executor: E, id: Uuid) -> Result<Block>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row: Option<BlockRow> = sqlx::query_as(&format!(
        "{} WHERE b.id = $1 AND b.deleted_at IS NULL",
        BLOCK_SELECT
    ))
    .bind(id)
    .fetch_optional(executor)
    .await
    .pg()?;

    row.map(Block::from)
        .ok_or_else(|| Error::NotFound(format!("Block {} not found", id)))
}

async fn fetch_atom<'e, E>(executor: E, lineage_id: Uuid) -> Result<Atom>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row: Option<AtomRow> = sqlx::query_as(
        r#"
        SELECT a.* FROM atoms a
        JOIN lineages l ON l.current_id = a.id
        WHERE l.id = $1 AND l.deleted_at IS NULL
        "#,
    )
    .bind(lineage_id)
    .fetch_optional(executor)
    .await
    .pg()?;

    row.map(Atom::from)
        .ok_or_else(|| Error::NotFound(format!("Lineage {} not found", lineage_id)))
}

async fn fetch_lineage<'e, E>(executor: E, lineage_id: Uuid) -> Result<Lineage>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row: Option<LineageRow> =
        sqlx::query_as("SELECT * FROM lineages WHERE id = $1 AND deleted_at IS NULL")
            .bind(lineage_id)
            .fetch_optional(executor)
            .await
            .pg()?;

    row.map(Lineage::from)
        .ok_or_else(|| Error::NotFound(format!("Lineage {} not found", lineage_id)))
}

// =============================================================================
// Store implementation
// =============================================================================

#[async_trait]
impl Store for PgStore {
    // -------------------------------------------------------------------------
    // Health
    // -------------------------------------------------------------------------

    async fn health_check(&self) -> Result<bool> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map(|_| true)
            .pg()
    }

    // -------------------------------------------------------------------------
    // Admin
    // -------------------------------------------------------------------------

    async fn is_empty(&self) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM atoms")
            .fetch_one(&self.pool)
            .await
            .pg()?;
        Ok(count == 0)
    }

    async fn clear_all_data(&self) -> Result<()> {
        sqlx::query("TRUNCATE TABLE edges, blocks, lineages, atoms CASCADE")
            .execute(&self.pool)
            .await
            .pg()?;
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
                       COALESCE(a.properties->>'name', '') as name,
                       0 AS depth
                FROM blocks b
                JOIN lineages l ON b.lineage_id = l.id
                JOIN atoms a ON l.current_id = a.id
                WHERE b.id = $1
                UNION ALL
                SELECT b.id, b.parent_id,
                       COALESCE(a.properties->>'name', '') as name,
                       anc.depth + 1
                FROM blocks b
                JOIN lineages l ON b.lineage_id = l.id
                JOIN atoms a ON l.current_id = a.id
                JOIN ancestors anc ON b.id = anc.parent_id
            )
            SELECT name FROM ancestors ORDER BY depth DESC
            "#,
        )
        .bind(block_id)
        .fetch_all(&self.pool)
        .await
        .pg()?;

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
        find_by_parent_name(&self.pool, parent_id, name).await
    }

    async fn get_next_position(&self, parent_id: Option<Uuid>) -> Result<String> {
        next_position(&self.pool, parent_id).await
    }

    // -------------------------------------------------------------------------
    // Atom + Lineage (SQL)
    // -------------------------------------------------------------------------

    async fn create_atom(&self, create: &CreateAtom) -> Result<(Atom, Lineage)> {
        let mut tx = self.pool.begin().await.pg()?;

        let id = Uuid::now_v7();
        let now = Utc::now();
        let hash = content_hash(
            &create.content_type,
            &create.content_template,
            &create.links,
        );

        let atom: AtomRow = sqlx::query_as(
            r#"
            INSERT INTO atoms (id, content_type, content_template, links, properties, content_hash, predecessor_id, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, NULL, $7)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(&create.content_type)
        .bind(&create.content_template)
        .bind(&create.links)
        .bind(&create.properties)
        .bind(&hash)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .pg()?;

        let lineage: LineageRow = sqlx::query_as(
            r#"
            INSERT INTO lineages (id, current_id, version, updated_at)
            VALUES ($1, $2, 1, $3)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(id)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .pg()?;

        tx.commit().await.pg()?;
        Ok((atom.into(), lineage.into()))
    }

    async fn get_atom(&self, lineage_id: Uuid) -> Result<Atom> {
        fetch_atom(&self.pool, lineage_id).await
    }

    async fn get_atom_by_id(&self, atom_id: Uuid) -> Result<Atom> {
        let row: Option<AtomRow> = sqlx::query_as("SELECT * FROM atoms WHERE id = $1")
            .bind(atom_id)
            .fetch_optional(&self.pool)
            .await
            .pg()?;
        row.map(Atom::from)
            .ok_or_else(|| Error::NotFound(format!("Atom {} not found", atom_id)))
    }

    async fn get_lineage(&self, lineage_id: Uuid) -> Result<Lineage> {
        fetch_lineage(&self.pool, lineage_id).await
    }

    async fn get_lineage_with_deleted(&self, lineage_id: Uuid) -> Result<Lineage> {
        let row: Option<LineageRow> = sqlx::query_as("SELECT * FROM lineages WHERE id = $1")
            .bind(lineage_id)
            .fetch_optional(&self.pool)
            .await
            .pg()?;

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
        let mut tx = self.pool.begin().await.pg()?;

        let lineage = fetch_lineage(&mut *tx, lineage_id).await?;
        let new_atom_id = Uuid::now_v7();
        let now = Utc::now();
        let hash = content_hash(content_type, content_template, links);

        let atom: AtomRow = sqlx::query_as(
            r#"
            INSERT INTO atoms (id, content_type, content_template, links, properties, content_hash, predecessor_id, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            "#,
        )
        .bind(new_atom_id)
        .bind(content_type)
        .bind(content_template)
        .bind(links)
        .bind(properties)
        .bind(&hash)
        .bind(lineage.current_id)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .pg()?;

        let updated_lineage: LineageRow = sqlx::query_as(
            r#"
            UPDATE lineages
            SET current_id = $2, version = version + 1
            WHERE id = $1 AND deleted_at IS NULL
            RETURNING *
            "#,
        )
        .bind(lineage_id)
        .bind(new_atom_id)
        .fetch_one(&mut *tx)
        .await
        .pg()?;

        tx.commit().await.pg()?;
        Ok((atom.into(), updated_lineage.into()))
    }

    async fn delete_lineage(&self, lineage_id: Uuid) -> Result<Lineage> {
        let row: Option<LineageRow> = sqlx::query_as(
            r#"
            UPDATE lineages
            SET deleted_at = NOW()
            WHERE id = $1 AND deleted_at IS NULL
            RETURNING *
            "#,
        )
        .bind(lineage_id)
        .fetch_optional(&self.pool)
        .await
        .pg()?;

        row.map(Lineage::from).ok_or_else(|| {
            Error::NotFound(format!(
                "Lineage {} not found or already deleted",
                lineage_id
            ))
        })
    }

    // -------------------------------------------------------------------------
    // Block (SQL) — name lives in atoms.properties->>'name'
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
        let mut tx = self.pool.begin().await.pg()?;

        // Uniqueness check inside the transaction to prevent TOCTOU races
        if find_by_parent_name(&mut *tx, parent_id, name)
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
        let position = next_position(&mut *tx, parent_id).await?;
        let hash = content_hash(content_type, content_template, links);

        // Inject name into atom properties
        let mut props = properties.clone();
        if let Some(obj) = props.as_object_mut() {
            obj.insert(
                "name".to_string(),
                serde_json::Value::String(name.to_string()),
            );
        }

        let atom: AtomRow = sqlx::query_as(
            r#"
            INSERT INTO atoms (id, content_type, content_template, links, properties, content_hash, predecessor_id, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, NULL, $7)
            RETURNING *
            "#,
        )
        .bind(atom_id)
        .bind(content_type)
        .bind(content_template)
        .bind(links)
        .bind(&props)
        .bind(&hash)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .pg()?;

        sqlx::query_as::<_, LineageRow>(
            r#"
            INSERT INTO lineages (id, current_id, version, updated_at)
            VALUES ($1, $2, 1, $3)
            RETURNING *
            "#,
        )
        .bind(atom_id)
        .bind(atom_id)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .pg()?;

        sqlx::query(
            r#"
            INSERT INTO blocks (id, lineage_id, parent_id, position, created_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(block_id)
        .bind(atom_id)
        .bind(parent_id)
        .bind(&position)
        .bind(now)
        .execute(&mut *tx)
        .await
        .pg()?;

        tx.commit().await.pg()?;

        let block = Block {
            id: block_id,
            lineage_id: atom_id,
            parent_id,
            name: name.to_string(),
            position,
            deleted_at: None,
            created_at: now,
        };

        Ok((block, atom.into()))
    }

    async fn get_block(&self, id: Uuid) -> Result<Block> {
        fetch_block(&self.pool, id).await
    }

    async fn get_block_with_deleted(&self, id: Uuid) -> Result<Block> {
        let row: Option<BlockRow> = sqlx::query_as(&format!("{} WHERE b.id = $1", BLOCK_SELECT))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .pg()?;

        row.map(Block::from)
            .ok_or_else(|| Error::NotFound(format!("Block {} not found", id)))
    }

    async fn update_block(&self, id: Uuid, update: &UpdateBlock) -> Result<Block> {
        let mut tx = self.pool.begin().await.pg()?;

        let existing = fetch_block(&mut *tx, id).await?;

        // If name is being updated, edit the atom properties
        if let Some(new_name) = &update.name {
            let atom = fetch_atom(&mut *tx, existing.lineage_id).await?;
            let mut props = atom.properties.clone();
            if let Some(obj) = props.as_object_mut() {
                obj.insert(
                    "name".to_string(),
                    serde_json::Value::String(new_name.clone()),
                );
            }

            let lineage = fetch_lineage(&mut *tx, existing.lineage_id).await?;
            let new_atom_id = Uuid::now_v7();
            let now = Utc::now();
            let hash = content_hash(&atom.content_type, &atom.content_template, &atom.links);

            sqlx::query(
                r#"
                INSERT INTO atoms (id, content_type, content_template, links, properties, content_hash, predecessor_id, created_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                "#,
            )
            .bind(new_atom_id)
            .bind(&atom.content_type)
            .bind(&atom.content_template)
            .bind(&atom.links)
            .bind(&props)
            .bind(&hash)
            .bind(lineage.current_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .pg()?;

            sqlx::query(
                "UPDATE lineages SET current_id = $2, version = version + 1 WHERE id = $1 AND deleted_at IS NULL",
            )
            .bind(existing.lineage_id)
            .bind(new_atom_id)
            .execute(&mut *tx)
            .await
            .pg()?;
        }

        // Update position if provided
        if let Some(new_position) = &update.position {
            sqlx::query("UPDATE blocks SET position = $2 WHERE id = $1 AND deleted_at IS NULL")
                .bind(id)
                .bind(new_position)
                .execute(&mut *tx)
                .await
                .pg()?;
        }

        let block = fetch_block(&mut *tx, id).await?;
        tx.commit().await.pg()?;
        Ok(block)
    }

    async fn delete_block(&self, id: Uuid) -> Result<Block> {
        let result = sqlx::query(
            "UPDATE blocks SET deleted_at = NOW() WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .pg()?;

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

        let result = sqlx::query(
            r#"
            WITH RECURSIVE tree AS (
                SELECT id FROM blocks WHERE id = $1 AND deleted_at IS NULL
                UNION ALL
                SELECT b.id FROM blocks b
                JOIN tree t ON b.parent_id = t.id
                WHERE b.deleted_at IS NULL
            )
            UPDATE blocks SET deleted_at = NOW()
            WHERE id IN (SELECT id FROM tree)
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .pg()?;

        Ok(result.rows_affected())
    }

    async fn restore_block(&self, id: Uuid) -> Result<Block> {
        let result = sqlx::query(
            "UPDATE blocks SET deleted_at = NULL WHERE id = $1 AND deleted_at IS NOT NULL",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .pg()?;

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
                SELECT id FROM blocks WHERE id = $1 AND deleted_at IS NOT NULL
                UNION ALL
                SELECT b.id FROM blocks b
                JOIN tree t ON b.parent_id = t.id
                WHERE b.deleted_at IS NOT NULL
            )
            UPDATE blocks SET deleted_at = NULL
            WHERE id IN (SELECT id FROM tree)
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .pg()?;

        Ok(result.rows_affected())
    }

    async fn get_block_children(&self, parent_id: Uuid) -> Result<Vec<Block>> {
        let rows: Vec<BlockRow> = sqlx::query_as(&format!(
            "{} WHERE b.parent_id = $1 AND b.deleted_at IS NULL ORDER BY b.position ASC",
            BLOCK_SELECT
        ))
        .bind(parent_id)
        .fetch_all(&self.pool)
        .await
        .pg()?;

        Ok(rows.into_iter().map(Block::from).collect())
    }

    async fn get_root_blocks(&self) -> Result<Vec<Block>> {
        let rows: Vec<BlockRow> = sqlx::query_as(&format!(
            "{} WHERE b.parent_id IS NULL AND b.deleted_at IS NULL ORDER BY b.position ASC",
            BLOCK_SELECT
        ))
        .fetch_all(&self.pool)
        .await
        .pg()?;

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
                               COALESCE(a.properties->>'name', '') as name,
                               b.position, b.deleted_at, b.created_at
                        FROM blocks b
                        JOIN lineages l ON b.lineage_id = l.id
                        JOIN atoms a ON l.current_id = a.id
                        WHERE b.id = $1 AND b.deleted_at IS NULL
                        UNION ALL
                        SELECT b.id, b.lineage_id, b.parent_id,
                               COALESCE(a.properties->>'name', '') as name,
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
                .bind(b.id)
                .fetch_all(&self.pool)
                .await
                .pg()?;

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
            ORDER BY COALESCE(a.properties->>'name', '') ASC
            "#,
            BLOCK_SELECT
        ))
        .fetch_all(&self.pool)
        .await
        .pg()?;

        Ok(rows.into_iter().map(Block::from).collect())
    }

    async fn search_blocks(&self, query: &str) -> Result<Vec<Block>> {
        let pattern = format!("%{}%", query);
        let rows: Vec<BlockRow> = sqlx::query_as(
            r#"
            WITH RECURSIVE ns AS (
                SELECT b.id,
                       COALESCE(a.properties->>'name', '') as name,
                       b.parent_id, b.lineage_id, b.position, b.deleted_at, b.created_at,
                       COALESCE(a.properties->>'name', '') AS namespace
                FROM blocks b
                JOIN lineages l ON b.lineage_id = l.id
                JOIN atoms a ON l.current_id = a.id
                WHERE b.parent_id IS NULL AND b.deleted_at IS NULL AND l.deleted_at IS NULL
                UNION ALL
                SELECT b.id,
                       COALESCE(a.properties->>'name', '') as name,
                       b.parent_id, b.lineage_id, b.position, b.deleted_at, b.created_at,
                       (ns.namespace || '::' || COALESCE(a.properties->>'name', ''))
                FROM blocks b
                JOIN lineages l ON b.lineage_id = l.id
                JOIN atoms a ON l.current_id = a.id
                JOIN ns ON b.parent_id = ns.id
                WHERE b.deleted_at IS NULL AND l.deleted_at IS NULL
            )
            SELECT id, lineage_id, parent_id, name, position, deleted_at, created_at
            FROM ns
            WHERE name ILIKE $1 OR namespace ILIKE $1
            ORDER BY namespace ASC
            LIMIT 20
            "#,
        )
        .bind(&pattern)
        .fetch_all(&self.pool)
        .await
        .pg()?;

        Ok(rows.into_iter().map(Block::from).collect())
    }

    async fn list_blocks_by_content_type(&self, content_type: &str) -> Result<Vec<Block>> {
        let rows: Vec<BlockRow> = sqlx::query_as(&format!(
            r#"
            {} WHERE a.content_type = $1
              AND b.deleted_at IS NULL
              AND l.deleted_at IS NULL
            ORDER BY COALESCE(a.properties->>'name', '') ASC
            "#,
            BLOCK_SELECT
        ))
        .bind(content_type)
        .fetch_all(&self.pool)
        .await
        .pg()?;

        Ok(rows.into_iter().map(Block::from).collect())
    }

    async fn move_block(
        &self,
        block_id: Uuid,
        new_parent_id: Option<Uuid>,
        new_position: Option<String>,
    ) -> Result<Block> {
        let mut tx = self.pool.begin().await.pg()?;

        let _ = fetch_block(&mut *tx, block_id).await?;
        let position = match new_position {
            Some(p) => p,
            None => next_position(&mut *tx, new_parent_id).await?,
        };

        sqlx::query(
            "UPDATE blocks SET parent_id = $2, position = $3 WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(block_id)
        .bind(new_parent_id)
        .bind(&position)
        .execute(&mut *tx)
        .await
        .pg()?;

        let block = fetch_block(&mut *tx, block_id).await?;
        tx.commit().await.pg()?;
        Ok(block)
    }

    async fn is_move_safe(&self, block_id: Uuid, new_parent_id: Option<Uuid>) -> Result<bool> {
        let Some(parent_id) = new_parent_id else {
            return Ok(true);
        };
        if parent_id == block_id {
            return Ok(false);
        }

        let is_ancestor: bool = sqlx::query_scalar(
            r#"
            WITH RECURSIVE ancestors AS (
                SELECT id, parent_id FROM blocks WHERE id = $1
                UNION ALL
                SELECT b.id, b.parent_id
                FROM blocks b
                JOIN ancestors a ON b.id = a.parent_id
            )
            SELECT EXISTS (SELECT 1 FROM ancestors WHERE id = $2)
            "#,
        )
        .bind(parent_id)
        .bind(block_id)
        .fetch_one(&self.pool)
        .await
        .pg()?;

        Ok(!is_ancestor)
    }

    async fn get_blocks_for_lineage(&self, lineage_id: Uuid) -> Result<Vec<Block>> {
        let rows: Vec<BlockRow> = sqlx::query_as(&format!(
            "{} WHERE b.lineage_id = $1 AND b.deleted_at IS NULL ORDER BY b.created_at ASC",
            BLOCK_SELECT
        ))
        .bind(lineage_id)
        .fetch_all(&self.pool)
        .await
        .pg()?;

        Ok(rows.into_iter().map(Block::from).collect())
    }

    // -------------------------------------------------------------------------
    // New methods: property keys, hard link, content hash search
    // -------------------------------------------------------------------------

    async fn list_property_keys_in_subtree(&self, block_id: Uuid) -> Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            r#"
            WITH RECURSIVE subtree AS (
                SELECT id, lineage_id FROM blocks WHERE id = $1 AND deleted_at IS NULL
                UNION ALL
                SELECT b.id, b.lineage_id FROM blocks b
                JOIN subtree s ON b.parent_id = s.id WHERE b.deleted_at IS NULL
            )
            SELECT DISTINCT k FROM subtree s
            JOIN lineages l ON s.lineage_id = l.id
            JOIN atoms a ON l.current_id = a.id,
            LATERAL jsonb_object_keys(a.properties) AS k
            WHERE l.deleted_at IS NULL ORDER BY k
            "#,
        )
        .bind(block_id)
        .fetch_all(&self.pool)
        .await
        .pg()?;

        Ok(rows.into_iter().map(|(k,)| k).collect())
    }

    async fn create_block_for_lineage(
        &self,
        parent_id: Option<Uuid>,
        lineage_id: Uuid,
    ) -> Result<Block> {
        let mut tx = self.pool.begin().await.pg()?;

        // Verify lineage exists and get name from atom properties
        let atom = fetch_atom(&mut *tx, lineage_id).await?;
        let name = atom
            .properties
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Check uniqueness
        if find_by_parent_name(&mut *tx, parent_id, &name)
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
        let position = next_position(&mut *tx, parent_id).await?;

        sqlx::query(
            r#"
            INSERT INTO blocks (id, lineage_id, parent_id, position, created_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(block_id)
        .bind(lineage_id)
        .bind(parent_id)
        .bind(&position)
        .bind(now)
        .execute(&mut *tx)
        .await
        .pg()?;

        tx.commit().await.pg()?;

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
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT l.id FROM lineages l
            JOIN atoms a ON l.current_id = a.id
            WHERE a.content_hash = $1 AND l.deleted_at IS NULL
            "#,
        )
        .bind(content_hash)
        .fetch_all(&self.pool)
        .await
        .pg()?;

        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    // -------------------------------------------------------------------------
    // Edge (SQL)
    // -------------------------------------------------------------------------

    async fn create_edge(&self, create: &CreateEdge) -> Result<Edge> {
        let mut tx = self.pool.begin().await.pg()?;

        let id = Uuid::now_v7();
        let now = Utc::now();

        let existing: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM edges
            WHERE from_lineage_id = $1 AND to_lineage_id = $2 AND edge_type = $3 AND deleted_at IS NULL
            "#,
        )
        .bind(create.from_lineage_id)
        .bind(create.to_lineage_id)
        .bind(&create.edge_type)
        .fetch_one(&mut *tx)
        .await
        .pg()?;

        if existing > 0 {
            return Err(Error::Conflict(format!(
                "Edge of type '{}' already exists between {} and {}",
                create.edge_type, create.from_lineage_id, create.to_lineage_id
            )));
        }

        let row: EdgeRow = sqlx::query_as(
            r#"
            INSERT INTO edges (id, from_lineage_id, to_lineage_id, edge_type, properties, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(create.from_lineage_id)
        .bind(create.to_lineage_id)
        .bind(&create.edge_type)
        .bind(&create.properties)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .pg()?;

        tx.commit().await.pg()?;
        Ok(row.into())
    }

    async fn get_edge(&self, id: Uuid) -> Result<Edge> {
        let row: Option<EdgeRow> =
            sqlx::query_as("SELECT * FROM edges WHERE id = $1 AND deleted_at IS NULL")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .pg()?;

        row.map(Edge::from)
            .ok_or_else(|| Error::NotFound(format!("Edge {} not found", id)))
    }

    async fn delete_edge(&self, id: Uuid) -> Result<Edge> {
        let row: Option<EdgeRow> = sqlx::query_as(
            r#"
            UPDATE edges
            SET deleted_at = NOW()
            WHERE id = $1 AND deleted_at IS NULL
            RETURNING *
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .pg()?;

        row.map(Edge::from)
            .ok_or_else(|| Error::NotFound(format!("Edge {} not found or already deleted", id)))
    }

    async fn get_edges_from(&self, lineage_id: Uuid) -> Result<Vec<Edge>> {
        let rows: Vec<EdgeRow> = sqlx::query_as(
            r#"
            SELECT * FROM edges
            WHERE from_lineage_id = $1 AND deleted_at IS NULL
            ORDER BY edge_type ASC
            "#,
        )
        .bind(lineage_id)
        .fetch_all(&self.pool)
        .await
        .pg()?;

        Ok(rows.into_iter().map(Edge::from).collect())
    }

    async fn get_edges_to(&self, lineage_id: Uuid) -> Result<Vec<Edge>> {
        let rows: Vec<EdgeRow> = sqlx::query_as(
            r#"
            SELECT * FROM edges
            WHERE to_lineage_id = $1 AND deleted_at IS NULL
            ORDER BY edge_type ASC
            "#,
        )
        .bind(lineage_id)
        .fetch_all(&self.pool)
        .await
        .pg()?;

        Ok(rows.into_iter().map(Edge::from).collect())
    }

    async fn get_all_edges(&self, lineage_id: Uuid) -> Result<Vec<Edge>> {
        let rows: Vec<EdgeRow> = sqlx::query_as(
            r#"
            SELECT * FROM edges
            WHERE (from_lineage_id = $1 OR to_lineage_id = $1) AND deleted_at IS NULL
            ORDER BY edge_type ASC
            "#,
        )
        .bind(lineage_id)
        .fetch_all(&self.pool)
        .await
        .pg()?;

        Ok(rows.into_iter().map(Edge::from).collect())
    }

    async fn get_edges_between(&self, lineage_ids: &[Uuid]) -> Result<Vec<Edge>> {
        if lineage_ids.is_empty() {
            return Ok(vec![]);
        }
        let rows: Vec<EdgeRow> = sqlx::query_as(
            r#"
            SELECT * FROM edges
            WHERE from_lineage_id = ANY($1::uuid[])
              AND to_lineage_id = ANY($1::uuid[])
              AND deleted_at IS NULL
            "#,
        )
        .bind(lineage_ids)
        .fetch_all(&self.pool)
        .await
        .pg()?;

        Ok(rows.into_iter().map(Edge::from).collect())
    }

    async fn get_content_links_between(&self, lineage_ids: &[Uuid]) -> Result<Vec<(Uuid, Uuid)>> {
        if lineage_ids.is_empty() {
            return Ok(vec![]);
        }
        let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
            r#"
            SELECT DISTINCT l.id AS from_lineage_id, link_id AS to_lineage_id
            FROM lineages l
            JOIN atoms a ON l.current_id = a.id,
            LATERAL unnest(a.links) AS link_id
            WHERE l.id = ANY($1::uuid[])
              AND link_id = ANY($1::uuid[])
              AND l.deleted_at IS NULL
            "#,
        )
        .bind(lineage_ids)
        .fetch_all(&self.pool)
        .await
        .pg()?;

        Ok(rows)
    }

    // -------------------------------------------------------------------------
    // Graph / Link (SQL — GIN-eligible)
    // -------------------------------------------------------------------------

    /// Get lineages linking to a target lineage via the `links @> ARRAY[$1::uuid]` GIN operator.
    async fn get_backlinks(&self, target_lineage_id: Uuid) -> Result<Vec<Backlink>> {
        let rows: Vec<BacklinkRow> = sqlx::query_as(
            r#"
            SELECT DISTINCT l.id AS lineage_id,
                   a.id AS atom_id, a.content_type, a.content_template,
                   a.links, a.properties, a.content_hash,
                   a.predecessor_id, a.created_at AS atom_created_at
            FROM atoms a
            JOIN lineages l ON l.current_id = a.id
            WHERE a.links @> ARRAY[$1::uuid]
              AND l.deleted_at IS NULL
            ORDER BY a.created_at DESC
            "#,
        )
        .bind(target_lineage_id)
        .fetch_all(&self.pool)
        .await
        .pg()?;

        Ok(rows.into_iter().map(Backlink::from).collect())
    }

    async fn count_backlinks(&self, target_lineage_id: Uuid) -> Result<i64> {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(DISTINCT l.id) FROM atoms a
            JOIN lineages l ON l.current_id = a.id
            WHERE a.links @> ARRAY[$1::uuid]
              AND l.deleted_at IS NULL
            "#,
        )
        .bind(target_lineage_id)
        .fetch_one(&self.pool)
        .await
        .pg()
    }
}
