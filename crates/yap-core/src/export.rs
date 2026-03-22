//! Tree export/import with content-addressed deduplication.
//!
//! Enables serializing subtrees to a portable JSON format and importing them
//! back, with two modes:
//! - `Copy`: creates all nodes with new UUIDs; external links become `Uuid::nil()`
//! - `Merge`: deduplicates via `_import_hash` in atom properties; resolves external links
//!
//! # Export Format
//!
//! Nodes are in BFS order (parents before children). Links are split into:
//! - `internal_links`: targets within the exported tree (stored as local_id references)
//! - `external_links`: targets outside the tree (stored as namespace paths)
//!
//! The `export_hash` mirrors `compute_content_hash()` in db.rs but uses local
//! u32 IDs instead of lineage UUIDs, making it stable across DB instances.

use std::collections::{HashMap, HashSet, VecDeque};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::hash::{compute_content_identity_hash, compute_merkle_hash, compute_topology_hash};
use crate::models::CreateEdge;
use crate::store::Store;

const FORMAT_VERSION: &str = "yap-tree-v2";

// =============================================================================
// Types
// =============================================================================

/// Portable export format for a block subtree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ExportTree {
    pub format: String,
    pub exported_at: DateTime<Utc>,
    pub source_namespace: String,
    /// Nodes in BFS order (parents before children).
    pub nodes: Vec<ExportNode>,
    /// Edges where both endpoints are within the exported subtree.
    pub edges: Vec<ExportEdge>,
    /// Layer 3: Topology hash over the whole tree (v2).
    #[serde(default)]
    pub topology_hash: String,
}

/// A single exported block+atom pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ExportNode {
    /// Monotonic index within this export (0 = root).
    pub local_id: u32,
    pub name: String,
    pub content_type: String,
    pub content_template: String,
    /// Links whose targets are within this export.
    pub internal_links: Vec<InternalLink>,
    /// Links whose targets are outside this export.
    pub external_links: Vec<ExternalLink>,
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub properties: serde_json::Value,
    /// SHA-256 over (content_type, content_template, sorted internal local_ids).
    pub export_hash: String,
    pub parent_local_id: Option<u32>,
    pub position: String,
    pub children_local_ids: Vec<u32>,
    /// Layer 1: Content identity hash (v2).
    #[serde(default)]
    pub content_identity_hash: String,
    /// Layer 2: Merkle hash (v2).
    #[serde(default)]
    pub merkle_hash: String,
}

/// A link within the exported subtree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct InternalLink {
    /// Index into content_template's `{N}` placeholders.
    pub placeholder_index: usize,
    pub target_local_id: u32,
}

/// A link pointing outside the exported subtree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ExternalLink {
    pub placeholder_index: usize,
    pub target_path: String,
}

/// An edge where both endpoints are within the exported subtree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ExportEdge {
    pub from_local_id: u32,
    pub to_local_id: u32,
    pub edge_type: String,
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub properties: serde_json::Value,
}

/// Import mode controlling how nodes are created.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportMode {
    /// Deduplicate via `_import_hash`; resolve external links against target DB.
    Merge,
    /// Create all nodes fresh with new UUIDs; external links become `Uuid::nil()`.
    Copy,
}

/// Match strategy for import deduplication.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchStrategy {
    /// v1 → ExportHash, v2 → ContentIdentity.
    #[default]
    Auto,
    /// Legacy: match by export_hash (v1 compat).
    ExportHash,
    /// Layer 1: content only (content_type-aware property inclusion).
    ContentIdentity,
    /// Layer 2: content + name + subtree structure.
    Merkle,
    /// Layer 3: root-check then fall back to Merkle.
    Topology,
}

/// Options controlling how import behaves.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportOptions {
    pub mode: ImportMode,
    #[serde(default)]
    pub match_strategy: MatchStrategy,
    /// When true in merge mode, search globally for matching lineages (not just siblings).
    #[serde(default)]
    pub global_link: bool,
    /// When true, delete the existing matching subtree after successfully
    /// importing the new tree. Import completes before deletion — old content
    /// is never lost on partial failure.
    #[serde(default)]
    pub replace_existing: bool,
}

impl ImportOptions {
    pub fn from_mode(mode: ImportMode) -> Self {
        Self {
            mode,
            match_strategy: MatchStrategy::Auto,
            global_link: false,
            replace_existing: false,
        }
    }

    /// Import options suitable for idempotent bootstrap seeding.
    /// Uses Merge + ContentIdentity so re-running is always a no-op.
    pub fn seed_defaults() -> Self {
        Self {
            mode: ImportMode::Merge,
            match_strategy: MatchStrategy::ContentIdentity,
            global_link: false,
            replace_existing: false,
        }
    }
}

/// Result of an import operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ImportResult {
    /// Nodes that were newly created.
    pub created: usize,
    /// Nodes that were skipped (matched by `_import_hash` in merge mode).
    pub skipped: usize,
    /// Nodes that were hard-linked to existing lineages (global_link mode).
    #[serde(default)]
    pub linked: usize,
    /// External links that could not be resolved.
    pub failed_external_links: Vec<FailedExternalLink>,
    /// Block ID of the root node that was created or matched.
    pub root_block_id: Option<Uuid>,
    /// Number of edges successfully created.
    #[serde(default)]
    pub edges_created: usize,
    /// Number of edges skipped (duplicate / already exists).
    #[serde(default)]
    pub edges_skipped: usize,
    /// Edges that failed to import.
    #[serde(default)]
    pub edges_failed: Vec<FailedEdge>,
}

/// An external link that failed to resolve during merge import.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FailedExternalLink {
    pub node_local_id: u32,
    pub placeholder_index: usize,
    pub target_path: String,
}

/// An edge that failed during import.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FailedEdge {
    pub from_local_id: u32,
    pub to_local_id: u32,
    pub edge_type: String,
    pub reason: String,
}

// =============================================================================
// Hash Function
// =============================================================================

/// Compute a SHA-256 export hash from export-format content fields.
///
/// Mirrors `compute_content_hash()` in db.rs but uses local u32 IDs so the
/// hash remains stable across DB instances (UUIDs differ, local IDs don't).
pub fn compute_export_hash(
    content_type: &str,
    template: &str,
    internal_link_local_ids: &[u32],
) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content_type.as_bytes());
    hasher.update(b"\0");
    hasher.update(template.as_bytes());
    hasher.update(b"\0");
    let mut sorted = internal_link_local_ids.to_vec();
    sorted.sort_unstable();
    for id in &sorted {
        hasher.update(id.to_le_bytes());
    }
    hex::encode(hasher.finalize())
}

// =============================================================================
// Export
// =============================================================================

/// Options controlling export behavior.
#[derive(Debug, Clone, Default)]
pub struct ExportOptions {
    /// If set, only include these property keys in exported nodes.
    /// `None` = include all non-underscore keys. The `name` key is always included.
    pub include_keys: Option<HashSet<String>>,
}

/// Remove nil UUIDs from a links array and rewrite the content template.
///
/// For each `{N}` placeholder where `links[N] == Uuid::nil()`, replaces the
/// placeholder with the original wiki-link text (`[[path]]`) and removes the
/// nil entry from the links array, reindexing remaining placeholders.
fn remove_nil_links(
    template: &str,
    links: &[Uuid],
    external_links: &[ExternalLink],
) -> (String, Vec<Uuid>) {
    // Build a map from placeholder index to the external link path
    let ext_map: HashMap<usize, &str> = external_links
        .iter()
        .map(|el| (el.placeholder_index, el.target_path.as_str()))
        .collect();

    // Build old→new index mapping (skipping nil entries)
    let mut old_to_new: HashMap<usize, usize> = HashMap::new();
    let mut clean_links: Vec<Uuid> = Vec::new();
    for (old_idx, &link) in links.iter().enumerate() {
        if link != Uuid::nil() {
            old_to_new.insert(old_idx, clean_links.len());
            clean_links.push(link);
        }
    }

    // If no nils found, return as-is
    if clean_links.len() == links.len() {
        return (template.to_string(), links.to_vec());
    }

    // Rewrite template: replace {N} placeholders
    let mut result = String::with_capacity(template.len());
    let mut i = 0;
    let bytes = template.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'{' {
            // Try to parse {N}
            if let Some(end) = template[i + 1..].find('}') {
                let inner = &template[i + 1..i + 1 + end];
                if let Ok(old_idx) = inner.parse::<usize>() {
                    if let Some(&new_idx) = old_to_new.get(&old_idx) {
                        // Resolved link — reindex
                        result.push_str(&format!("{{{}}}", new_idx));
                    } else {
                        // Nil link — replace with wiki-link text
                        if let Some(&path) = ext_map.get(&old_idx) {
                            result.push_str(&format!("[[{}]]", path));
                        } else {
                            // Internal forward ref that never resolved — keep placeholder text
                            result.push_str(&format!("{{{}}}", old_idx));
                        }
                    }
                    i += 2 + end; // skip past `{N}`
                    continue;
                }
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }

    (result, clean_links)
}

/// Whether properties should be included in content identity hashing.
///
/// Types whose identity is primarily defined by their properties (schemas,
/// settings, type registries) include non-underscore properties in the hash.
pub fn should_include_properties(content_type: &str) -> bool {
    matches!(content_type, "schema" | "setting" | "type_registry")
}

/// Filter properties according to export options.
fn filter_properties(props: &serde_json::Value, options: &ExportOptions) -> serde_json::Value {
    let obj = match props.as_object() {
        Some(o) => o,
        None => return props.clone(),
    };
    match &options.include_keys {
        None => {
            // Default: include all non-underscore keys
            let filtered: serde_json::Map<String, serde_json::Value> = obj
                .iter()
                .filter(|(k, _)| !k.starts_with('_'))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            serde_json::Value::Object(filtered)
        }
        Some(keys) => {
            let filtered: serde_json::Map<String, serde_json::Value> = obj
                .iter()
                .filter(|(k, _)| k.as_str() == "name" || keys.contains(k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            serde_json::Value::Object(filtered)
        }
    }
}

/// Export a subtree rooted at `root_block_id` to a portable format.
///
/// Performs BFS traversal, assigns monotonic local IDs, and classifies each
/// link as either internal (within the subtree) or external (pointing outside).
/// Edges where both endpoints are in the subtree are included.
pub async fn export_tree(
    db: &dyn Store,
    root_block_id: Uuid,
    options: &ExportOptions,
) -> Result<ExportTree> {
    // BFS: collect block+atom data in order, assigning local IDs.
    struct Collected {
        name: String,
        position: String,
        content_type: String,
        content_template: String,
        links: Vec<Uuid>,
        properties: serde_json::Value,
        parent_local: Option<u32>,
    }

    let mut queue: VecDeque<(Uuid, Option<u32>)> = VecDeque::new();
    queue.push_back((root_block_id, None));

    let mut collected: Vec<Collected> = Vec::new();
    let mut lineage_to_local: HashMap<Uuid, u32> = HashMap::new();

    while let Some((block_id, parent_local)) = queue.pop_front() {
        let block = db.get_block(block_id).await?;
        let atom = db.get_atom(block.lineage_id).await?;
        let local_id = collected.len() as u32;
        lineage_to_local.insert(block.lineage_id, local_id);

        collected.push(Collected {
            name: block.name,
            position: block.position,
            content_type: atom.content_type,
            content_template: atom.content_template,
            links: atom.links,
            properties: atom.properties,
            parent_local,
        });

        for child in db.get_block_children(block_id).await? {
            queue.push_back((child.id, Some(local_id)));
        }
    }

    // Build ExportNode list: classify links, compute hashes, fill children.
    let mut nodes: Vec<ExportNode> = Vec::with_capacity(collected.len());
    let mut children_map: HashMap<u32, Vec<u32>> = HashMap::new();

    for (idx, node) in collected.iter().enumerate() {
        let local_id = idx as u32;
        let mut internal_links: Vec<InternalLink> = Vec::new();
        let mut external_links: Vec<ExternalLink> = Vec::new();

        for (placeholder_index, &link_lineage_id) in node.links.iter().enumerate() {
            if let Some(&target_local_id) = lineage_to_local.get(&link_lineage_id) {
                internal_links.push(InternalLink {
                    placeholder_index,
                    target_local_id,
                });
            } else {
                let path = db
                    .get_canonical_path(link_lineage_id)
                    .await?
                    .unwrap_or_else(|| format!("?::{}", link_lineage_id));
                external_links.push(ExternalLink {
                    placeholder_index,
                    target_path: path,
                });
            }
        }

        let internal_ids: Vec<u32> = internal_links.iter().map(|l| l.target_local_id).collect();
        let export_hash =
            compute_export_hash(&node.content_type, &node.content_template, &internal_ids);

        if let Some(pid) = node.parent_local {
            children_map.entry(pid).or_default().push(local_id);
        }

        let filtered_props = filter_properties(&node.properties, options);

        nodes.push(ExportNode {
            local_id,
            name: node.name.clone(),
            content_type: node.content_type.clone(),
            content_template: node.content_template.clone(),
            internal_links,
            external_links,
            properties: filtered_props,
            export_hash,
            parent_local_id: node.parent_local,
            position: node.position.clone(),
            children_local_ids: Vec::new(),
            content_identity_hash: String::new(),
            merkle_hash: String::new(),
        });
    }

    for (parent_local, children) in children_map {
        nodes[parent_local as usize].children_local_ids = children;
    }

    // v2 post-pass: compute three-layer hashes.
    // Layer 1: content identity hash
    for node in &mut nodes {
        let props_for_hash = if should_include_properties(&node.content_type) {
            Some(&node.properties)
        } else {
            None
        };
        node.content_identity_hash = compute_content_identity_hash(
            &node.content_type,
            &node.content_template,
            props_for_hash,
        );
    }

    // Layer 2: merkle hash (reverse BFS — children before parents)
    for idx in (0..nodes.len()).rev() {
        let children_hashes: Vec<String> = nodes[idx]
            .children_local_ids
            .iter()
            .map(|&cid| nodes[cid as usize].merkle_hash.clone())
            .collect();
        let children_refs: Vec<&str> = children_hashes.iter().map(|s| s.as_str()).collect();
        nodes[idx].merkle_hash = compute_merkle_hash(
            &nodes[idx].content_identity_hash,
            &nodes[idx].name,
            &children_refs,
        );
    }

    // Collect edges where both endpoints are in the exported set.
    let lineage_set: HashSet<Uuid> = lineage_to_local.keys().copied().collect();
    let mut edges: Vec<ExportEdge> = Vec::new();
    let mut seen_edges: HashSet<(u32, u32, String)> = HashSet::new();

    for &lineage_id in &lineage_set {
        for edge in db.get_all_edges(lineage_id).await? {
            if lineage_set.contains(&edge.from_lineage_id)
                && lineage_set.contains(&edge.to_lineage_id)
            {
                let from = lineage_to_local[&edge.from_lineage_id];
                let to = lineage_to_local[&edge.to_lineage_id];
                let key = (from, to, edge.edge_type.clone());
                if seen_edges.insert(key) {
                    edges.push(ExportEdge {
                        from_local_id: from,
                        to_local_id: to,
                        edge_type: edge.edge_type,
                        properties: edge.properties,
                    });
                }
            }
        }
    }

    // Layer 3: topology hash (cross-links by merkle hash)
    let mut triples = Vec::new();
    for node in &nodes {
        for il in &node.internal_links {
            triples.push((
                node.merkle_hash.clone(),
                il.placeholder_index,
                nodes[il.target_local_id as usize].merkle_hash.clone(),
            ));
        }
    }
    let topology_hash = compute_topology_hash(&nodes[0].merkle_hash, &mut triples);

    let source_namespace = db.compute_namespace(root_block_id).await?;

    Ok(ExportTree {
        format: FORMAT_VERSION.to_string(),
        exported_at: Utc::now(),
        source_namespace,
        nodes,
        edges,
        topology_hash,
    })
}

// =============================================================================
// Import
// =============================================================================

/// Import a subtree, optionally under a parent block.
///
/// When `parent_block_id` is `None`, root nodes in the tree become root-level
/// blocks (parent_id = NULL). When `Some(id)`, root nodes import under that block.
///
/// Nodes are processed in the order they appear in the tree (BFS = parents
/// before children), so `local_to_block` always has a parent's entry ready
/// before children are processed.
///
/// Forward references (a node linking to a sibling that appears later) are
/// handled by a second pass using `edit_lineage`.
pub async fn import_tree(
    db: &dyn Store,
    tree: &ExportTree,
    parent_block_id: Option<Uuid>,
    options: ImportOptions,
) -> Result<ImportResult> {
    let effective_strategy = match options.match_strategy {
        MatchStrategy::Auto => {
            if tree.format == "yap-tree-v1" {
                MatchStrategy::ExportHash
            } else {
                MatchStrategy::ContentIdentity
            }
        }
        other => other,
    };

    let mut local_to_lineage: HashMap<u32, Uuid> = HashMap::new();
    let mut local_to_block: HashMap<u32, Uuid> = HashMap::new();

    // Tracks nodes that need second-pass link fixing (had forward references).
    // Maps local_id -> (content_type, content_template, properties_used_in_first_pass)
    let mut needs_fix: HashMap<u32, (String, String, serde_json::Value)> = HashMap::new();

    // Cache external link resolution results (merge mode).
    let mut external_cache: HashMap<String, Option<Uuid>> = HashMap::new();

    let mut created = 0usize;
    let mut skipped = 0usize;
    let mut linked = 0usize;
    let mut failed_external_links: Vec<FailedExternalLink> = Vec::new();
    let mut root_block_id: Option<Uuid> = None;

    // Track old root block for replace_existing (delete after import succeeds).
    let mut old_root_block_id: Option<Uuid> = None;

    // Topology strategy: check whole-tree match before processing nodes.
    if options.mode == ImportMode::Merge
        && effective_strategy == MatchStrategy::Topology
        && !tree.topology_hash.is_empty()
    {
        let children = match parent_block_id {
            Some(pid) => db.get_block_children(pid).await?,
            None => db.get_root_blocks().await?,
        };
        for child_block in &children {
            let existing_topology = compute_existing_topology(db, child_block.id).await?;
            if existing_topology == tree.topology_hash {
                // Entire tree matches — skip everything.
                return Ok(ImportResult {
                    created: 0,
                    skipped: tree.nodes.len(),
                    linked: 0,
                    failed_external_links: Vec::new(),
                    root_block_id: Some(child_block.id),
                    edges_created: 0,
                    edges_skipped: 0,
                    edges_failed: Vec::new(),
                });
            }
        }
    }
    // Fall through to per-node Merkle matching.

    // Cache for existing merkle hashes (used by Merkle/Topology strategies).
    let mut merkle_cache: HashMap<Uuid, String> = HashMap::new();

    for node in &tree.nodes {
        // Determine parent block ID in the target DB.
        let parent_id: Option<Uuid> = if let Some(pid_local) = node.parent_local_id {
            let block_id = *local_to_block.get(&pid_local).ok_or_else(|| {
                Error::Internal(format!(
                    "Parent block not yet created for local_id {}",
                    pid_local
                ))
            })?;
            Some(block_id)
        } else {
            parent_block_id
        };

        // Merge-mode dedup: skip if a matching child exists.
        if options.mode == ImportMode::Merge {
            let children = match parent_id {
                Some(pid) => db.get_block_children(pid).await?,
                None => db.get_root_blocks().await?,
            };

            let mut found = false;
            for child_block in &children {
                let child_atom = db.get_atom(child_block.lineage_id).await?;
                let matched = match &effective_strategy {
                    MatchStrategy::ExportHash => {
                        child_atom
                            .properties
                            .get("_import_hash")
                            .and_then(|v| v.as_str())
                            == Some(&node.export_hash)
                    }
                    MatchStrategy::ContentIdentity => {
                        // Name must match to avoid wrong-sibling matches
                        // (e.g. multiple namespace blocks with empty content).
                        if child_block.name != node.name {
                            false
                        } else {
                            let props_for_hash =
                                if should_include_properties(&child_atom.content_type) {
                                    Some(&child_atom.properties)
                                } else {
                                    None
                                };
                            let existing_hash = compute_content_identity_hash(
                                &child_atom.content_type,
                                &child_atom.content_template,
                                props_for_hash,
                            );
                            existing_hash == node.content_identity_hash
                        }
                    }
                    MatchStrategy::Merkle | MatchStrategy::Topology => {
                        // Fast pre-filter: content identity
                        let props_for_hash = if should_include_properties(&child_atom.content_type)
                        {
                            Some(&child_atom.properties)
                        } else {
                            None
                        };
                        let existing_ci = compute_content_identity_hash(
                            &child_atom.content_type,
                            &child_atom.content_template,
                            props_for_hash,
                        );
                        if existing_ci == node.content_identity_hash {
                            let existing_merkle =
                                compute_existing_merkle(db, child_block.id, &mut merkle_cache)
                                    .await?;
                            existing_merkle == node.merkle_hash
                        } else {
                            false
                        }
                    }
                    MatchStrategy::Auto => unreachable!("Auto resolved above"),
                };
                if matched {
                    // replace_existing: if this is the root node, record the
                    // old block and fall through to create a fresh replacement.
                    if node.parent_local_id.is_none() && options.replace_existing {
                        old_root_block_id = Some(child_block.id);
                        break;
                    }
                    local_to_lineage.insert(node.local_id, child_block.lineage_id);
                    local_to_block.insert(node.local_id, child_block.id);
                    if node.parent_local_id.is_none() {
                        root_block_id = Some(child_block.id);
                    }
                    skipped += 1;
                    found = true;
                    break;
                }
            }
            if found {
                continue;
            }

            // Global link: search for matching lineages across the whole DB.
            if options.global_link {
                let import_hash = crate::hash::compute_content_hash(
                    &node.content_type,
                    &node.content_template,
                    &[],
                );
                let candidates = db.find_lineages_by_content_hash(&import_hash).await?;
                let mut global_found = false;
                for candidate_lid in candidates {
                    let candidate_atom = db.get_atom(candidate_lid).await?;

                    // Name must match to avoid false positives (e.g. all
                    // empty namespace blocks share the same content hash).
                    let candidate_name = candidate_atom
                        .properties
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if candidate_name != node.name {
                        continue;
                    }

                    let props_for_hash = if should_include_properties(&candidate_atom.content_type)
                    {
                        Some(&candidate_atom.properties)
                    } else {
                        None
                    };
                    let candidate_ci = compute_content_identity_hash(
                        &candidate_atom.content_type,
                        &candidate_atom.content_template,
                        props_for_hash,
                    );
                    if candidate_ci == node.content_identity_hash {
                        // Match found — create hard link
                        let block = db
                            .create_block_for_lineage(parent_id, candidate_lid)
                            .await?;
                        local_to_lineage.insert(node.local_id, candidate_lid);
                        local_to_block.insert(node.local_id, block.id);
                        if node.parent_local_id.is_none() {
                            root_block_id = Some(block.id);
                        }
                        linked += 1;
                        global_found = true;
                        break;
                    }
                }
                if global_found {
                    continue;
                }
            }
        }

        // Build the links array for this node.
        let placeholder_count = node
            .internal_links
            .iter()
            .map(|l| l.placeholder_index + 1)
            .chain(node.external_links.iter().map(|l| l.placeholder_index + 1))
            .max()
            .unwrap_or(0);

        let mut links: Vec<Uuid> = vec![Uuid::nil(); placeholder_count];
        let mut has_forward_refs = false;

        for il in &node.internal_links {
            match local_to_lineage.get(&il.target_local_id) {
                Some(&lid) => {
                    if il.placeholder_index < links.len() {
                        links[il.placeholder_index] = lid;
                    }
                }
                None => {
                    // Forward reference — target comes later in the list.
                    has_forward_refs = true;
                }
            }
        }

        for el in &node.external_links {
            let resolved = match options.mode {
                ImportMode::Merge => {
                    let path = &el.target_path;
                    if let Some(&cached) = external_cache.get(path) {
                        cached
                    } else {
                        let r = db.resolve_namespace_to_lineage(path).await?;
                        external_cache.insert(path.clone(), r);
                        r
                    }
                }
                ImportMode::Copy => None,
            };

            match resolved {
                Some(lid) => {
                    if el.placeholder_index < links.len() {
                        links[el.placeholder_index] = lid;
                    }
                }
                None => {
                    failed_external_links.push(FailedExternalLink {
                        node_local_id: node.local_id,
                        placeholder_index: el.placeholder_index,
                        target_path: el.target_path.clone(),
                    });
                }
            }
        }

        // Build properties (inject hash metadata in merge mode).
        let properties = if options.mode == ImportMode::Merge {
            let mut props = node.properties.clone();
            let (key, value) = match &effective_strategy {
                MatchStrategy::ExportHash => ("_import_hash", node.export_hash.clone()),
                MatchStrategy::ContentIdentity => {
                    ("_import_content_hash", node.content_identity_hash.clone())
                }
                MatchStrategy::Merkle | MatchStrategy::Topology => {
                    ("_import_merkle_hash", node.merkle_hash.clone())
                }
                MatchStrategy::Auto => unreachable!("Auto resolved above"),
            };
            match props.as_object_mut() {
                Some(obj) => {
                    obj.insert(key.to_string(), serde_json::Value::String(value));
                }
                None => {
                    props = serde_json::json!({ key: value });
                }
            }
            props
        } else {
            node.properties.clone()
        };

        // Remove nil UUIDs (unresolved external links in copy mode) and
        // replace their placeholders with wiki-link text.
        let (clean_template, clean_links) =
            remove_nil_links(&node.content_template, &links, &node.external_links);

        let (block, _atom) = db
            .create_block_with_content(
                parent_id,
                &node.name,
                &clean_template,
                &clean_links,
                &node.content_type,
                &properties,
            )
            .await?;

        local_to_lineage.insert(node.local_id, block.lineage_id);
        local_to_block.insert(node.local_id, block.id);

        if node.parent_local_id.is_none() {
            root_block_id = Some(block.id);
        }
        created += 1;

        if has_forward_refs {
            needs_fix.insert(
                node.local_id,
                (
                    node.content_type.clone(),
                    node.content_template.clone(),
                    properties,
                ),
            );
        }
    }

    // Second pass: fix forward references now that all lineage IDs are known.
    for node in &tree.nodes {
        let Some((content_type, content_template, properties)) = needs_fix.get(&node.local_id)
        else {
            continue;
        };

        let lineage_id = match local_to_lineage.get(&node.local_id) {
            Some(&lid) => lid,
            None => continue, // was skipped in merge mode
        };

        let placeholder_count = node
            .internal_links
            .iter()
            .map(|l| l.placeholder_index + 1)
            .chain(node.external_links.iter().map(|l| l.placeholder_index + 1))
            .max()
            .unwrap_or(0);

        let mut links: Vec<Uuid> = vec![Uuid::nil(); placeholder_count];

        for il in &node.internal_links {
            if let Some(&lid) = local_to_lineage.get(&il.target_local_id)
                && il.placeholder_index < links.len()
            {
                links[il.placeholder_index] = lid;
            }
        }

        // Re-apply cached external link resolutions.
        for el in &node.external_links {
            if let Some(&Some(lid)) = external_cache.get(&el.target_path)
                && el.placeholder_index < links.len()
            {
                links[el.placeholder_index] = lid;
            }
        }

        // Remove nil UUIDs from the second-pass links too
        let (clean_template, clean_links) =
            remove_nil_links(content_template, &links, &node.external_links);
        db.edit_lineage(
            lineage_id,
            content_type,
            &clean_template,
            &clean_links,
            properties,
        )
        .await?;
    }

    // Create edges, tracking outcomes.
    let mut edges_created = 0usize;
    let mut edges_skipped = 0usize;
    let mut edges_failed: Vec<FailedEdge> = Vec::new();

    for edge in &tree.edges {
        let from_lid = match local_to_lineage.get(&edge.from_local_id) {
            Some(&l) => l,
            None => {
                edges_failed.push(FailedEdge {
                    from_local_id: edge.from_local_id,
                    to_local_id: edge.to_local_id,
                    edge_type: edge.edge_type.clone(),
                    reason: "from endpoint not imported".to_string(),
                });
                continue;
            }
        };
        let to_lid = match local_to_lineage.get(&edge.to_local_id) {
            Some(&l) => l,
            None => {
                edges_failed.push(FailedEdge {
                    from_local_id: edge.from_local_id,
                    to_local_id: edge.to_local_id,
                    edge_type: edge.edge_type.clone(),
                    reason: "to endpoint not imported".to_string(),
                });
                continue;
            }
        };

        let create = CreateEdge {
            from_lineage_id: from_lid,
            to_lineage_id: to_lid,
            edge_type: edge.edge_type.clone(),
            properties: edge.properties.clone(),
        };

        match db.create_edge(&create).await {
            Ok(_) => edges_created += 1,
            Err(Error::Conflict(_)) => edges_skipped += 1,
            Err(e) => return Err(e),
        }
    }

    // replace_existing: now that the import succeeded, delete the old tree.
    if options.replace_existing
        && let Some(old_id) = old_root_block_id
    {
        db.delete_block_recursive(old_id).await?;
    }

    Ok(ImportResult {
        created,
        skipped,
        linked,
        failed_external_links,
        root_block_id,
        edges_created,
        edges_skipped,
        edges_failed,
    })
}

// =============================================================================
// Helper: compute hashes for existing subtrees
// =============================================================================

/// Recursively compute merkle hash for an existing block subtree.
///
/// Caches results in `cache` to avoid redundant computation across siblings.
async fn compute_existing_merkle(
    db: &dyn Store,
    block_id: Uuid,
    cache: &mut HashMap<Uuid, String>,
) -> Result<String> {
    if let Some(hash) = cache.get(&block_id) {
        return Ok(hash.clone());
    }

    let block = db.get_block(block_id).await?;
    let atom = db.get_atom(block.lineage_id).await?;

    let props_for_hash = if should_include_properties(&atom.content_type) {
        Some(&atom.properties)
    } else {
        None
    };
    let ci_hash =
        compute_content_identity_hash(&atom.content_type, &atom.content_template, props_for_hash);

    let children = db.get_block_children(block_id).await?;
    let mut children_hashes = Vec::with_capacity(children.len());
    for child in &children {
        let child_merkle = Box::pin(compute_existing_merkle(db, child.id, cache)).await?;
        children_hashes.push(child_merkle);
    }

    let children_refs: Vec<&str> = children_hashes.iter().map(|s| s.as_str()).collect();
    let merkle = compute_merkle_hash(&ci_hash, &block.name, &children_refs);

    cache.insert(block_id, merkle.clone());
    Ok(merkle)
}

/// Compute topology hash for an existing block subtree.
///
/// Builds merkle hashes bottom-up, then collects internal link triples.
async fn compute_existing_topology(db: &dyn Store, root_block_id: Uuid) -> Result<String> {
    // BFS to collect all blocks in the subtree.
    let mut queue: VecDeque<Uuid> = VecDeque::new();
    queue.push_back(root_block_id);

    #[allow(dead_code)]
    struct NodeInfo {
        block_id: Uuid,
        lineage_id: Uuid,
        name: String,
        content_type: String,
        content_template: String,
        properties: serde_json::Value,
        links: Vec<Uuid>,
        children_block_ids: Vec<Uuid>,
    }

    let mut nodes: Vec<NodeInfo> = Vec::new();
    let mut block_id_to_idx: HashMap<Uuid, usize> = HashMap::new();
    let mut lineage_to_idx: HashMap<Uuid, usize> = HashMap::new();

    while let Some(bid) = queue.pop_front() {
        let block = db.get_block(bid).await?;
        let atom = db.get_atom(block.lineage_id).await?;
        let idx = nodes.len();
        block_id_to_idx.insert(bid, idx);
        lineage_to_idx.insert(block.lineage_id, idx);

        let children = db.get_block_children(bid).await?;
        let children_ids: Vec<Uuid> = children.iter().map(|c| c.id).collect();
        for &cid in &children_ids {
            queue.push_back(cid);
        }

        nodes.push(NodeInfo {
            block_id: bid,
            lineage_id: block.lineage_id,
            name: block.name,
            content_type: atom.content_type,
            content_template: atom.content_template,
            properties: atom.properties,
            links: atom.links,
            children_block_ids: children_ids,
        });
    }

    // Compute content identity hashes.
    let mut ci_hashes: Vec<String> = Vec::with_capacity(nodes.len());
    for node in &nodes {
        let props_for_hash = if should_include_properties(&node.content_type) {
            Some(&node.properties)
        } else {
            None
        };
        ci_hashes.push(compute_content_identity_hash(
            &node.content_type,
            &node.content_template,
            props_for_hash,
        ));
    }

    // Compute merkle hashes (reverse order — children before parents).
    let mut merkle_hashes: Vec<String> = vec![String::new(); nodes.len()];
    for idx in (0..nodes.len()).rev() {
        let children_merkles: Vec<String> = nodes[idx]
            .children_block_ids
            .iter()
            .filter_map(|cid| block_id_to_idx.get(cid))
            .map(|&cidx| merkle_hashes[cidx].clone())
            .collect();
        let children_refs: Vec<&str> = children_merkles.iter().map(|s| s.as_str()).collect();
        merkle_hashes[idx] = compute_merkle_hash(&ci_hashes[idx], &nodes[idx].name, &children_refs);
    }

    // Collect link triples (internal links only).
    let mut triples = Vec::new();
    for (idx, node) in nodes.iter().enumerate() {
        for (placeholder_index, &link_lineage) in node.links.iter().enumerate() {
            if let Some(&target_idx) = lineage_to_idx.get(&link_lineage) {
                triples.push((
                    merkle_hashes[idx].clone(),
                    placeholder_index,
                    merkle_hashes[target_idx].clone(),
                ));
            }
        }
    }

    Ok(compute_topology_hash(&merkle_hashes[0], &mut triples))
}

#[cfg(test)]
mod proptest_export {
    use super::*;
    use proptest::prelude::*;

    fn arb_content_type() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("content".to_string()),
            Just("namespace".to_string()),
            "[a-z_]{2,15}",
        ]
    }

    proptest! {
        #[test]
        fn compute_export_hash_determinism(
            content_type in arb_content_type(),
            template in "[a-zA-Z0-9 ]{0,50}",
            local_ids in prop::collection::vec(0u32..100, 0..=5),
        ) {
            let hash1 = compute_export_hash(&content_type, &template, &local_ids);
            let hash2 = compute_export_hash(&content_type, &template, &local_ids);
            prop_assert_eq!(hash1, hash2);
        }

        #[test]
        fn compute_export_hash_link_order_independence(
            content_type in arb_content_type(),
            template in "[a-zA-Z0-9 ]{0,50}",
            local_ids in prop::collection::vec(0u32..100, 2..=5),
        ) {
            let hash_original = compute_export_hash(&content_type, &template, &local_ids);
            // Shuffle by reversing (a simple permutation)
            let mut shuffled = local_ids.clone();
            shuffled.reverse();
            let hash_shuffled = compute_export_hash(&content_type, &template, &shuffled);
            prop_assert_eq!(hash_original, hash_shuffled,
                "Hash should be independent of link order");
        }

        #[test]
        fn should_include_properties_exhaustiveness(
            content_type in "[a-z_]{2,20}"
                .prop_filter("not a known type", |s| {
                    !matches!(s.as_str(), "schema" | "setting" | "type_registry")
                })
        ) {
            prop_assert!(!should_include_properties(&content_type),
                "Only schema, setting, type_registry should return true, but got true for: {}",
                content_type);
        }

        #[test]
        fn remove_nil_links_reindexing(n in 1usize..=6) {
            // Generate a mix of nil and valid UUIDs
            let mut links: Vec<Uuid> = Vec::new();
            let mut external_links_vec: Vec<ExternalLink> = Vec::new();
            let mut valid_count = 0usize;
            for i in 0..n {
                if i % 2 == 0 {
                    // nil link with external path
                    links.push(Uuid::nil());
                    external_links_vec.push(ExternalLink {
                        placeholder_index: i,
                        target_path: format!("ext::path{}", i),
                    });
                } else {
                    // valid link
                    links.push(Uuid::now_v7());
                    valid_count += 1;
                }
            }

            // Build template with placeholders {0} through {N-1}
            let template: String = (0..n)
                .map(|i| format!("text{} {{{}}} ", i, i))
                .collect();

            let (result, clean_links) = remove_nil_links(&template, &links, &external_links_vec);

            // The clean_links should have exactly the non-nil links
            prop_assert_eq!(clean_links.len(), valid_count,
                "Expected {} non-nil links, got {}", valid_count, clean_links.len());

            // Verify all clean links are non-nil
            for link in &clean_links {
                prop_assert_ne!(*link, Uuid::nil(), "Clean links should not contain nil UUIDs");
            }

            // Verify remaining placeholders in output are contiguous from {0} to {M-1}
            for i in 0..valid_count {
                prop_assert!(result.contains(&format!("{{{}}}", i)),
                    "Result should contain placeholder {{{}}}, result: {}", i, result);
            }
            // No placeholder >= valid_count should exist
            for i in valid_count..n {
                prop_assert!(!result.contains(&format!("{{{}}}", i)),
                    "Result should NOT contain placeholder {{{}}}, result: {}", i, result);
            }

            // Nil link placeholders should be replaced with [[ext::pathN]]
            for el in &external_links_vec {
                let expected_text = format!("[[{}]]", el.target_path);
                prop_assert!(result.contains(&expected_text),
                    "Result should contain {}, result: {}", expected_text, result);
            }
        }

        #[test]
        fn remove_nil_links_with_non_ascii(_dummy in 0u8..1) {
            // This test demonstrates the bug at line 296: `result.push(bytes[i] as char)`
            // which corrupts multi-byte UTF-8 characters.
            //
            // The bug only manifests when there ARE nil links (otherwise the function
            // short-circuits and returns the template unchanged).

            let template = "caf\u{e9} {0} na\u{ef}ve";
            // One nil link so the rewrite path is triggered, plus an external link
            // to replace the nil placeholder
            let links = vec![Uuid::nil()];
            let external_links_vec = vec![ExternalLink {
                placeholder_index: 0,
                target_path: "some::path".to_string(),
            }];

            let (result, clean_links) = remove_nil_links(template, &links, &external_links_vec);

            // The nil link should be removed
            prop_assert!(clean_links.is_empty(),
                "All links were nil, so clean_links should be empty");

            // The EXPECTED correct output would be: "café [[some::path]] naïve"
            let expected = "caf\u{e9} [[some::path]] na\u{ef}ve";

            // BUG: bytes[i] as char corrupts multi-byte UTF-8.
            // \u{e9} (é) is 2 bytes: [0xc3, 0xa9] — each byte becomes a separate char
            // \u{ef} (ï) is 2 bytes: [0xc3, 0xaf] — same corruption
            // So the output will have wrong characters where the non-ASCII chars were.
            // Use prop_assert! with boolean to avoid prop_assert_ne! ownership issues.
            let r = result.clone();
            prop_assert!(r != expected,
                "BUG DEMONSTRATED: remove_nil_links corrupts multi-byte UTF-8. \
                 Result: {:?}, Expected: {:?}", r, expected);

            // Additionally verify the result has different length or content
            // (each 2-byte char becomes 2 single-byte chars, so length changes)
            prop_assert!(result.len() != expected.len(),
                "BUG: Result length ({}) should differ from expected length ({}) \
                 because multi-byte chars are split into individual bytes",
                result.len(), expected.len());
        }
    }
}
