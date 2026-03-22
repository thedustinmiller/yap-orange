//! Content hashing utilities.
//!
//! SHA-256 hash over the content fields of an atom, used for deduplication.
//!
//! # Three-Layer Hash System (v2)
//!
//! In addition to the DB-level `content_hash`, the export system uses three
//! composable hash layers:
//!
//! 1. **Content identity** — content_type + template + (optionally) properties
//! 2. **Merkle** — content identity + name + children's merkle hashes (recursive)
//! 3. **Topology** — root merkle + all cross-link triples

use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Compute a SHA-256 content hash from the atom's content fields.
///
/// Links are sorted before hashing so the hash is stable regardless of
/// insertion order. The null byte separates fields to prevent collisions.
pub fn compute_content_hash(content_type: &str, template: &str, links: &[Uuid]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content_type.as_bytes());
    hasher.update(b"\0");
    hasher.update(template.as_bytes());
    hasher.update(b"\0");
    let mut sorted = links.to_vec();
    sorted.sort();
    for link in &sorted {
        hasher.update(link.as_bytes());
    }
    hex::encode(hasher.finalize())
}

// =============================================================================
// Three-Layer Hash Functions (v2 Export)
// =============================================================================

/// Layer 1: Content identity hash.
///
/// SHA-256 over `content_type || \0 || content_template || \0 || canonical_properties`.
/// The caller decides whether to pass properties (based on content_type awareness).
/// When `properties` is `None`, the properties segment is omitted entirely.
/// When `properties` is `Some({})` (empty after filtering), treated same as `None`.
pub fn compute_content_identity_hash(
    content_type: &str,
    content_template: &str,
    properties: Option<&serde_json::Value>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content_type.as_bytes());
    hasher.update(b"\0");
    hasher.update(content_template.as_bytes());
    hasher.update(b"\0");
    if let Some(props) = properties {
        let bytes = canonical_properties_bytes(props);
        if !bytes.is_empty() {
            hasher.update(&bytes);
        }
    }
    hex::encode(hasher.finalize())
}

/// Layer 2: Merkle hash over a node and its children.
///
/// SHA-256 over `content_identity_hash || \0 || name || \0 || sorted(children).join(\0)`.
/// Children are sorted lexicographically before joining (sibling-order independent).
pub fn compute_merkle_hash(
    content_identity_hash: &str,
    name: &str,
    children_merkle_hashes: &[&str],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content_identity_hash.as_bytes());
    hasher.update(b"\0");
    hasher.update(name.as_bytes());
    hasher.update(b"\0");
    let mut sorted: Vec<&str> = children_merkle_hashes.to_vec();
    sorted.sort();
    for (i, child_hash) in sorted.iter().enumerate() {
        if i > 0 {
            hasher.update(b"\0");
        }
        hasher.update(child_hash.as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// Layer 3: Topology hash over the full tree including cross-links.
///
/// Each triple is `(source_merkle_hash, placeholder_index, target_merkle_hash)`.
/// Triples are sorted lexicographically, then SHA-256 over
/// `root_merkle || \0 || sorted_triples_canonical`.
pub fn compute_topology_hash(
    root_merkle_hash: &str,
    link_triples: &mut [(String, usize, String)],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(root_merkle_hash.as_bytes());
    hasher.update(b"\0");
    link_triples.sort();
    for (i, (src, idx, tgt)) in link_triples.iter().enumerate() {
        if i > 0 {
            hasher.update(b"\0");
        }
        hasher.update(src.as_bytes());
        hasher.update(b"\0");
        hasher.update(idx.to_string().as_bytes());
        hasher.update(b"\0");
        hasher.update(tgt.as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// Canonical byte representation of JSON properties for hashing.
///
/// Filters out underscore-prefixed keys (e.g. `_import_hash`), sorts remaining
/// keys, and serializes as `key1\0value1\0key2\0value2\0...`.
/// Returns empty vec for non-object or empty-after-filtering values.
fn canonical_properties_bytes(props: &serde_json::Value) -> Vec<u8> {
    let obj = match props.as_object() {
        Some(o) => o,
        None => return Vec::new(),
    };

    let mut keys: Vec<&String> = obj.keys().filter(|k| !k.starts_with('_')).collect();

    if keys.is_empty() {
        return Vec::new();
    }

    keys.sort();

    let mut bytes = Vec::new();
    for (i, key) in keys.iter().enumerate() {
        if i > 0 {
            bytes.push(b'\0');
        }
        bytes.extend_from_slice(key.as_bytes());
        bytes.push(b'\0');
        // Value as canonical JSON string
        let val = &obj[key.as_str()];
        let val_str = serde_json::to_string(val).unwrap_or_default();
        bytes.extend_from_slice(val_str.as_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_stable() {
        let h1 = compute_content_hash("content", "hello", &[]);
        let h2 = compute_content_hash("content", "hello", &[]);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_link_order_independent() {
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        let h1 = compute_content_hash("content", "x", &[a, b]);
        let h2 = compute_content_hash("content", "x", &[b, a]);
        assert_eq!(h1, h2, "link order should not affect hash");
    }

    #[test]
    fn test_hash_different_inputs_differ() {
        let h1 = compute_content_hash("content", "hello", &[]);
        let h2 = compute_content_hash("content", "world", &[]);
        assert_ne!(h1, h2);
    }

    // =========================================================================
    // Content Identity Hash Tests
    // =========================================================================

    #[test]
    fn test_content_identity_hash_stable() {
        let h1 = compute_content_identity_hash("content", "hello", None);
        let h2 = compute_content_identity_hash("content", "hello", None);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_content_identity_hash_ignores_underscore_props() {
        let props_with = serde_json::json!({"_import_hash": "x", "foo": "bar"});
        let props_without = serde_json::json!({"foo": "bar"});
        let h1 = compute_content_identity_hash("content", "hello", Some(&props_with));
        let h2 = compute_content_identity_hash("content", "hello", Some(&props_without));
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_content_identity_hash_none_vs_empty_props() {
        // Some({}) after filtering = no props bytes = same as None
        let empty = serde_json::json!({});
        let h1 = compute_content_identity_hash("content", "hello", None);
        let h2 = compute_content_identity_hash("content", "hello", Some(&empty));
        assert_eq!(h1, h2);

        // Some({_only_underscore: "x"}) after filtering = empty = same as None
        let underscore_only = serde_json::json!({"_hidden": "x"});
        let h3 = compute_content_identity_hash("content", "hello", Some(&underscore_only));
        assert_eq!(h1, h3);
    }

    #[test]
    fn test_content_identity_hash_with_props() {
        let props = serde_json::json!({"foo": "bar"});
        let h1 = compute_content_identity_hash("content", "hello", None);
        let h2 = compute_content_identity_hash("content", "hello", Some(&props));
        assert_ne!(h1, h2, "including props should change hash");
    }

    #[test]
    fn test_content_identity_hash_prop_order_independent() {
        let props1 = serde_json::json!({"a": 1, "b": 2});
        let props2 = serde_json::json!({"b": 2, "a": 1});
        let h1 = compute_content_identity_hash("schema", "x", Some(&props1));
        let h2 = compute_content_identity_hash("schema", "x", Some(&props2));
        assert_eq!(h1, h2, "property key order should not matter");
    }

    // =========================================================================
    // Merkle Hash Tests
    // =========================================================================

    #[test]
    fn test_merkle_hash_stable() {
        let h1 = compute_merkle_hash("abc123", "mynode", &["child1", "child2"]);
        let h2 = compute_merkle_hash("abc123", "mynode", &["child1", "child2"]);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_merkle_hash_child_order_independent() {
        let h1 = compute_merkle_hash("abc123", "mynode", &["abc", "def"]);
        let h2 = compute_merkle_hash("abc123", "mynode", &["def", "abc"]);
        assert_eq!(h1, h2, "child order should not affect merkle hash");
    }

    #[test]
    fn test_merkle_hash_name_matters() {
        let h1 = compute_merkle_hash("abc123", "name-a", &[]);
        let h2 = compute_merkle_hash("abc123", "name-b", &[]);
        assert_ne!(h1, h2, "different name should produce different hash");
    }

    #[test]
    fn test_merkle_leaf() {
        let h1 = compute_merkle_hash("abc123", "leaf", &[]);
        let h2 = compute_merkle_hash("abc123", "leaf", &[]);
        assert_eq!(h1, h2, "leaf node should be deterministic");
        // Leaf with empty children differs from node with children
        let h3 = compute_merkle_hash("abc123", "leaf", &["child1"]);
        assert_ne!(h1, h3);
    }

    // =========================================================================
    // Topology Hash Tests
    // =========================================================================

    #[test]
    fn test_topology_hash_stable() {
        let mut t1 = vec![
            ("src1".to_string(), 0, "tgt1".to_string()),
            ("src2".to_string(), 1, "tgt2".to_string()),
        ];
        let mut t2 = vec![
            ("src1".to_string(), 0, "tgt1".to_string()),
            ("src2".to_string(), 1, "tgt2".to_string()),
        ];
        let h1 = compute_topology_hash("root_merkle", &mut t1);
        let h2 = compute_topology_hash("root_merkle", &mut t2);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_topology_hash_differs_on_link_change() {
        let mut t1 = vec![("src".to_string(), 0, "tgt_a".to_string())];
        let mut t2 = vec![("src".to_string(), 0, "tgt_b".to_string())];
        let h1 = compute_topology_hash("root", &mut t1);
        let h2 = compute_topology_hash("root", &mut t2);
        assert_ne!(h1, h2, "different target should produce different hash");
    }

    #[test]
    fn test_topology_hash_no_links() {
        let mut empty = Vec::new();
        let h1 = compute_topology_hash("root", &mut empty);
        let mut empty2 = Vec::new();
        let h2 = compute_topology_hash("root", &mut empty2);
        assert_eq!(h1, h2, "no links should be valid and deterministic");
    }
}

#[cfg(test)]
mod proptest_hash {
    use super::*;
    use proptest::prelude::*;

    fn arb_hex_hash() -> impl Strategy<Value = String> {
        "[0-9a-f]{64}" // SHA-256 hex string
    }

    fn arb_content_type() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("content".to_string()),
            Just("namespace".to_string()),
            Just("schema".to_string()),
            "[a-z]{3,10}",
        ]
    }

    fn arb_template() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 .,!?\n]{0,100}"
    }

    proptest! {
        // 1. content_hash_determinism — same inputs always produce same hash
        #[test]
        fn content_hash_determinism(
            ct in arb_content_type(),
            tmpl in arb_template(),
            link_vals in prop::collection::vec(any::<u128>(), 0..=5),
        ) {
            let links: Vec<Uuid> = link_vals.iter().map(|n| Uuid::from_u128(*n)).collect();
            let h1 = compute_content_hash(&ct, &tmpl, &links);
            let h2 = compute_content_hash(&ct, &tmpl, &links);
            prop_assert_eq!(h1, h2);
        }

        // 2. link_order_independence — shuffled link order produces same hash
        #[test]
        fn link_order_independence(
            link_vals in prop::collection::vec(any::<u128>(), 2..=10),
        ) {
            let links: Vec<Uuid> = link_vals.iter().map(|n| Uuid::from_u128(*n)).collect();
            let mut shuffled = links.clone();
            shuffled.reverse(); // simple "shuffle" — reverse the order
            let h1 = compute_content_hash("content", "content", &links);
            let h2 = compute_content_hash("content", "content", &shuffled);
            prop_assert_eq!(h1, h2, "link order should not affect content hash");
        }

        // 3. field_separation — different (type, template) pairs produce different hashes
        #[test]
        fn field_separation(
            type1 in arb_content_type(),
            tmpl1 in arb_template(),
            type2 in arb_content_type(),
            tmpl2 in arb_template(),
        ) {
            prop_assume!(type1 != type2 || tmpl1 != tmpl2);
            let h1 = compute_content_identity_hash(&type1, &tmpl1, None);
            let h2 = compute_content_identity_hash(&type2, &tmpl2, None);
            prop_assert_ne!(h1, h2,
                "different inputs should produce different hashes: ({}, {}) vs ({}, {})",
                type1, tmpl1, type2, tmpl2);
        }

        // 4. property_order_independence — same keys in different order produce same hash
        #[test]
        fn property_order_independence(
            pairs in prop::collection::vec(
                ("[a-z]{1,5}", "[a-z0-9]{1,10}"),
                2..=5,
            ),
        ) {
            // Ensure unique keys
            let mut seen = std::collections::HashSet::new();
            let unique_pairs: Vec<_> = pairs.into_iter()
                .filter(|(k, _)| seen.insert(k.clone()))
                .collect();
            prop_assume!(unique_pairs.len() >= 2);

            // Build two JSON objects with same keys but different insertion order
            let mut sorted_pairs = unique_pairs.clone();
            sorted_pairs.sort_by(|a, b| a.0.cmp(&b.0));
            let mut reversed_pairs = sorted_pairs.clone();
            reversed_pairs.reverse();

            let obj1: serde_json::Map<String, serde_json::Value> = sorted_pairs.iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect();
            let obj2: serde_json::Map<String, serde_json::Value> = reversed_pairs.iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect();

            let props1 = serde_json::Value::Object(obj1);
            let props2 = serde_json::Value::Object(obj2);

            let h1 = compute_content_identity_hash("content", "x", Some(&props1));
            let h2 = compute_content_identity_hash("content", "x", Some(&props2));
            prop_assert_eq!(h1, h2, "property key order should not affect hash");
        }

        // 5. underscore_filtering — _prefixed keys are ignored in hash
        #[test]
        fn underscore_filtering(
            key in "[a-z]{1,5}",
            val in "[a-z0-9]{1,10}",
            underscore_key in "[a-z]{1,5}",
            underscore_val in "[a-z0-9]{1,10}",
        ) {
            let base_props = serde_json::json!({ &key: &val });
            let extended_props = serde_json::json!({
                &key: &val,
                format!("_{}", underscore_key): &underscore_val,
            });

            let h1 = compute_content_identity_hash("content", "x", Some(&base_props));
            let h2 = compute_content_identity_hash("content", "x", Some(&extended_props));
            prop_assert_eq!(h1, h2, "underscore-prefixed keys should be filtered out");
        }

        // 6. merkle_child_order_independence — shuffled children produce same merkle hash
        #[test]
        fn merkle_child_order_independence(
            identity in arb_hex_hash(),
            name in "[a-z]{1,10}",
            children in prop::collection::vec(arb_hex_hash(), 2..=8),
        ) {
            let mut reversed = children.clone();
            reversed.reverse();

            let refs1: Vec<&str> = children.iter().map(|s| s.as_str()).collect();
            let refs2: Vec<&str> = reversed.iter().map(|s| s.as_str()).collect();

            let h1 = compute_merkle_hash(&identity, &name, &refs1);
            let h2 = compute_merkle_hash(&identity, &name, &refs2);
            prop_assert_eq!(h1, h2, "child order should not affect merkle hash");
        }

        // 7. merkle_child_presence — adding a child changes the merkle hash
        #[test]
        fn merkle_child_presence(
            identity in arb_hex_hash(),
            name in "[a-z]{1,10}",
            extra_child in arb_hex_hash(),
        ) {
            let without: Vec<&str> = vec![];
            let with: Vec<&str> = vec![extra_child.as_str()];

            let h1 = compute_merkle_hash(&identity, &name, &without);
            let h2 = compute_merkle_hash(&identity, &name, &with);
            prop_assert_ne!(h1, h2, "adding a child should change merkle hash");
        }

        // 8. topology_triple_order_independence — shuffled triples produce same topology hash
        #[test]
        fn topology_triple_order_independence(
            root in arb_hex_hash(),
            triples in prop::collection::vec(
                (arb_hex_hash(), 0..100usize, arb_hex_hash()),
                2..=6,
            ),
        ) {
            let mut t1 = triples.clone();
            let mut t2 = triples.clone();
            t2.reverse();

            let h1 = compute_topology_hash(&root, &mut t1);
            let h2 = compute_topology_hash(&root, &mut t2);
            prop_assert_eq!(h1, h2, "triple order should not affect topology hash");
        }

        // 9. merkle_name_sensitivity — different names produce different merkle hashes
        #[test]
        fn merkle_name_sensitivity(
            identity in arb_hex_hash(),
            name1 in "[a-z]{1,10}",
            name2 in "[a-z]{1,10}",
            children in prop::collection::vec(arb_hex_hash(), 0..=3),
        ) {
            prop_assume!(name1 != name2);
            let refs: Vec<&str> = children.iter().map(|s| s.as_str()).collect();

            let h1 = compute_merkle_hash(&identity, &name1, &refs);
            let h2 = compute_merkle_hash(&identity, &name2, &refs);
            prop_assert_ne!(h1, h2,
                "different names should produce different merkle hashes: '{}' vs '{}'",
                name1, name2);
        }
    }
}
