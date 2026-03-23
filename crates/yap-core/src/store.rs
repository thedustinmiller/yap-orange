//! Database-agnostic storage trait.
//!
//! All database operations are expressed through `Store`. Required methods
//! contain backend-specific SQL; default implementations compose from those
//! required methods and are therefore backend-agnostic.

use async_trait::async_trait;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::models::{Atom, Backlink, Block, CreateAtom, CreateEdge, Edge, Lineage, UpdateBlock};

/// Information needed to render a link in the editor.
pub struct LinkDisplayInfo {
    /// Canonical namespace path (e.g. `research::ml::attention`)
    pub namespace: String,
    /// Content type of the atom (e.g. `"content"`, `"schema"`, or a custom type name)
    pub content_type: String,
    /// Atom properties
    pub properties: serde_json::Value,
    /// Lineage ID of the atom
    pub lineage_id: Uuid,
}

/// Backend-agnostic storage interface.
///
/// Implement the ~35 required methods (SQL per-backend) to get the remaining
/// ~11 default implementations for free.
#[async_trait]
pub trait Store: Send + Sync {
    // =========================================================================
    // Health
    // =========================================================================

    async fn health_check(&self) -> Result<bool>;

    // =========================================================================
    // Admin
    // =========================================================================

    /// True if the store has no data (atoms table is empty).
    async fn is_empty(&self) -> Result<bool>;

    /// Delete all rows from all data tables. Preserves schema.
    async fn clear_all_data(&self) -> Result<()>;

    // =========================================================================
    // Namespace / Path helpers — Required (SQL)
    // =========================================================================

    /// Compute the display namespace for a block by walking the parent chain.
    async fn compute_namespace(&self, block_id: Uuid) -> Result<String>;

    /// Find a block by parent_id + name.
    async fn find_block_by_parent_and_name(
        &self,
        parent_id: Option<Uuid>,
        name: &str,
    ) -> Result<Option<Block>>;

    /// Get the next fractional position after the last child of a parent.
    async fn get_next_position(&self, parent_id: Option<Uuid>) -> Result<String>;

    // =========================================================================
    // Atom + Lineage — Required (SQL)
    // =========================================================================

    async fn create_atom(&self, create: &CreateAtom) -> Result<(Atom, Lineage)>;

    /// Get the current atom snapshot for a lineage (excludes soft-deleted lineages).
    async fn get_atom(&self, lineage_id: Uuid) -> Result<Atom>;

    /// Get a specific atom snapshot by its own ID (not lineage ID).
    /// Used for pinned schema version lookups.
    async fn get_atom_by_id(&self, atom_id: Uuid) -> Result<Atom>;

    /// Get the lineage record (excludes soft-deleted).
    async fn get_lineage(&self, lineage_id: Uuid) -> Result<Lineage>;

    /// Get a lineage by ID (includes soft-deleted).
    async fn get_lineage_with_deleted(&self, lineage_id: Uuid) -> Result<Lineage>;

    /// Create a new immutable atom snapshot and advance the lineage pointer.
    async fn edit_lineage(
        &self,
        lineage_id: Uuid,
        content_type: &str,
        content_template: &str,
        links: &[Uuid],
        properties: &serde_json::Value,
    ) -> Result<(Atom, Lineage)>;

    /// Soft delete a lineage.
    async fn delete_lineage(&self, lineage_id: Uuid) -> Result<Lineage>;

    // =========================================================================
    // Block — Required (SQL)
    // =========================================================================

    /// Create a new block with pre-serialized content (template + links).
    ///
    /// Parameter order: parent_id, name, content_template, links, content_type, properties.
    async fn create_block_with_content(
        &self,
        parent_id: Option<Uuid>,
        name: &str,
        content_template: &str,
        links: &[Uuid],
        content_type: &str,
        properties: &serde_json::Value,
    ) -> Result<(Block, Atom)>;

    /// Get a block by ID (excludes soft-deleted).
    async fn get_block(&self, id: Uuid) -> Result<Block>;

    /// Get a block by ID (includes soft-deleted).
    async fn get_block_with_deleted(&self, id: Uuid) -> Result<Block>;

    /// Update a block's metadata (name and/or position).
    async fn update_block(&self, id: Uuid, update: &UpdateBlock) -> Result<Block>;

    /// Soft delete a block.
    async fn delete_block(&self, id: Uuid) -> Result<Block>;

    /// Soft delete a block and all its descendants recursively.
    async fn delete_block_recursive(&self, id: Uuid) -> Result<u64>;

    /// Restore a soft-deleted block.
    async fn restore_block(&self, id: Uuid) -> Result<Block>;

    /// Restore a soft-deleted block and all its descendants recursively.
    async fn restore_block_recursive(&self, id: Uuid) -> Result<u64>;

    /// Get direct child blocks of a parent.
    async fn get_block_children(&self, parent_id: Uuid) -> Result<Vec<Block>>;

    /// Get root blocks (parent_id IS NULL).
    async fn get_root_blocks(&self) -> Result<Vec<Block>>;

    /// List all blocks under a namespace prefix (recursive subtree).
    async fn list_blocks_by_namespace(&self, namespace_prefix: &str) -> Result<Vec<Block>>;

    /// List orphaned blocks (non-null parent_id but parent is deleted or missing).
    async fn list_orphaned_blocks(&self) -> Result<Vec<Block>>;

    /// Search blocks by name or namespace.
    async fn search_blocks(&self, query: &str) -> Result<Vec<Block>>;

    /// List blocks whose current atom has a given content_type.
    async fn list_blocks_by_content_type(&self, content_type: &str) -> Result<Vec<Block>>;

    /// Move a block to a new parent.
    async fn move_block(
        &self,
        block_id: Uuid,
        new_parent_id: Option<Uuid>,
        new_position: Option<String>,
    ) -> Result<Block>;

    /// Check if moving a block would create a cycle in the hierarchy.
    async fn is_move_safe(&self, block_id: Uuid, new_parent_id: Option<Uuid>) -> Result<bool>;

    /// Get all blocks referencing a lineage.
    async fn get_blocks_for_lineage(&self, lineage_id: Uuid) -> Result<Vec<Block>>;

    /// List all distinct property keys across atoms in a block subtree.
    async fn list_property_keys_in_subtree(&self, block_id: Uuid) -> Result<Vec<String>>;

    /// Create a block pointing to an existing lineage (hard link).
    /// Name comes from the atom's properties.
    async fn create_block_for_lineage(
        &self,
        parent_id: Option<Uuid>,
        lineage_id: Uuid,
    ) -> Result<Block>;

    /// Find lineages whose current atom has a given content_hash.
    async fn find_lineages_by_content_hash(&self, content_hash: &str) -> Result<Vec<Uuid>>;

    // =========================================================================
    // Edge — Required (SQL)
    // =========================================================================

    async fn create_edge(&self, create: &CreateEdge) -> Result<Edge>;

    async fn get_edge(&self, id: Uuid) -> Result<Edge>;

    async fn delete_edge(&self, id: Uuid) -> Result<Edge>;

    async fn get_edges_from(&self, lineage_id: Uuid) -> Result<Vec<Edge>>;

    async fn get_edges_to(&self, lineage_id: Uuid) -> Result<Vec<Edge>>;

    /// Get all edges for a lineage (both directions).
    async fn get_all_edges(&self, lineage_id: Uuid) -> Result<Vec<Edge>>;

    /// Get semantic edges where both endpoints are in the given set.
    async fn get_edges_between(&self, lineage_ids: &[Uuid]) -> Result<Vec<Edge>>;

    /// Get content links where both source and target lineage are in the given set.
    /// Returns `(from_lineage_id, to_lineage_id)` pairs.
    async fn get_content_links_between(&self, lineage_ids: &[Uuid]) -> Result<Vec<(Uuid, Uuid)>>;

    // =========================================================================
    // Graph / Link — Required (SQL)
    // =========================================================================

    /// Get lineages that link to a target lineage via their links array.
    /// Returns `Backlink` which includes both the stable lineage ID and the current atom.
    async fn get_backlinks(&self, target_lineage_id: Uuid) -> Result<Vec<Backlink>>;

    /// Count atoms that link to a target lineage.
    async fn count_backlinks(&self, target_lineage_id: Uuid) -> Result<i64>;

    // =========================================================================
    // Default implementations — compose from required methods
    // =========================================================================

    /// Get a block with its associated current atom.
    async fn get_block_with_atom(&self, id: Uuid) -> Result<(Block, Atom)> {
        let block = self.get_block(id).await?;
        let atom = self.get_atom(block.lineage_id).await?;
        Ok((block, atom))
    }

    /// Find a block by walking a namespace path segment by segment.
    async fn find_block_by_namespace(&self, namespace: &str) -> Result<Option<Block>> {
        if namespace.is_empty() {
            return Ok(None);
        }
        let segments: Vec<&str> = namespace.split("::").collect();
        let mut current_parent_id: Option<Uuid> = None;

        for (i, segment) in segments.iter().enumerate() {
            let block = self
                .find_block_by_parent_and_name(current_parent_id, segment)
                .await?;
            match block {
                Some(b) => {
                    if i == segments.len() - 1 {
                        return Ok(Some(b));
                    }
                    current_parent_id = Some(b.id);
                }
                None => return Ok(None),
            }
        }
        Ok(None)
    }

    /// Resolve a namespace path string to the lineage ID at that path.
    async fn resolve_namespace_to_lineage(&self, namespace: &str) -> Result<Option<Uuid>> {
        let block = self.find_block_by_namespace(namespace).await?;
        Ok(block.map(|b| b.lineage_id))
    }

    /// Resolve a parsed wiki-link to a lineage ID.
    async fn resolve_link_to_lineage(
        &self,
        segments: &[String],
        is_relative: bool,
        levels_up: usize,
        context_namespace: Option<&str>,
    ) -> Result<Option<Uuid>> {
        use crate::links::{format_namespace, resolve_path};
        let resolved_segments = resolve_path(segments, is_relative, levels_up, context_namespace);
        match resolved_segments {
            Some(segs) if !segs.is_empty() => {
                let namespace = format_namespace(&segs);
                self.resolve_namespace_to_lineage(&namespace).await
            }
            _ => Ok(None),
        }
    }

    /// Get display info for a lineage — namespace + content type + properties.
    ///
    /// Used during deserialization to render `[[path]]` wiki links.
    async fn get_link_display_info(&self, lineage_id: Uuid) -> Result<Option<LinkDisplayInfo>> {
        let blocks = self.get_blocks_for_lineage(lineage_id).await?;
        match blocks.first() {
            Some(b) => {
                let namespace = self.compute_namespace(b.id).await?;
                let atom = self.get_atom(lineage_id).await?;
                Ok(Some(LinkDisplayInfo {
                    namespace,
                    content_type: atom.content_type,
                    properties: atom.properties,
                    lineage_id,
                }))
            }
            None => Ok(None),
        }
    }

    /// Get the canonical display path for a lineage.
    ///
    /// Returns the namespace of the earliest-created block referencing the lineage.
    async fn get_canonical_path(&self, lineage_id: Uuid) -> Result<Option<String>> {
        let blocks = self.get_blocks_for_lineage(lineage_id).await?;
        match blocks.first() {
            Some(b) => {
                let display = self.compute_namespace(b.id).await?;
                Ok(Some(display))
            }
            None => Ok(None),
        }
    }

    /// Ensure a namespace hierarchy exists, creating any missing levels.
    ///
    /// Returns `(block_id, was_created)` for each path segment.
    async fn ensure_namespace(&self, namespace: &str) -> Result<Vec<(Uuid, bool)>> {
        if namespace.is_empty() {
            return Ok(Vec::new());
        }
        let segments: Vec<&str> = namespace.split("::").collect();
        let mut results: Vec<(Uuid, bool)> = Vec::new();
        let mut current_parent_id: Option<Uuid> = None;

        for segment in &segments {
            if let Some(existing) = self
                .find_block_by_parent_and_name(current_parent_id, segment)
                .await?
            {
                results.push((existing.id, false));
                current_parent_id = Some(existing.id);
            } else {
                match self
                    .create_block(
                        current_parent_id,
                        segment,
                        "",
                        "namespace",
                        &serde_json::json!({}),
                    )
                    .await
                {
                    Ok((block, _)) => {
                        results.push((block.id, true));
                        current_parent_id = Some(block.id);
                    }
                    // Race condition: another writer beat us, find the winner
                    Err(Error::Conflict(_)) => {
                        if let Some(existing) = self
                            .find_block_by_parent_and_name(current_parent_id, segment)
                            .await?
                        {
                            results.push((existing.id, false));
                            current_parent_id = Some(existing.id);
                        } else {
                            return Err(Error::Internal(format!(
                                "Conflict creating '{}' but not found on retry",
                                segment
                            )));
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        Ok(results)
    }

    /// Create a namespace hierarchy, returning just the block IDs.
    async fn create_namespace(&self, namespace: &str) -> Result<Vec<Uuid>> {
        let results = self.ensure_namespace(namespace).await?;
        Ok(results.into_iter().map(|(id, _)| id).collect())
    }

    /// Get the leaf block ID for a namespace, creating any missing levels.
    async fn ensure_namespace_block(&self, namespace: &str) -> Result<Uuid> {
        let results = self.ensure_namespace(namespace).await?;
        results
            .last()
            .map(|(id, _)| *id)
            .ok_or_else(|| Error::InvalidInput("Empty namespace".to_string()))
    }

    /// List schema blocks (content_type = "schema").
    async fn list_schemas(&self) -> Result<Vec<Block>> {
        self.list_blocks_by_content_type("schema").await
    }

    /// Resolve a type name to its schema block, walking up namespaces.
    async fn resolve_schema(
        &self,
        type_name: &str,
        context_namespace: Option<&str>,
    ) -> Result<Option<Block>> {
        if type_name.contains("::") {
            return self.find_block_by_namespace(type_name).await;
        }

        let mut candidates: Vec<String> = Vec::new();
        if let Some(ns) = context_namespace {
            let segments: Vec<&str> = ns.split("::").collect();
            for i in (0..=segments.len()).rev() {
                let prefix = if i == 0 {
                    String::new()
                } else {
                    segments[..i].join("::")
                };
                let candidate = if prefix.is_empty() {
                    format!("types::{}", type_name)
                } else {
                    format!("{}::types::{}", prefix, type_name)
                };
                candidates.push(candidate);
            }
        } else {
            candidates.push(format!("types::{}", type_name));
        }

        for candidate in &candidates {
            if let Some(block) = self.find_block_by_namespace(candidate).await? {
                return Ok(Some(block));
            }
        }
        Ok(None)
    }

    /// Create a block from raw editor content (serializes [[wiki links]] before storing).
    ///
    /// Inlines the link-serialization logic from `content::serialize_content` to avoid
    /// the `&Self → &dyn Store` coercion that would require `Self: Sized` in the trait def.
    async fn create_block(
        &self,
        parent_id: Option<Uuid>,
        name: &str,
        content: &str,
        content_type: &str,
        properties: &serde_json::Value,
    ) -> Result<(Block, Atom)> {
        let context_ns = if let Some(pid) = parent_id {
            Some(self.compute_namespace(pid).await?)
        } else {
            None
        };

        // Inline serialize_content: parse [[wiki links]] and resolve each to a lineage ID.
        let parsed_links = crate::links::parse_links(content);
        let (template, links) = if parsed_links.is_empty() {
            (content.to_string(), Vec::new())
        } else {
            let mut tmpl = String::new();
            let mut link_ids: Vec<Uuid> = Vec::new();
            let mut last_end = 0usize;

            for link in &parsed_links {
                tmpl.push_str(&content[last_end..link.start]);
                let resolved = self
                    .resolve_link_to_lineage(
                        &link.segments,
                        link.is_relative,
                        link.parent_levels,
                        context_ns.as_deref(),
                    )
                    .await?;
                match resolved {
                    Some(lineage_id) => {
                        if link.is_embed {
                            tmpl.push_str(&format!("!{{{}}}", link_ids.len()));
                        } else {
                            tmpl.push_str(&format!("{{{}}}", link_ids.len()));
                        }
                        link_ids.push(lineage_id);
                    }
                    None => {
                        tmpl.push_str(&link.original);
                    }
                }
                last_end = link.end;
            }
            tmpl.push_str(&content[last_end..]);
            (tmpl, link_ids)
        };

        self.create_block_with_content(parent_id, name, &template, &links, content_type, properties)
            .await
    }
}
