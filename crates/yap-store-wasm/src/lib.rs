//! WASM SQLite backend for yap-orange.
//!
//! Implements `Store` for `WasmSqliteStore` using the raw `sqlite-wasm-rs` FFI
//! via the safe `WasmDb` wrapper. Uses the exact same SQL as `yap-store-sqlite`
//! but doesn't depend on `sqlx` or `tokio`.
//!
//! After the name→properties migration, block names live in
//! `atoms.properties->>'name'`. Every block SELECT joins through
//! lineages→atoms to populate the name field.

pub mod db;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use fractional_index::FractionalIndex;
use uuid::Uuid;

use yap_core::error::{Error, Result};
use yap_core::models::{Atom, Backlink, Block, CreateAtom, CreateEdge, Edge, Lineage, UpdateBlock};
use yap_core::Store;

use db::{Row, Value, WasmDb};

// =============================================================================
// Embedded migration SQL
// =============================================================================

const MIGRATIONS: &str = r#"
-- atoms: immutable content snapshots (append-only)
CREATE TABLE IF NOT EXISTS atoms (
    id TEXT PRIMARY KEY,
    content_type TEXT NOT NULL DEFAULT '',
    content_template TEXT NOT NULL DEFAULT '',
    properties TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(properties)),
    content_hash TEXT NOT NULL DEFAULT '',
    predecessor_id TEXT REFERENCES atoms(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- atom_links: junction table replacing UUID[] on atoms
CREATE TABLE IF NOT EXISTS atom_links (
    atom_id TEXT NOT NULL REFERENCES atoms(id),
    lineage_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    PRIMARY KEY (atom_id, position)
);
CREATE INDEX IF NOT EXISTS idx_atom_links_lineage ON atom_links(lineage_id);

-- lineages: mutable pointer to current atom snapshot (stable identity)
CREATE TABLE IF NOT EXISTS lineages (
    id TEXT PRIMARY KEY,
    current_id TEXT NOT NULL REFERENCES atoms(id),
    version INTEGER NOT NULL DEFAULT 1,
    deleted_at TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- blocks: hierarchy entries (directory entries) — name lives in atom properties
CREATE TABLE IF NOT EXISTS blocks (
    id TEXT PRIMARY KEY,
    lineage_id TEXT NOT NULL REFERENCES lineages(id),
    parent_id TEXT REFERENCES blocks(id),
    position TEXT NOT NULL DEFAULT '80',
    deleted_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- edges: non-hierarchical relationships
CREATE TABLE IF NOT EXISTS edges (
    id TEXT PRIMARY KEY,
    from_lineage_id TEXT NOT NULL REFERENCES lineages(id),
    to_lineage_id TEXT NOT NULL REFERENCES lineages(id),
    edge_type TEXT NOT NULL,
    properties TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(properties)),
    deleted_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes on atoms
CREATE INDEX IF NOT EXISTS idx_atoms_content_hash ON atoms(content_hash);
CREATE INDEX IF NOT EXISTS idx_atoms_predecessor ON atoms(predecessor_id);
CREATE INDEX IF NOT EXISTS idx_atoms_content_type ON atoms(content_type);

-- Indexes on lineages
CREATE INDEX IF NOT EXISTS idx_lineages_current_id ON lineages(current_id);

-- Indexes on blocks (partial indexes supported in SQLite)
CREATE INDEX IF NOT EXISTS idx_blocks_parent_id ON blocks(parent_id) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_blocks_lineage_id ON blocks(lineage_id) WHERE deleted_at IS NULL;

-- Indexes on edges
CREATE UNIQUE INDEX IF NOT EXISTS idx_edges_unique ON edges(from_lineage_id, to_lineage_id, edge_type) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_edges_to ON edges(to_lineage_id) WHERE deleted_at IS NULL;
"#;

// =============================================================================
// Helper functions
// =============================================================================

/// Parse a SQLite datetime TEXT into `DateTime<Utc>`.
///
/// Handles both `YYYY-MM-DD HH:MM:SS` (SQLite `datetime()` output)
/// and RFC 3339 (`YYYY-MM-DDTHH:MM:SS+00:00`) formats.
fn parse_dt(s: &str) -> DateTime<Utc> {
    // Try RFC 3339 first (what we bind on insert)
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return dt.with_timezone(&Utc);
    }
    // Fallback: SQLite datetime('now') format
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .map(|n| n.and_utc())
        .unwrap_or_else(|_| Utc::now())
}

fn parse_uuid(s: &str) -> Uuid {
    Uuid::parse_str(s).unwrap_or_default()
}

fn content_hash(content_type: &str, template: &str, links: &[Uuid]) -> String {
    yap_core::hash::compute_content_hash(content_type, template, links)
}

// =============================================================================
// Row mapping functions
// =============================================================================

/// Map an atom row (columns: id, content_type, content_template, properties,
/// content_hash, predecessor_id, created_at).
/// Note: links must be loaded separately from atom_links.
fn map_atom_row(r: &Row) -> Atom {
    map_atom_row_offset(r, 0)
}

/// Map atom columns starting at a given column offset.
fn map_atom_row_offset(r: &Row, off: usize) -> Atom {
    Atom {
        id: parse_uuid(r.get_text(off)),
        content_type: r.get_text(off + 1).to_string(),
        content_template: r.get_text(off + 2).to_string(),
        links: Vec::new(), // loaded separately
        properties: serde_json::from_str(r.get_text(off + 3)).unwrap_or_default(),
        content_hash: r.get_text(off + 4).to_string(),
        predecessor_id: r.get_opt_text(off + 5).map(parse_uuid),
        created_at: parse_dt(r.get_text(off + 6)),
    }
}

/// Map a lineage row (columns: id, current_id, version, deleted_at, updated_at).
fn map_lineage_row(r: &Row) -> Lineage {
    Lineage {
        id: parse_uuid(r.get_text(0)),
        current_id: parse_uuid(r.get_text(1)),
        version: r.get_int(2),
        deleted_at: r.get_opt_text(3).map(parse_dt),
        updated_at: parse_dt(r.get_text(4)),
    }
}

/// Map a block row (columns: id, lineage_id, parent_id, name, position, deleted_at, created_at).
/// Name comes from the JOIN with atoms via COALESCE(json_extract(a.properties, '$.name'), '').
fn map_block_row(r: &Row) -> Block {
    Block {
        id: parse_uuid(r.get_text(0)),
        lineage_id: parse_uuid(r.get_text(1)),
        parent_id: r.get_opt_text(2).map(parse_uuid),
        name: r.get_text(3).to_string(),
        position: r.get_text(4).to_string(),
        deleted_at: r.get_opt_text(5).map(parse_dt),
        created_at: parse_dt(r.get_text(6)),
    }
}

/// Map an edge row (columns: id, from_lineage_id, to_lineage_id, edge_type,
/// properties, deleted_at, created_at).
fn map_edge_row(r: &Row) -> Edge {
    Edge {
        id: parse_uuid(r.get_text(0)),
        from_lineage_id: parse_uuid(r.get_text(1)),
        to_lineage_id: parse_uuid(r.get_text(2)),
        edge_type: r.get_text(3).to_string(),
        properties: serde_json::from_str(r.get_text(4)).unwrap_or_default(),
        deleted_at: r.get_opt_text(5).map(parse_dt),
        created_at: parse_dt(r.get_text(6)),
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
// Atom links helpers
// =============================================================================

/// Load links for an atom from the junction table.
fn load_atom_links(db: &WasmDb, atom_id: &str) -> Result<Vec<Uuid>> {
    let rows = db.query_rows(
        "SELECT lineage_id FROM atom_links WHERE atom_id = ?1 ORDER BY position ASC",
        &[Value::Text(atom_id)],
        |r| parse_uuid(r.get_text(0)),
    )?;
    Ok(rows)
}

/// Insert links into the atom_links junction table.
fn insert_atom_links(db: &WasmDb, atom_id: &str, links: &[Uuid]) -> Result<()> {
    for (i, link) in links.iter().enumerate() {
        let link_str = link.to_string();
        db.execute(
            "INSERT INTO atom_links (atom_id, lineage_id, position) VALUES (?1, ?2, ?3)",
            &[
                Value::Text(atom_id),
                Value::Text(&link_str),
                Value::Int(i as i32),
            ],
        )?;
    }
    Ok(())
}

// =============================================================================
// Error mapping for UNIQUE constraint violations
// =============================================================================

/// Check if an error message indicates a UNIQUE constraint violation and
/// convert to Error::Conflict if so.
fn map_db_error(e: Error) -> Error {
    match &e {
        Error::Database(msg) if msg.contains("UNIQUE constraint") => {
            Error::Conflict(msg.clone())
        }
        _ => e,
    }
}

// =============================================================================
// WasmSqliteStore
// =============================================================================

/// WASM SQLite-backed implementation of `Store`.
pub struct WasmSqliteStore {
    db: WasmDb,
}

impl WasmSqliteStore {
    /// Create a new store wrapping an already-opened `WasmDb`.
    pub fn new(db: WasmDb) -> Self {
        Self { db }
    }

    /// Execute the embedded migration SQL to create all tables and indexes.
    /// Handles migration from old schema (blocks with name column) to new
    /// schema (name in atom properties).
    pub fn run_migrations(&self) -> Result<()> {
        // Check if old schema has name column on blocks (existing OPFS data)
        let has_name_col = self.db.query_scalar_int(
            "SELECT COUNT(*) FROM pragma_table_info('blocks') WHERE name = 'name'",
            &[],
        ).unwrap_or(0) > 0;

        if has_name_col {
            // Migrate names to atom properties
            self.db.exec(r#"
                UPDATE atoms SET properties = json_set(properties, '$.name', (
                  SELECT b.name FROM blocks b
                  JOIN lineages l ON b.lineage_id = l.id WHERE l.current_id = atoms.id
                )) WHERE EXISTS (
                  SELECT 1 FROM blocks b JOIN lineages l ON b.lineage_id = l.id
                  WHERE l.current_id = atoms.id
                ) AND json_extract(properties, '$.name') IS NULL;
            "#)?;

            // Rebuild blocks without name column
            self.db.exec(r#"
                CREATE TABLE IF NOT EXISTS blocks_new (
                    id TEXT PRIMARY KEY,
                    lineage_id TEXT NOT NULL REFERENCES lineages(id),
                    parent_id TEXT REFERENCES blocks_new(id),
                    position TEXT NOT NULL DEFAULT '80',
                    deleted_at TEXT,
                    created_at TEXT NOT NULL DEFAULT (datetime('now'))
                );
                INSERT OR IGNORE INTO blocks_new SELECT id, lineage_id, parent_id, position, deleted_at, created_at FROM blocks;
                DROP TABLE blocks;
                ALTER TABLE blocks_new RENAME TO blocks;
            "#)?;

            // Drop old name-based unique indexes (they were on the old table, now gone)
            // Re-create needed indexes will happen in MIGRATIONS below
        }

        self.db.exec(MIGRATIONS)
    }
}

// =============================================================================
// Store implementation
// =============================================================================

#[async_trait]
impl Store for WasmSqliteStore {
    // -------------------------------------------------------------------------
    // Health
    // -------------------------------------------------------------------------

    async fn health_check(&self) -> Result<bool> {
        self.db.query_scalar_int("SELECT 1", &[])?;
        Ok(true)
    }

    // -------------------------------------------------------------------------
    // Admin
    // -------------------------------------------------------------------------

    async fn is_empty(&self) -> Result<bool> {
        let count = self.db.query_scalar_int("SELECT COUNT(*) FROM atoms", &[])?;
        Ok(count == 0)
    }

    async fn clear_all_data(&self) -> Result<()> {
        // DELETE in FK-safe order (no TRUNCATE in SQLite).
        self.db.exec("DELETE FROM edges")?;
        self.db.exec("DELETE FROM blocks")?;
        self.db.exec("DELETE FROM lineages")?;
        self.db.exec("DELETE FROM atom_links")?;
        self.db.exec("DELETE FROM atoms")?;
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Namespace / Path helpers (SQL)
    // -------------------------------------------------------------------------

    async fn compute_namespace(&self, block_id: Uuid) -> Result<String> {
        let id_str = block_id.to_string();
        let names: Vec<String> = self.db.query_rows(
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
            &[Value::Text(&id_str)],
            |r| r.get_text(0).to_string(),
        )?;
        Ok(names.join("::"))
    }

    async fn find_block_by_parent_and_name(
        &self,
        parent_id: Option<Uuid>,
        name: &str,
    ) -> Result<Option<Block>> {
        let sql = format!(
            "{} WHERE {} AND json_extract(a.properties, '$.name') = ?{} AND b.deleted_at IS NULL AND l.deleted_at IS NULL",
            BLOCK_SELECT,
            if parent_id.is_some() { "b.parent_id = ?1" } else { "b.parent_id IS NULL" },
            if parent_id.is_some() { "2" } else { "1" },
        );

        match parent_id {
            Some(pid) => {
                let pid_str = pid.to_string();
                self.db.query_optional(
                    &sql,
                    &[Value::Text(&pid_str), Value::Text(name)],
                    map_block_row,
                )
            }
            None => {
                self.db.query_optional(
                    &sql,
                    &[Value::Text(name)],
                    map_block_row,
                )
            }
        }
    }

    async fn get_next_position(&self, parent_id: Option<Uuid>) -> Result<String> {
        let last_pos: Option<String> = match parent_id {
            Some(pid) => {
                let pid_str = pid.to_string();
                self.db.query_scalar_opt_text(
                    r#"
                    SELECT position FROM blocks
                    WHERE parent_id = ?1 AND deleted_at IS NULL
                    ORDER BY position DESC
                    LIMIT 1
                    "#,
                    &[Value::Text(&pid_str)],
                )?
            }
            None => {
                self.db.query_scalar_opt_text(
                    r#"
                    SELECT position FROM blocks
                    WHERE parent_id IS NULL AND deleted_at IS NULL
                    ORDER BY position DESC
                    LIMIT 1
                    "#,
                    &[],
                )?
            }
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

    // -------------------------------------------------------------------------
    // Atom + Lineage (SQL)
    // -------------------------------------------------------------------------

    async fn create_atom(&self, create: &CreateAtom) -> Result<(Atom, Lineage)> {
        let id = Uuid::now_v7();
        let now = Utc::now();
        let hash = content_hash(&create.content_type, &create.content_template, &create.links);
        let id_str = id.to_string();
        let now_str = now.to_rfc3339();
        let props = serde_json::to_string(&create.properties).unwrap_or_default();

        let mut atom = self.db.query_one(
            r#"
            INSERT INTO atoms (id, content_type, content_template, properties, content_hash, predecessor_id, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)
            RETURNING *
            "#,
            &[
                Value::Text(&id_str),
                Value::Text(&create.content_type),
                Value::Text(&create.content_template),
                Value::Text(&props),
                Value::Text(&hash),
                Value::Text(&now_str),
            ],
            map_atom_row,
        )?;

        insert_atom_links(&self.db, &id_str, &create.links)?;
        atom.links = create.links.clone();

        let lineage = self.db.query_one(
            r#"
            INSERT INTO lineages (id, current_id, version, updated_at)
            VALUES (?1, ?2, 1, ?3)
            RETURNING *
            "#,
            &[
                Value::Text(&id_str),
                Value::Text(&id_str),
                Value::Text(&now_str),
            ],
            map_lineage_row,
        )?;

        Ok((atom, lineage))
    }

    async fn get_atom(&self, lineage_id: Uuid) -> Result<Atom> {
        let lid_str = lineage_id.to_string();
        let row = self.db.query_optional(
            r#"
            SELECT a.* FROM atoms a
            JOIN lineages l ON l.current_id = a.id
            WHERE l.id = ?1 AND l.deleted_at IS NULL
            "#,
            &[Value::Text(&lid_str)],
            map_atom_row,
        )?;

        match row {
            Some(mut atom) => {
                atom.links = load_atom_links(&self.db, &atom.id.to_string())?;
                Ok(atom)
            }
            None => Err(Error::NotFound(format!("Lineage {} not found", lineage_id))),
        }
    }

    async fn get_atom_by_id(&self, atom_id: Uuid) -> Result<Atom> {
        let aid_str = atom_id.to_string();
        let row = self.db.query_optional(
            "SELECT * FROM atoms WHERE id = ?1",
            &[Value::Text(&aid_str)],
            map_atom_row,
        )?;
        match row {
            Some(mut atom) => {
                atom.links = load_atom_links(&self.db, &atom.id.to_string())?;
                Ok(atom)
            }
            None => Err(Error::NotFound(format!("Atom {} not found", atom_id))),
        }
    }

    async fn get_lineage(&self, lineage_id: Uuid) -> Result<Lineage> {
        let lid_str = lineage_id.to_string();
        let row = self.db.query_optional(
            "SELECT * FROM lineages WHERE id = ?1 AND deleted_at IS NULL",
            &[Value::Text(&lid_str)],
            map_lineage_row,
        )?;

        row.ok_or_else(|| Error::NotFound(format!("Lineage {} not found", lineage_id)))
    }

    async fn get_lineage_with_deleted(&self, lineage_id: Uuid) -> Result<Lineage> {
        let lid_str = lineage_id.to_string();
        let row = self.db.query_optional(
            "SELECT * FROM lineages WHERE id = ?1",
            &[Value::Text(&lid_str)],
            map_lineage_row,
        )?;

        row.ok_or_else(|| Error::NotFound(format!("Lineage {} not found", lineage_id)))
    }

    async fn edit_lineage(
        &self,
        lineage_id: Uuid,
        content_type: &str,
        content_template: &str,
        links: &[Uuid],
        properties: &serde_json::Value,
    ) -> Result<(Atom, Lineage)> {
        let lineage = self.get_lineage(lineage_id).await?;
        let new_atom_id = Uuid::now_v7();
        let now = Utc::now();
        let hash = content_hash(content_type, content_template, links);
        let new_id_str = new_atom_id.to_string();
        let now_str = now.to_rfc3339();
        let props = serde_json::to_string(properties).unwrap_or_default();
        let pred_str = lineage.current_id.to_string();
        let lid_str = lineage_id.to_string();

        let mut atom = self.db.query_one(
            r#"
            INSERT INTO atoms (id, content_type, content_template, properties, content_hash, predecessor_id, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            RETURNING *
            "#,
            &[
                Value::Text(&new_id_str),
                Value::Text(content_type),
                Value::Text(content_template),
                Value::Text(&props),
                Value::Text(&hash),
                Value::Text(&pred_str),
                Value::Text(&now_str),
            ],
            map_atom_row,
        )?;

        insert_atom_links(&self.db, &new_id_str, links)?;
        atom.links = links.to_vec();

        let updated_lineage = self.db.query_one(
            r#"
            UPDATE lineages
            SET current_id = ?2, version = version + 1, updated_at = ?3
            WHERE id = ?1 AND deleted_at IS NULL
            RETURNING *
            "#,
            &[
                Value::Text(&lid_str),
                Value::Text(&new_id_str),
                Value::Text(&now_str),
            ],
            map_lineage_row,
        )?;

        Ok((atom, updated_lineage))
    }

    async fn delete_lineage(&self, lineage_id: Uuid) -> Result<Lineage> {
        let lid_str = lineage_id.to_string();
        let now_str = Utc::now().to_rfc3339();

        let row = self.db.query_optional(
            r#"
            UPDATE lineages
            SET deleted_at = ?2
            WHERE id = ?1 AND deleted_at IS NULL
            RETURNING *
            "#,
            &[Value::Text(&lid_str), Value::Text(&now_str)],
            map_lineage_row,
        )?;

        row.ok_or_else(|| {
            Error::NotFound(format!(
                "Lineage {} not found or already deleted",
                lineage_id
            ))
        })
    }

    // -------------------------------------------------------------------------
    // Block (SQL) — name lives in atoms.properties->'name'
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
        // Check for duplicate
        if self
            .find_block_by_parent_and_name(parent_id, name)
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
        let position = self.get_next_position(parent_id).await?;
        let hash = content_hash(content_type, content_template, links);
        let atom_id_str = atom_id.to_string();
        let block_id_str = block_id.to_string();
        let now_str = now.to_rfc3339();

        // Inject name into atom properties
        let mut props = properties.clone();
        if let Some(obj) = props.as_object_mut() {
            obj.insert("name".to_string(), serde_json::Value::String(name.to_string()));
        }
        let props_str = serde_json::to_string(&props).unwrap_or_default();

        let parent_id_str = parent_id.map(|p| p.to_string());

        // Insert atom
        let mut atom = self.db.query_one(
            r#"
            INSERT INTO atoms (id, content_type, content_template, properties, content_hash, predecessor_id, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)
            RETURNING *
            "#,
            &[
                Value::Text(&atom_id_str),
                Value::Text(content_type),
                Value::Text(content_template),
                Value::Text(&props_str),
                Value::Text(&hash),
                Value::Text(&now_str),
            ],
            map_atom_row,
        )?;

        insert_atom_links(&self.db, &atom_id_str, links)?;
        atom.links = links.to_vec();

        // Insert lineage (lineage ID = atom ID)
        let _lineage = self.db.query_one(
            r#"
            INSERT INTO lineages (id, current_id, version, updated_at)
            VALUES (?1, ?2, 1, ?3)
            RETURNING *
            "#,
            &[
                Value::Text(&atom_id_str),
                Value::Text(&atom_id_str),
                Value::Text(&now_str),
            ],
            map_lineage_row,
        )?;

        // Insert block without name column
        self.db.execute(
            r#"
            INSERT INTO blocks (id, lineage_id, parent_id, position, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            &[
                Value::Text(&block_id_str),
                Value::Text(&atom_id_str),
                Value::from(parent_id_str.as_deref()),
                Value::Text(&position),
                Value::Text(&now_str),
            ],
        )?;

        let block = Block {
            id: block_id,
            lineage_id: atom_id,
            parent_id,
            name: name.to_string(),
            position,
            deleted_at: None,
            created_at: now,
        };

        Ok((block, atom))
    }

    async fn get_block(&self, id: Uuid) -> Result<Block> {
        let id_str = id.to_string();
        let sql = format!(
            "{} WHERE b.id = ?1 AND b.deleted_at IS NULL",
            BLOCK_SELECT
        );
        let row = self.db.query_optional(
            &sql,
            &[Value::Text(&id_str)],
            map_block_row,
        )?;

        row.ok_or_else(|| Error::NotFound(format!("Block {} not found", id)))
    }

    async fn get_block_with_deleted(&self, id: Uuid) -> Result<Block> {
        let id_str = id.to_string();
        let sql = format!(
            "{} WHERE b.id = ?1",
            BLOCK_SELECT
        );
        let row = self.db.query_optional(
            &sql,
            &[Value::Text(&id_str)],
            map_block_row,
        )?;

        row.ok_or_else(|| Error::NotFound(format!("Block {} not found", id)))
    }

    async fn update_block(&self, id: Uuid, update: &UpdateBlock) -> Result<Block> {
        let existing = self.get_block(id).await?;

        // If name is being updated, edit the atom properties
        if let Some(new_name) = &update.name {
            let atom = self.get_atom(existing.lineage_id).await?;
            let mut props = atom.properties.clone();
            if let Some(obj) = props.as_object_mut() {
                obj.insert("name".to_string(), serde_json::Value::String(new_name.clone()));
            }
            self.edit_lineage(
                existing.lineage_id,
                &atom.content_type,
                &atom.content_template,
                &atom.links,
                &props,
            )
            .await?;
        }

        // Update position if provided
        if let Some(new_position) = &update.position {
            let id_str = id.to_string();
            self.db.execute(
                "UPDATE blocks SET position = ?2 WHERE id = ?1 AND deleted_at IS NULL",
                &[Value::Text(&id_str), Value::Text(new_position)],
            )?;
        }

        self.get_block(id).await
    }

    async fn delete_block(&self, id: Uuid) -> Result<Block> {
        let id_str = id.to_string();
        let now_str = Utc::now().to_rfc3339();

        let rows_affected = self.db.execute(
            "UPDATE blocks SET deleted_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
            &[Value::Text(&id_str), Value::Text(&now_str)],
        )?;

        if rows_affected == 0 {
            return Err(Error::NotFound(format!(
                "Block {} not found or already deleted",
                id
            )));
        }

        self.get_block_with_deleted(id).await
    }

    async fn delete_block_recursive(&self, id: Uuid) -> Result<u64> {
        // Verify the block exists first
        let _ = self.get_block(id).await?;
        let id_str = id.to_string();
        let now_str = Utc::now().to_rfc3339();

        let rows_affected = self.db.execute(
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
            &[Value::Text(&id_str), Value::Text(&now_str)],
        )?;

        Ok(rows_affected)
    }

    async fn restore_block(&self, id: Uuid) -> Result<Block> {
        let id_str = id.to_string();

        let rows_affected = self.db.execute(
            "UPDATE blocks SET deleted_at = NULL WHERE id = ?1 AND deleted_at IS NOT NULL",
            &[Value::Text(&id_str)],
        )?;

        if rows_affected == 0 {
            return Err(Error::NotFound(format!(
                "Block {} not found or not deleted",
                id
            )));
        }

        self.get_block(id).await
    }

    async fn restore_block_recursive(&self, id: Uuid) -> Result<u64> {
        let id_str = id.to_string();

        let rows_affected = self.db.execute(
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
            &[Value::Text(&id_str)],
        )?;

        Ok(rows_affected)
    }

    async fn get_block_children(&self, parent_id: Uuid) -> Result<Vec<Block>> {
        let pid_str = parent_id.to_string();
        let sql = format!(
            "{} WHERE b.parent_id = ?1 AND b.deleted_at IS NULL ORDER BY b.position ASC",
            BLOCK_SELECT
        );

        self.db.query_rows(
            &sql,
            &[Value::Text(&pid_str)],
            map_block_row,
        )
    }

    async fn get_root_blocks(&self) -> Result<Vec<Block>> {
        let sql = format!(
            "{} WHERE b.parent_id IS NULL AND b.deleted_at IS NULL ORDER BY b.position ASC",
            BLOCK_SELECT
        );

        self.db.query_rows(
            &sql,
            &[],
            map_block_row,
        )
    }

    async fn list_blocks_by_namespace(&self, namespace_prefix: &str) -> Result<Vec<Block>> {
        let root = self.find_block_by_namespace(namespace_prefix).await?;
        match root {
            Some(b) => {
                let bid_str = b.id.to_string();
                self.db.query_rows(
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
                    &[Value::Text(&bid_str)],
                    map_block_row,
                )
            }
            None => Ok(Vec::new()),
        }
    }

    async fn list_orphaned_blocks(&self) -> Result<Vec<Block>> {
        let sql = format!(
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
        );

        self.db.query_rows(
            &sql,
            &[],
            map_block_row,
        )
    }

    async fn search_blocks(&self, query: &str) -> Result<Vec<Block>> {
        let pattern = format!("%{}%", query);

        self.db.query_rows(
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
            &[Value::Text(&pattern)],
            map_block_row,
        )
    }

    async fn list_blocks_by_content_type(&self, content_type: &str) -> Result<Vec<Block>> {
        let sql = format!(
            r#"
            {} WHERE a.content_type = ?1
              AND b.deleted_at IS NULL
              AND l.deleted_at IS NULL
            ORDER BY COALESCE(json_extract(a.properties, '$.name'), '') ASC
            "#,
            BLOCK_SELECT
        );

        self.db.query_rows(
            &sql,
            &[Value::Text(content_type)],
            map_block_row,
        )
    }

    async fn move_block(
        &self,
        block_id: Uuid,
        new_parent_id: Option<Uuid>,
        new_position: Option<String>,
    ) -> Result<Block> {
        let _ = self.get_block(block_id).await?;
        let position = match new_position {
            Some(p) => p,
            None => self.get_next_position(new_parent_id).await?,
        };
        let bid_str = block_id.to_string();
        let parent_str = new_parent_id.map(|p| p.to_string());

        self.db.execute(
            "UPDATE blocks SET parent_id = ?2, position = ?3 WHERE id = ?1 AND deleted_at IS NULL",
            &[
                Value::Text(&bid_str),
                Value::from(parent_str.as_deref()),
                Value::Text(&position),
            ],
        )?;

        self.get_block(block_id).await
    }

    async fn is_move_safe(&self, block_id: Uuid, new_parent_id: Option<Uuid>) -> Result<bool> {
        let Some(parent_id) = new_parent_id else {
            return Ok(true);
        };
        if parent_id == block_id {
            return Ok(false);
        }

        let pid_str = parent_id.to_string();
        let bid_str = block_id.to_string();

        let is_ancestor = self.db.query_scalar_int(
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
            &[Value::Text(&pid_str), Value::Text(&bid_str)],
        )?;

        Ok(is_ancestor == 0)
    }

    async fn get_blocks_for_lineage(&self, lineage_id: Uuid) -> Result<Vec<Block>> {
        let lid_str = lineage_id.to_string();
        let sql = format!(
            "{} WHERE b.lineage_id = ?1 AND b.deleted_at IS NULL ORDER BY b.created_at ASC",
            BLOCK_SELECT
        );

        self.db.query_rows(
            &sql,
            &[Value::Text(&lid_str)],
            map_block_row,
        )
    }

    // -------------------------------------------------------------------------
    // New methods: property keys, hard link, content hash search
    // -------------------------------------------------------------------------

    async fn list_property_keys_in_subtree(&self, block_id: Uuid) -> Result<Vec<String>> {
        let bid_str = block_id.to_string();

        self.db.query_rows(
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
            WHERE l.deleted_at IS NULL
            ORDER BY je.key
            "#,
            &[Value::Text(&bid_str)],
            |r| r.get_text(0).to_string(),
        )
    }

    async fn create_block_for_lineage(
        &self,
        parent_id: Option<Uuid>,
        lineage_id: Uuid,
    ) -> Result<Block> {
        // Verify lineage exists and get name from atom properties
        let atom = self.get_atom(lineage_id).await?;
        let name = atom
            .properties
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Check uniqueness
        if self
            .find_block_by_parent_and_name(parent_id, &name)
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
        let position = self.get_next_position(parent_id).await?;
        let block_id_str = block_id.to_string();
        let lineage_id_str = lineage_id.to_string();
        let now_str = now.to_rfc3339();
        let parent_id_str = parent_id.map(|p| p.to_string());

        self.db.execute(
            r#"
            INSERT INTO blocks (id, lineage_id, parent_id, position, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            &[
                Value::Text(&block_id_str),
                Value::Text(&lineage_id_str),
                Value::from(parent_id_str.as_deref()),
                Value::Text(&position),
                Value::Text(&now_str),
            ],
        )?;

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
        self.db.query_rows(
            r#"
            SELECT l.id FROM lineages l
            JOIN atoms a ON l.current_id = a.id
            WHERE a.content_hash = ?1 AND l.deleted_at IS NULL
            "#,
            &[Value::Text(content_hash)],
            |r| parse_uuid(r.get_text(0)),
        )
    }

    // -------------------------------------------------------------------------
    // Edge (SQL)
    // -------------------------------------------------------------------------

    async fn create_edge(&self, create: &CreateEdge) -> Result<Edge> {
        let id = Uuid::now_v7();
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        let props = serde_json::to_string(&create.properties).unwrap_or_default();
        let from_str = create.from_lineage_id.to_string();
        let to_str = create.to_lineage_id.to_string();
        let id_str = id.to_string();

        // Check for existing duplicate
        let existing = self.db.query_scalar_int(
            r#"
            SELECT COUNT(*) FROM edges
            WHERE from_lineage_id = ?1 AND to_lineage_id = ?2 AND edge_type = ?3 AND deleted_at IS NULL
            "#,
            &[
                Value::Text(&from_str),
                Value::Text(&to_str),
                Value::Text(&create.edge_type),
            ],
        )?;

        if existing > 0 {
            return Err(Error::Conflict(format!(
                "Edge of type '{}' already exists between {} and {}",
                create.edge_type, create.from_lineage_id, create.to_lineage_id
            )));
        }

        let edge = self.db.query_one(
            r#"
            INSERT INTO edges (id, from_lineage_id, to_lineage_id, edge_type, properties, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            RETURNING *
            "#,
            &[
                Value::Text(&id_str),
                Value::Text(&from_str),
                Value::Text(&to_str),
                Value::Text(&create.edge_type),
                Value::Text(&props),
                Value::Text(&now_str),
            ],
            map_edge_row,
        )?;

        Ok(edge)
    }

    async fn get_edge(&self, id: Uuid) -> Result<Edge> {
        let id_str = id.to_string();

        let row = self.db.query_optional(
            "SELECT * FROM edges WHERE id = ?1 AND deleted_at IS NULL",
            &[Value::Text(&id_str)],
            map_edge_row,
        )?;

        row.ok_or_else(|| Error::NotFound(format!("Edge {} not found", id)))
    }

    async fn delete_edge(&self, id: Uuid) -> Result<Edge> {
        let id_str = id.to_string();
        let now_str = Utc::now().to_rfc3339();

        let row = self.db.query_optional(
            r#"
            UPDATE edges
            SET deleted_at = ?2
            WHERE id = ?1 AND deleted_at IS NULL
            RETURNING *
            "#,
            &[Value::Text(&id_str), Value::Text(&now_str)],
            map_edge_row,
        )?;

        row.ok_or_else(|| {
            Error::NotFound(format!("Edge {} not found or already deleted", id))
        })
    }

    async fn get_edges_from(&self, lineage_id: Uuid) -> Result<Vec<Edge>> {
        let lid_str = lineage_id.to_string();

        self.db.query_rows(
            r#"
            SELECT * FROM edges
            WHERE from_lineage_id = ?1 AND deleted_at IS NULL
            ORDER BY edge_type ASC
            "#,
            &[Value::Text(&lid_str)],
            map_edge_row,
        )
    }

    async fn get_edges_to(&self, lineage_id: Uuid) -> Result<Vec<Edge>> {
        let lid_str = lineage_id.to_string();

        self.db.query_rows(
            r#"
            SELECT * FROM edges
            WHERE to_lineage_id = ?1 AND deleted_at IS NULL
            ORDER BY edge_type ASC
            "#,
            &[Value::Text(&lid_str)],
            map_edge_row,
        )
    }

    async fn get_all_edges(&self, lineage_id: Uuid) -> Result<Vec<Edge>> {
        let lid_str = lineage_id.to_string();

        self.db.query_rows(
            r#"
            SELECT * FROM edges
            WHERE (from_lineage_id = ?1 OR to_lineage_id = ?1) AND deleted_at IS NULL
            ORDER BY edge_type ASC
            "#,
            &[Value::Text(&lid_str)],
            map_edge_row,
        )
    }

    async fn get_edges_between(&self, lineage_ids: &[Uuid]) -> Result<Vec<Edge>> {
        if lineage_ids.is_empty() {
            return Ok(vec![]);
        }
        let strs: Vec<String> = lineage_ids.iter().map(|id| id.to_string()).collect();
        let placeholders: Vec<String> = (1..=strs.len()).map(|i| format!("?{}", i)).collect();
        let in_clause = placeholders.join(",");
        let n = strs.len();
        let from_placeholders: Vec<String> = (n + 1..=2 * n).map(|i| format!("?{}", i)).collect();
        let from_clause = from_placeholders.join(",");
        let sql = format!(
            "SELECT * FROM edges WHERE from_lineage_id IN ({}) AND to_lineage_id IN ({}) AND deleted_at IS NULL",
            in_clause, from_clause,
        );
        let mut params: Vec<Value> = Vec::with_capacity(2 * n);
        for s in &strs {
            params.push(Value::Text(s));
        }
        for s in &strs {
            params.push(Value::Text(s));
        }
        self.db.query_rows(&sql, &params, map_edge_row)
    }

    async fn get_content_links_between(&self, lineage_ids: &[Uuid]) -> Result<Vec<(Uuid, Uuid)>> {
        if lineage_ids.is_empty() {
            return Ok(vec![]);
        }
        let strs: Vec<String> = lineage_ids.iter().map(|id| id.to_string()).collect();
        let n = strs.len();
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
        let mut params: Vec<Value> = Vec::with_capacity(2 * n);
        for s in &strs {
            params.push(Value::Text(s));
        }
        for s in &strs {
            params.push(Value::Text(s));
        }
        self.db.query_rows(&sql, &params, |r| {
            let from = parse_uuid(r.get_text(0));
            let to = parse_uuid(r.get_text(1));
            (from, to)
        })
    }

    // -------------------------------------------------------------------------
    // Graph / Link (SQL — via atom_links junction table)
    // -------------------------------------------------------------------------

    async fn get_backlinks(&self, target_lineage_id: Uuid) -> Result<Vec<Backlink>> {
        let tlid_str = target_lineage_id.to_string();

        // Query lineage ID alongside atom columns
        let rows: Vec<(Uuid, Atom)> = self.db.query_rows(
            r#"
            SELECT DISTINCT l.id, a.*
            FROM atoms a
            JOIN lineages l ON l.current_id = a.id
            JOIN atom_links al ON al.atom_id = a.id
            WHERE al.lineage_id = ?1
              AND l.deleted_at IS NULL
            ORDER BY a.created_at DESC
            "#,
            &[Value::Text(&tlid_str)],
            |r| {
                let lineage_id = parse_uuid(r.get_text(0));
                // Atom columns start at index 1
                let atom = map_atom_row_offset(r, 1);
                (lineage_id, atom)
            },
        )?;

        // Load links for each atom
        let mut backlinks = Vec::with_capacity(rows.len());
        for (lineage_id, mut atom) in rows {
            atom.links = load_atom_links(&self.db, &atom.id.to_string())?;
            backlinks.push(Backlink { lineage_id, atom });
        }
        Ok(backlinks)
    }

    async fn count_backlinks(&self, target_lineage_id: Uuid) -> Result<i64> {
        let tlid_str = target_lineage_id.to_string();

        let count = self.db.query_scalar_int(
            r#"
            SELECT COUNT(DISTINCT l.id) FROM atoms a
            JOIN lineages l ON l.current_id = a.id
            JOIN atom_links al ON al.atom_id = a.id
            WHERE al.lineage_id = ?1
              AND l.deleted_at IS NULL
            "#,
            &[Value::Text(&tlid_str)],
        )?;

        Ok(count as i64)
    }
}
