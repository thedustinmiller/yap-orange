//! Internal SQLx row types for PostgreSQL.
//!
//! These types derive `FromRow` and map directly to PostgreSQL column types.
//! They convert to the shared model types via `From` impls.
//!
//! PG-specific types handled here:
//! - `uuid[]` → `Vec<Uuid>`
//! - `jsonb`  → `serde_json::Value`

use chrono::{DateTime, Utc};
use uuid::Uuid;

use yap_core::models::{Atom, Backlink, Block, Edge, Lineage};

#[derive(sqlx::FromRow)]
pub(crate) struct AtomRow {
    pub id: Uuid,
    pub content_type: String,
    pub content_template: String,
    pub links: Vec<Uuid>,
    pub properties: serde_json::Value,
    pub content_hash: String,
    pub predecessor_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

impl From<AtomRow> for Atom {
    fn from(r: AtomRow) -> Self {
        Atom {
            id: r.id,
            content_type: r.content_type,
            content_template: r.content_template,
            links: r.links,
            properties: r.properties,
            content_hash: r.content_hash,
            predecessor_id: r.predecessor_id,
            created_at: r.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
pub(crate) struct LineageRow {
    pub id: Uuid,
    pub current_id: Uuid,
    pub version: i32,
    pub deleted_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

impl From<LineageRow> for Lineage {
    fn from(r: LineageRow) -> Self {
        Lineage {
            id: r.id,
            current_id: r.current_id,
            version: r.version,
            deleted_at: r.deleted_at,
            updated_at: r.updated_at,
        }
    }
}

/// Row type for backlink queries that include the lineage ID alongside atom columns.
#[derive(sqlx::FromRow)]
pub(crate) struct BacklinkRow {
    pub lineage_id: Uuid,
    pub atom_id: Uuid,
    pub content_type: String,
    pub content_template: String,
    pub links: Vec<Uuid>,
    pub properties: serde_json::Value,
    pub content_hash: String,
    pub predecessor_id: Option<Uuid>,
    pub atom_created_at: DateTime<Utc>,
}

impl From<BacklinkRow> for Backlink {
    fn from(r: BacklinkRow) -> Self {
        Backlink {
            lineage_id: r.lineage_id,
            atom: Atom {
                id: r.atom_id,
                content_type: r.content_type,
                content_template: r.content_template,
                links: r.links,
                properties: r.properties,
                content_hash: r.content_hash,
                predecessor_id: r.predecessor_id,
                created_at: r.atom_created_at,
            },
        }
    }
}

#[derive(sqlx::FromRow)]
pub(crate) struct BlockRow {
    pub id: Uuid,
    pub lineage_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub position: String,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<BlockRow> for Block {
    fn from(r: BlockRow) -> Self {
        Block {
            id: r.id,
            lineage_id: r.lineage_id,
            parent_id: r.parent_id,
            name: r.name,
            position: r.position,
            deleted_at: r.deleted_at,
            created_at: r.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
pub(crate) struct EdgeRow {
    pub id: Uuid,
    pub from_lineage_id: Uuid,
    pub to_lineage_id: Uuid,
    pub edge_type: String,
    pub properties: serde_json::Value,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<EdgeRow> for Edge {
    fn from(r: EdgeRow) -> Self {
        Edge {
            id: r.id,
            from_lineage_id: r.from_lineage_id,
            to_lineage_id: r.to_lineage_id,
            edge_type: r.edge_type,
            properties: r.properties,
            deleted_at: r.deleted_at,
            created_at: r.created_at,
        }
    }
}
