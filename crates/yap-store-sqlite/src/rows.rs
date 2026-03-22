//! Internal SQLx row types for SQLite.
//!
//! SQLite returns TEXT for UUIDs and timestamps, so all fields are String.
//! Each row type converts to the shared model types via `From` impls,
//! parsing UUIDs and timestamps in the process.
//!
//! Key difference from PgStore: AtomRow has **no `links` field** — links
//! are loaded separately from the `atom_links` junction table.

use chrono::{DateTime, NaiveDateTime, Utc};
use uuid::Uuid;

use yap_core::models::{Atom, Backlink, Block, Edge, Lineage};

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

// ─── Atom ────────────────────────────────────────────────────────────────────

/// SQLite atom row — links loaded separately from `atom_links`.
#[derive(sqlx::FromRow)]
pub(crate) struct AtomRow {
    pub id: String,
    pub content_type: String,
    pub content_template: String,
    pub properties: String,
    pub content_hash: String,
    pub predecessor_id: Option<String>,
    pub created_at: String,
}

impl AtomRow {
    /// Convert to model `Atom`, supplying the links loaded from `atom_links`.
    pub fn into_atom(self, links: Vec<Uuid>) -> Atom {
        Atom {
            id: parse_uuid(&self.id),
            content_type: self.content_type,
            content_template: self.content_template,
            links,
            properties: serde_json::from_str(&self.properties).unwrap_or_default(),
            content_hash: self.content_hash,
            predecessor_id: self.predecessor_id.as_deref().map(parse_uuid),
            created_at: parse_dt(&self.created_at),
        }
    }
}

/// SQLite backlink row — includes lineage_id alongside atom columns.
#[derive(sqlx::FromRow)]
pub(crate) struct BacklinkRow {
    pub lineage_id: String,
    pub atom_id: String,
    pub content_type: String,
    pub content_template: String,
    pub properties: String,
    pub content_hash: String,
    pub predecessor_id: Option<String>,
    pub atom_created_at: String,
}

impl BacklinkRow {
    /// Convert to model `Backlink`, supplying the links loaded from `atom_links`.
    pub fn into_backlink(self, links: Vec<Uuid>) -> Backlink {
        Backlink {
            lineage_id: parse_uuid(&self.lineage_id),
            atom: Atom {
                id: parse_uuid(&self.atom_id),
                content_type: self.content_type,
                content_template: self.content_template,
                links,
                properties: serde_json::from_str(&self.properties).unwrap_or_default(),
                content_hash: self.content_hash,
                predecessor_id: self.predecessor_id.as_deref().map(parse_uuid),
                created_at: parse_dt(&self.atom_created_at),
            },
        }
    }
}

// ─── Lineage ─────────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
pub(crate) struct LineageRow {
    pub id: String,
    pub current_id: String,
    pub version: i32,
    pub deleted_at: Option<String>,
    pub updated_at: String,
}

impl From<LineageRow> for Lineage {
    fn from(r: LineageRow) -> Self {
        Lineage {
            id: parse_uuid(&r.id),
            current_id: parse_uuid(&r.current_id),
            version: r.version,
            deleted_at: r.deleted_at.as_deref().map(parse_dt),
            updated_at: parse_dt(&r.updated_at),
        }
    }
}

// ─── Block ───────────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
pub(crate) struct BlockRow {
    pub id: String,
    pub lineage_id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub position: String,
    pub deleted_at: Option<String>,
    pub created_at: String,
}

impl From<BlockRow> for Block {
    fn from(r: BlockRow) -> Self {
        Block {
            id: parse_uuid(&r.id),
            lineage_id: parse_uuid(&r.lineage_id),
            parent_id: r.parent_id.as_deref().map(parse_uuid),
            name: r.name,
            position: r.position,
            deleted_at: r.deleted_at.as_deref().map(parse_dt),
            created_at: parse_dt(&r.created_at),
        }
    }
}

// ─── Edge ────────────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
pub(crate) struct EdgeRow {
    pub id: String,
    pub from_lineage_id: String,
    pub to_lineage_id: String,
    pub edge_type: String,
    pub properties: String,
    pub deleted_at: Option<String>,
    pub created_at: String,
}

impl From<EdgeRow> for Edge {
    fn from(r: EdgeRow) -> Self {
        Edge {
            id: parse_uuid(&r.id),
            from_lineage_id: parse_uuid(&r.from_lineage_id),
            to_lineage_id: parse_uuid(&r.to_lineage_id),
            edge_type: r.edge_type,
            properties: serde_json::from_str(&r.properties).unwrap_or_default(),
            deleted_at: r.deleted_at.as_deref().map(parse_dt),
            created_at: parse_dt(&r.created_at),
        }
    }
}
