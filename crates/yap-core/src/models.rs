//! Data models for atoms, blocks, edges, and lineages

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Atom - Immutable content snapshot (like a filesystem inode version)
///
/// Atoms are append-only. Each edit creates a new atom with predecessor_id
/// pointing to the previous version. The lineage table tracks the "current" pointer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Atom {
    pub id: Uuid,
    pub content_type: String,
    pub content_template: String,
    pub links: Vec<Uuid>,
    pub properties: serde_json::Value,
    pub content_hash: String,
    pub predecessor_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Lineage - Mutable pointer to the current atom snapshot (stable identity)
///
/// The lineage ID equals the first atom's UUID, so existing IDs survive.
/// Soft delete lives here, not on atoms (which are immutable).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lineage {
    pub id: Uuid,
    pub current_id: Uuid,
    pub version: i32,
    pub deleted_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

/// Block - References to lineages in the hierarchy (like filesystem directory entries)
///
/// Uses parent_id for hierarchy. NULL parent_id = root block.
/// Position is a fractional index string for lexicographic ordering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub id: Uuid,
    pub lineage_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub position: String,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Edge - Non-hierarchical relationships between lineages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: Uuid,
    pub from_lineage_id: Uuid,
    pub to_lineage_id: Uuid,
    pub edge_type: String,
    pub properties: serde_json::Value,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// A backlink result: the linking lineage's ID paired with its current atom.
///
/// `get_backlinks` returns these so callers get the stable lineage ID
/// (not the atom snapshot ID, which changes on every edit).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backlink {
    pub lineage_id: Uuid,
    pub atom: Atom,
}

// DTOs for creating

/// DTO for creating a new atom (used internally)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAtom {
    pub content_type: String,
    pub content_template: String,
    #[serde(default)]
    pub links: Vec<Uuid>,
    #[serde(default = "default_properties")]
    pub properties: serde_json::Value,
}

/// DTO for creating a new block (with atom)
///
/// Takes a parent display path (e.g. "research::ml") and a name.
/// The system resolves the parent path and creates the block under it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBlock {
    pub namespace: String,
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub content_type: String,
    #[serde(default = "default_properties")]
    pub properties: serde_json::Value,
}

/// DTO for updating block metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateBlock {
    pub name: Option<String>,
    pub position: Option<String>,
}

/// DTO for creating an edge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEdge {
    pub from_lineage_id: Uuid,
    pub to_lineage_id: Uuid,
    pub edge_type: String,
    #[serde(default = "default_properties")]
    pub properties: serde_json::Value,
}

fn default_properties() -> serde_json::Value {
    serde_json::json!({})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uuidv7_generation() {
        let id1 = Uuid::now_v7();
        let id2 = Uuid::now_v7();

        // UUIDv7 should be time-sortable
        assert!(id2 > id1, "Second UUID should be greater than first");
    }

    #[test]
    fn test_uuidv7_ordering() {
        let mut ids: Vec<Uuid> = (0..10).map(|_| Uuid::now_v7()).collect();
        let original = ids.clone();
        ids.sort();

        // Sorted order should match creation order for UUIDv7
        assert_eq!(
            ids, original,
            "UUIDv7 should maintain creation order when sorted"
        );
    }

    #[test]
    fn test_atom_serialization() {
        let atom = Atom {
            id: Uuid::now_v7(),
            content_type: "content".to_string(),
            content_template: "Hello {0}".to_string(),
            links: vec![Uuid::now_v7()],
            properties: serde_json::json!({"key": "value"}),
            content_hash: "abc123".to_string(),
            predecessor_id: None,
            created_at: Utc::now(),
        };

        let json = serde_json::to_string(&atom).expect("Failed to serialize atom");
        let deserialized: Atom = serde_json::from_str(&json).expect("Failed to deserialize atom");

        assert_eq!(atom.id, deserialized.id);
        assert_eq!(atom.content_template, deserialized.content_template);
    }

    // =========================================================================
    // DTO serialization / deserialization
    // =========================================================================

    #[test]
    fn test_create_atom_default_properties() {
        let json = r#"{"content_type":"content","content_template":"Hello","links":[]}"#;
        let dto: CreateAtom = serde_json::from_str(json).expect("Failed to deserialize CreateAtom");
        // properties should default to {} when missing
        assert_eq!(dto.properties, serde_json::json!({}));
        assert_eq!(dto.content_type, "content");
        assert_eq!(dto.content_template, "Hello");
    }

    #[test]
    fn test_create_atom_with_links() {
        let id = Uuid::now_v7();
        let json = serde_json::json!({
            "content_type": "content",
            "content_template": "See {0}",
            "links": [id],
        });
        let dto: CreateAtom = serde_json::from_value(json).expect("Failed to deserialize");
        assert_eq!(dto.links, vec![id]);
    }

    #[test]
    fn test_update_block_partial_fields() {
        // Only name set — position should be None
        let json = r#"{"name": "new-name"}"#;
        let dto: UpdateBlock =
            serde_json::from_str(json).expect("Failed to deserialize UpdateBlock");
        assert_eq!(dto.name, Some("new-name".to_string()));
        assert!(dto.position.is_none());
    }

    #[test]
    fn test_create_edge_roundtrip() {
        let from = Uuid::now_v7();
        let to = Uuid::now_v7();
        let dto = CreateEdge {
            from_lineage_id: from,
            to_lineage_id: to,
            edge_type: "inspired-by".to_string(),
            properties: serde_json::json!({"weight": 1}),
        };

        let json = serde_json::to_string(&dto).expect("serialize");
        let back: CreateEdge = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.from_lineage_id, from);
        assert_eq!(back.to_lineage_id, to);
        assert_eq!(back.edge_type, "inspired-by");
        assert_eq!(back.properties["weight"], 1);
    }

    #[test]
    fn test_create_edge_default_properties() {
        let from = Uuid::now_v7();
        let to = Uuid::now_v7();
        let json = serde_json::json!({
            "from_lineage_id": from,
            "to_lineage_id": to,
            "edge_type": "references"
        });
        let dto: CreateEdge = serde_json::from_value(json).expect("Failed to deserialize");
        assert_eq!(dto.properties, serde_json::json!({}));
    }
}

#[cfg(test)]
mod proptest_models {
    use super::*;
    use proptest::prelude::*;

    fn arb_json_value(depth: u32) -> impl Strategy<Value = serde_json::Value> {
        if depth == 0 {
            prop_oneof![
                Just(serde_json::Value::Null),
                any::<bool>().prop_map(serde_json::Value::Bool),
                any::<i32>().prop_map(|n| serde_json::json!(n)),
                "[a-zA-Z0-9_ ]{0,20}".prop_map(serde_json::Value::String),
            ]
            .boxed()
        } else {
            prop_oneof![
                Just(serde_json::Value::Null),
                any::<bool>().prop_map(serde_json::Value::Bool),
                any::<i32>().prop_map(|n| serde_json::json!(n)),
                "[a-zA-Z0-9_ ]{0,20}".prop_map(serde_json::Value::String),
                prop::collection::vec(arb_json_value(depth - 1), 0..=3)
                    .prop_map(serde_json::Value::Array),
                prop::collection::vec(("[a-z]{1,8}", arb_json_value(depth - 1)), 0..=3,).prop_map(
                    |pairs| {
                        let map: serde_json::Map<String, serde_json::Value> =
                            pairs.into_iter().collect();
                        serde_json::Value::Object(map)
                    }
                ),
            ]
            .boxed()
        }
    }

    fn arb_atom() -> impl Strategy<Value = Atom> {
        (
            "[a-z]{2,10}",                                                         // content_type
            "[a-zA-Z0-9 ]{0,50}", // content_template
            prop::collection::vec(any::<u128>().prop_map(Uuid::from_u128), 0..=3), // links
            arb_json_value(2),    // properties
            "[a-f0-9]{8,16}",     // content_hash
            any::<bool>(),        // has_predecessor
        )
            .prop_map(
                |(content_type, content_template, links, properties, content_hash, has_pred)| {
                    let id = Uuid::now_v7();
                    Atom {
                        id,
                        content_type,
                        content_template,
                        links,
                        properties,
                        content_hash,
                        predecessor_id: if has_pred { Some(Uuid::now_v7()) } else { None },
                        created_at: Utc::now(),
                    }
                },
            )
    }

    fn arb_block() -> impl Strategy<Value = Block> {
        (
            "[a-zA-Z0-9_]{1,20}", // name
            "[a-z0-9]{4,12}",     // position
            any::<bool>(),        // has_parent
            any::<bool>(),        // is_deleted
        )
            .prop_map(|(name, position, has_parent, is_deleted)| Block {
                id: Uuid::now_v7(),
                lineage_id: Uuid::now_v7(),
                parent_id: if has_parent {
                    Some(Uuid::now_v7())
                } else {
                    None
                },
                name,
                position,
                deleted_at: if is_deleted { Some(Utc::now()) } else { None },
                created_at: Utc::now(),
            })
    }

    fn arb_edge() -> impl Strategy<Value = Edge> {
        (
            "[a-z_]{2,15}",    // edge_type
            arb_json_value(1), // properties
            any::<bool>(),     // is_deleted
        )
            .prop_map(|(edge_type, properties, is_deleted)| Edge {
                id: Uuid::now_v7(),
                from_lineage_id: Uuid::now_v7(),
                to_lineage_id: Uuid::now_v7(),
                edge_type,
                properties,
                deleted_at: if is_deleted { Some(Utc::now()) } else { None },
                created_at: Utc::now(),
            })
    }

    proptest! {
        #[test]
        fn atom_json_roundtrip(atom in arb_atom()) {
            let json = serde_json::to_string(&atom).expect("Failed to serialize Atom");
            let deserialized: Atom = serde_json::from_str(&json).expect("Failed to deserialize Atom");

            prop_assert_eq!(atom.id, deserialized.id);
            prop_assert_eq!(&atom.content_type, &deserialized.content_type);
            prop_assert_eq!(&atom.content_template, &deserialized.content_template);
            prop_assert_eq!(&atom.links, &deserialized.links);
            prop_assert_eq!(&atom.properties, &deserialized.properties);
            prop_assert_eq!(&atom.content_hash, &deserialized.content_hash);
            prop_assert_eq!(atom.predecessor_id, deserialized.predecessor_id);
            prop_assert_eq!(atom.created_at, deserialized.created_at);
        }

        #[test]
        fn block_json_roundtrip(block in arb_block()) {
            let json = serde_json::to_string(&block).expect("Failed to serialize Block");
            let deserialized: Block = serde_json::from_str(&json).expect("Failed to deserialize Block");

            prop_assert_eq!(block.id, deserialized.id);
            prop_assert_eq!(block.lineage_id, deserialized.lineage_id);
            prop_assert_eq!(block.parent_id, deserialized.parent_id);
            prop_assert_eq!(&block.name, &deserialized.name);
            prop_assert_eq!(&block.position, &deserialized.position);
            prop_assert_eq!(block.deleted_at, deserialized.deleted_at);
            prop_assert_eq!(block.created_at, deserialized.created_at);
        }

        #[test]
        fn edge_json_roundtrip(edge in arb_edge()) {
            let json = serde_json::to_string(&edge).expect("Failed to serialize Edge");
            let deserialized: Edge = serde_json::from_str(&json).expect("Failed to deserialize Edge");

            prop_assert_eq!(edge.id, deserialized.id);
            prop_assert_eq!(edge.from_lineage_id, deserialized.from_lineage_id);
            prop_assert_eq!(edge.to_lineage_id, deserialized.to_lineage_id);
            prop_assert_eq!(&edge.edge_type, &deserialized.edge_type);
            prop_assert_eq!(&edge.properties, &deserialized.properties);
            prop_assert_eq!(edge.deleted_at, deserialized.deleted_at);
            prop_assert_eq!(edge.created_at, deserialized.created_at);
        }

        #[test]
        fn create_atom_default_properties(
            content_type in "[a-z]{2,10}",
            content_template in "[a-zA-Z0-9 ]{0,30}",
        ) {
            let json = serde_json::json!({
                "content_type": content_type,
                "content_template": content_template,
            });
            let dto: CreateAtom = serde_json::from_value(json).expect("Failed to deserialize CreateAtom");
            prop_assert_eq!(dto.properties, serde_json::json!({}));
        }

        #[test]
        fn properties_with_nested_json(props in arb_json_value(3)) {
            // Wrap in an object if not already one (Atom.properties is a Value)
            let properties = if props.is_object() {
                props.clone()
            } else {
                serde_json::json!({"nested": props})
            };

            let atom = Atom {
                id: Uuid::now_v7(),
                content_type: "content".to_string(),
                content_template: "test".to_string(),
                links: vec![],
                properties: properties.clone(),
                content_hash: "hash".to_string(),
                predecessor_id: None,
                created_at: Utc::now(),
            };

            let json = serde_json::to_string(&atom).expect("Failed to serialize");
            let deserialized: Atom = serde_json::from_str(&json).expect("Failed to deserialize");
            prop_assert_eq!(&properties, &deserialized.properties);
        }

        #[test]
        fn uuidv7_monotonicity(n in 2usize..=100) {
            let ids: Vec<Uuid> = (0..n).map(|_| Uuid::now_v7()).collect();
            for i in 1..ids.len() {
                prop_assert!(ids[i] > ids[i - 1],
                    "UUID at index {} ({}) should be greater than UUID at index {} ({})",
                    i, ids[i], i - 1, ids[i - 1]);
            }
        }
    }
}
