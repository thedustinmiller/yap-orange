//! Default seed data for bootstrapping fresh databases.
//!
//! Embeds the tutorial fixture at compile time via `include_str!` so all
//! backends (server, desktop, WASM) can seed without filesystem access.

use crate::export::ExportTree;

/// The built-in tutorial fixture, embedded at compile time.
const TUTORIAL_JSON: &str = include_str!("../../../fixtures/tutorial.json");

/// Returns the default seed trees embedded at compile time.
///
/// Currently returns a single tree (the tutorial). The returned trees
/// are suitable for passing to `bootstrap(db, &trees)`.
pub fn default_seed_trees() -> Vec<ExportTree> {
    parse_seed_json(TUTORIAL_JSON).expect("embedded tutorial.json must be valid")
}

/// Parse a JSON string as seed data.
///
/// Accepts either a single `ExportTree` object or a JSON array of them,
/// allowing `YAP_SEED_FILE` to point to either format.
pub fn parse_seed_json(json: &str) -> anyhow::Result<Vec<ExportTree>> {
    // Try as array first
    if let Ok(trees) = serde_json::from_str::<Vec<ExportTree>>(json) {
        return Ok(trees);
    }
    // Fall back to single tree
    let tree: ExportTree = serde_json::from_str(json)?;
    Ok(vec![tree])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_seed_trees_loads() {
        let trees = default_seed_trees();
        assert!(!trees.is_empty(), "should load at least one tree");
    }

    #[test]
    fn tutorial_has_expected_structure() {
        let trees = default_seed_trees();
        let tree = &trees[0];
        assert_eq!(tree.source_namespace, "tutorial");
        assert!(tree.nodes.len() >= 20, "tutorial should have 20+ nodes");
        assert!(!tree.edges.is_empty(), "tutorial should have edges");

        // Root node
        assert_eq!(tree.nodes[0].local_id, 0);
        assert_eq!(tree.nodes[0].name, "tutorial");
        assert!(tree.nodes[0].parent_local_id.is_none());
    }

    #[test]
    fn tutorial_nodes_in_bfs_order() {
        let trees = default_seed_trees();
        let tree = &trees[0];
        // Every node's parent must appear before it (BFS guarantee)
        for node in &tree.nodes {
            if let Some(parent_id) = node.parent_local_id {
                assert!(
                    parent_id < node.local_id,
                    "node {} has parent {} which should appear earlier",
                    node.local_id,
                    parent_id,
                );
            }
        }
    }

    #[test]
    fn tutorial_has_internal_links() {
        let trees = default_seed_trees();
        let tree = &trees[0];
        let total_links: usize = tree.nodes.iter().map(|n| n.internal_links.len()).sum();
        assert!(
            total_links >= 10,
            "tutorial should have 10+ internal links, found {}",
            total_links
        );
    }

    #[test]
    fn parse_seed_json_single_tree() {
        let json = r#"{"format":"yap-tree-v1","exported_at":"2026-01-01T00:00:00Z","source_namespace":"test","nodes":[],"edges":[]}"#;
        let trees = parse_seed_json(json).unwrap();
        assert_eq!(trees.len(), 1);
    }

    #[test]
    fn parse_seed_json_array() {
        let json = r#"[{"format":"yap-tree-v1","exported_at":"2026-01-01T00:00:00Z","source_namespace":"a","nodes":[],"edges":[]},{"format":"yap-tree-v1","exported_at":"2026-01-01T00:00:00Z","source_namespace":"b","nodes":[],"edges":[]}]"#;
        let trees = parse_seed_json(json).unwrap();
        assert_eq!(trees.len(), 2);
    }

    #[test]
    fn parse_seed_json_invalid() {
        assert!(parse_seed_json("not json").is_err());
    }
}
