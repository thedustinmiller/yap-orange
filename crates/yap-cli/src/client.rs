//! HTTP client for the yap-orange API
//!
//! Provides typed methods for all server endpoints.

use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use yap_core::export::{ExportTree, ImportResult};

fn append_import_params(
    url: &mut String,
    match_by: Option<&str>,
    global_link: bool,
    replace: bool,
) {
    if let Some(mb) = match_by {
        url.push_str(&format!("&match_by={}", mb));
    }
    if global_link {
        url.push_str("&global_link=true");
    }
    if replace {
        url.push_str("&replace=true");
    }
}

/// API client for the yap-orange server
pub struct ApiClient {
    client: Client,
    base_url: String,
}

#[allow(dead_code)]
impl ApiClient {
    /// Create a new API client
    pub fn new(base_url: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Make a GET request and deserialize the response
    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body: ErrorResponse = response.json().await.unwrap_or(ErrorResponse {
                error: "Unknown error".to_string(),
            });
            return Err(anyhow!("{}: {}", status, body.error));
        }

        response.json().await.context("Failed to parse response")
    }

    /// Make a POST request with a JSON body
    async fn post<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .post(&url)
            .json(body)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body: ErrorResponse = response.json().await.unwrap_or(ErrorResponse {
                error: "Unknown error".to_string(),
            });
            return Err(anyhow!("{}: {}", status, body.error));
        }

        response.json().await.context("Failed to parse response")
    }

    /// Make a PUT request with a JSON body
    async fn put<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .put(&url)
            .json(body)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body: ErrorResponse = response.json().await.unwrap_or(ErrorResponse {
                error: "Unknown error".to_string(),
            });
            return Err(anyhow!("{}: {}", status, body.error));
        }

        response.json().await.context("Failed to parse response")
    }

    /// Make a DELETE request
    async fn delete(&self, path: &str) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .delete(&url)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body: ErrorResponse = response.json().await.unwrap_or(ErrorResponse {
                error: "Unknown error".to_string(),
            });
            return Err(anyhow!("{}: {}", status, body.error));
        }

        Ok(())
    }

    // =========================================================================
    // Health
    // =========================================================================

    /// Check server health
    pub async fn health(&self) -> Result<HealthResponse> {
        self.get("/health").await
    }

    // =========================================================================
    // Atom Endpoints
    // =========================================================================

    /// Get an atom by ID (raw, with template and links array)
    pub async fn get_atom(&self, id: Uuid) -> Result<AtomResponse> {
        self.get(&format!("/api/atoms/{}", id)).await
    }

    /// Get an atom with content rendered (links resolved to paths)
    pub async fn get_atom_rendered(&self, id: Uuid) -> Result<AtomRenderedResponse> {
        self.get(&format!("/api/atoms/{}/rendered", id)).await
    }

    /// Update an atom's content
    pub async fn update_atom(&self, id: Uuid, content: &str) -> Result<AtomResponse> {
        let request = UpdateAtomRequest {
            content: content.to_string(),
            content_type: None,
            properties: None,
        };
        self.put(&format!("/api/atoms/{}", id), &request).await
    }

    /// Update an atom's content, type, and properties
    pub async fn update_atom_full(
        &self,
        id: Uuid,
        content: &str,
        content_type: Option<&str>,
        properties: Option<serde_json::Value>,
    ) -> Result<AtomResponse> {
        let request = UpdateAtomRequest {
            content: content.to_string(),
            content_type: content_type.map(String::from),
            properties,
        };
        self.put(&format!("/api/atoms/{}", id), &request).await
    }

    /// Get atoms that link to this atom (via links array)
    pub async fn get_backlinks(&self, id: Uuid) -> Result<Vec<BacklinkResponse>> {
        self.get(&format!("/api/atoms/{}/backlinks", id)).await
    }

    /// Get atoms with edges pointing to this atom
    pub async fn get_references(&self, id: Uuid) -> Result<Vec<BacklinkResponse>> {
        self.get(&format!("/api/atoms/{}/references", id)).await
    }

    /// Get full graph neighborhood for an atom
    pub async fn get_graph(&self, id: Uuid) -> Result<GraphResponse> {
        self.get(&format!("/api/atoms/{}/graph", id)).await
    }

    /// Get all edges for an atom (both directions)
    pub async fn get_atom_edges(&self, id: Uuid) -> Result<EdgesResponse> {
        self.get(&format!("/api/atoms/{}/edges", id)).await
    }

    // =========================================================================
    // Block Endpoints
    // =========================================================================

    /// Create a new block
    pub async fn create_block(
        &self,
        namespace: &str,
        name: &str,
        content: &str,
        content_type: &str,
        properties: Option<serde_json::Value>,
    ) -> Result<CreateBlockResponse> {
        let request = CreateBlockRequest {
            namespace: namespace.to_string(),
            name: name.to_string(),
            content: content.to_string(),
            content_type: content_type.to_string(),
            properties: properties.unwrap_or_else(|| serde_json::json!({})),
        };
        self.post("/api/blocks", &request).await
    }

    /// Get a block by ID
    pub async fn get_block(&self, id: Uuid) -> Result<BlockResponse> {
        self.get(&format!("/api/blocks/{}", id)).await
    }

    /// List blocks (optionally filtered by namespace)
    pub async fn list_blocks(&self, namespace: Option<&str>) -> Result<Vec<BlockResponse>> {
        let path = match namespace {
            Some(ns) => format!("/api/blocks?namespace={}", urlencoding::encode(ns)),
            None => "/api/blocks".to_string(),
        };
        self.get(&path).await
    }

    /// Search blocks by name or namespace path
    pub async fn search_blocks(&self, query: &str) -> Result<Vec<BlockResponse>> {
        self.get(&format!(
            "/api/blocks?search={}",
            urlencoding::encode(query)
        ))
        .await
    }

    /// List orphaned blocks
    pub async fn list_orphans(&self) -> Result<Vec<BlockResponse>> {
        self.get("/api/blocks/orphans").await
    }

    /// Get child blocks
    pub async fn get_children(&self, id: Uuid) -> Result<Vec<BlockResponse>> {
        self.get(&format!("/api/blocks/{}/children", id)).await
    }

    /// List blocks by lineage ID
    pub async fn list_blocks_by_lineage_id(&self, id: Uuid) -> Result<Vec<BlockResponse>> {
        self.get(&format!("/api/blocks?lineage_id={}", id)).await
    }

    /// Get property keys for a block's atom
    pub async fn get_property_keys(&self, id: Uuid) -> Result<Vec<String>> {
        self.get(&format!("/api/blocks/{}/property-keys", id)).await
    }

    /// Update block metadata
    pub async fn update_block(
        &self,
        id: Uuid,
        name: Option<&str>,
        position: Option<&str>,
    ) -> Result<BlockResponse> {
        let request = UpdateBlockRequest {
            name: name.map(String::from),
            position: position.map(String::from),
        };
        self.put(&format!("/api/blocks/{}", id), &request).await
    }

    /// Delete a block (soft delete)
    pub async fn delete_block(&self, id: Uuid) -> Result<()> {
        self.delete(&format!("/api/blocks/{}", id)).await
    }

    /// Delete a block and all its descendants recursively
    pub async fn delete_block_recursive(&self, id: Uuid) -> Result<()> {
        self.delete(&format!("/api/blocks/{}/recursive", id)).await
    }

    /// Restore a soft-deleted block
    pub async fn restore_block(&self, id: Uuid) -> Result<BlockResponse> {
        self.post(&format!("/api/blocks/{}/restore", id), &()).await
    }

    /// Restore a soft-deleted block and all its descendants
    pub async fn restore_block_recursive(&self, id: Uuid) -> Result<serde_json::Value> {
        self.post(&format!("/api/blocks/{}/restore-recursive", id), &())
            .await
    }

    /// Move a block to a new parent
    pub async fn move_block(
        &self,
        id: Uuid,
        parent_id: Option<Uuid>,
        position: Option<&str>,
    ) -> Result<BlockResponse> {
        let request = MoveBlockRequest {
            parent_id,
            position: position.map(String::from),
        };
        self.post(&format!("/api/blocks/{}/move", id), &request)
            .await
    }

    // =========================================================================
    // Edge Endpoints
    // =========================================================================

    /// Create an edge between atoms
    pub async fn create_edge(
        &self,
        from_lineage_id: Uuid,
        to_lineage_id: Uuid,
        edge_type: &str,
        properties: serde_json::Value,
    ) -> Result<EdgeResponse> {
        let request = CreateEdgeRequest {
            from_lineage_id,
            to_lineage_id,
            edge_type: edge_type.to_string(),
            properties,
        };
        self.post("/api/edges", &request).await
    }

    /// Delete an edge
    pub async fn delete_edge(&self, id: Uuid) -> Result<()> {
        self.delete(&format!("/api/edges/{}", id)).await
    }

    // =========================================================================
    // Root Block Endpoints
    // =========================================================================

    /// List all root-level blocks
    pub async fn list_roots(&self) -> Result<Vec<NamespaceResponse>> {
        self.get("/api/blocks/roots").await
    }

    /// Create a namespace (by creating a namespace-type block)
    pub async fn create_namespace(&self, path: &str) -> Result<CreateBlockResponse> {
        // Extract name from path
        let segments: Vec<&str> = path.split("::").collect();
        let name = segments.last().unwrap_or(&path);

        // Get parent namespace (all but last segment)
        let parent_namespace = if segments.len() > 1 {
            segments[..segments.len() - 1].join("::")
        } else {
            String::new()
        };

        let request = CreateBlockRequest {
            namespace: parent_namespace,
            name: name.to_string(),
            content: String::new(),
            content_type: String::new(), // Empty - "namespace" is implicit via children
            properties: serde_json::json!({}),
        };
        self.post("/api/blocks", &request).await
    }

    // =========================================================================
    // Schema Endpoints
    // =========================================================================

    /// List all schema blocks
    pub async fn list_schemas(&self) -> Result<Vec<SchemaResponse>> {
        self.get("/api/schemas").await
    }

    /// Resolve a schema type name with optional namespace context
    pub async fn resolve_schema(
        &self,
        type_name: &str,
        from_namespace: Option<&str>,
    ) -> Result<SchemaResponse> {
        let request = ResolveSchemaRequest {
            type_name: type_name.to_string(),
            from_namespace: from_namespace.map(String::from),
        };
        self.post("/api/schemas/resolve", &request).await
    }

    /// List blocks by content type
    pub async fn list_blocks_by_content_type(
        &self,
        content_type: &str,
    ) -> Result<Vec<BlockResponse>> {
        self.get(&format!(
            "/api/blocks?content_type={}",
            urlencoding::encode(content_type)
        ))
        .await
    }

    // =========================================================================
    // Link Resolution
    // =========================================================================

    /// Resolve a link path to an atom ID
    pub async fn resolve_link(
        &self,
        path: &str,
        from_namespace: Option<&str>,
    ) -> Result<ResolveResponse> {
        let request = ResolveRequest {
            path: path.to_string(),
            from_namespace: from_namespace.map(String::from),
        };
        self.post("/api/resolve", &request).await
    }

    // =========================================================================
    // Export / Import Endpoints
    // =========================================================================

    /// Export a block subtree to portable JSON format.
    pub async fn export_tree(
        &self,
        block_id: Uuid,
        include_keys: Option<&str>,
    ) -> Result<ExportTree> {
        let mut url = format!("/api/blocks/{}/export", block_id);
        if let Some(keys) = include_keys {
            url.push_str(&format!("?include_keys={}", urlencoding::encode(keys)));
        }
        self.get(&url).await
    }

    /// Import a subtree under `parent_block_id`.
    ///
    /// `mode` must be `"merge"` (default) or `"copy"`.
    /// `match_by` controls the dedup strategy: "export_hash", "content_identity", "merkle", "topology".
    pub async fn import_tree(
        &self,
        parent_id: Uuid,
        tree: &ExportTree,
        mode: &str,
        match_by: Option<&str>,
        global_link: bool,
        replace: bool,
    ) -> Result<ImportResult> {
        let mut url = format!("/api/blocks/{}/import?mode={}", parent_id, mode);
        append_import_params(&mut url, match_by, global_link, replace);
        self.post(&url, tree).await
    }

    /// Import a subtree at root level (no parent block).
    pub async fn import_tree_at_root(
        &self,
        tree: &ExportTree,
        mode: &str,
        match_by: Option<&str>,
        global_link: bool,
        replace: bool,
    ) -> Result<ImportResult> {
        let mut url = format!("/api/import?mode={}", mode);
        append_import_params(&mut url, match_by, global_link, replace);
        self.post(&url, tree).await
    }

    // =========================================================================
    // Graph Endpoints
    // =========================================================================

    /// Get subtree graph data for a set of lineage IDs
    pub async fn get_subtree_graph(&self, lineage_ids: &[Uuid]) -> Result<SubtreeGraphResponse> {
        self.post(
            "/api/graph/subtree",
            &serde_json::json!({ "lineage_ids": lineage_ids }),
        )
        .await
    }

    // =========================================================================
    // Debug Endpoints
    // =========================================================================

    /// Fetch recent server log entries
    pub async fn get_debug_logs(&self, since: Option<u64>) -> Result<Vec<LogEntry>> {
        let path = match since {
            Some(n) => format!("/api/debug/logs?since={}", n),
            None => "/api/debug/logs".to_string(),
        };
        self.get(&path).await
    }

    /// Run performance benchmarks
    pub async fn run_benchmarks(
        &self,
        suites: Option<&str>,
        seed: Option<u64>,
    ) -> Result<serde_json::Value> {
        let mut body = serde_json::Map::new();
        if let Some(s) = suites {
            let suite_list: Vec<&str> = s.split(',').map(|s| s.trim()).collect();
            body.insert(
                "suites".to_string(),
                serde_json::Value::Array(
                    suite_list
                        .into_iter()
                        .map(|s| serde_json::Value::String(s.to_string()))
                        .collect(),
                ),
            );
        }
        if let Some(seed) = seed {
            body.insert(
                "seed".to_string(),
                serde_json::Value::Number(serde_json::Number::from(seed)),
            );
        }
        self.post("/api/debug/benchmarks", &serde_json::Value::Object(body))
            .await
    }
}

// =============================================================================
// Request Types
// =============================================================================

#[derive(Serialize)]
struct UpdateAtomRequest {
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct CreateBlockRequest {
    namespace: String,
    name: String,
    content: String,
    content_type: String,
    properties: serde_json::Value,
}

#[derive(Serialize)]
struct UpdateBlockRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<String>,
}

#[derive(Serialize)]
struct MoveBlockRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<String>,
}

#[derive(Serialize)]
struct CreateEdgeRequest {
    from_lineage_id: Uuid,
    to_lineage_id: Uuid,
    edge_type: String,
    properties: serde_json::Value,
}

#[derive(Serialize)]
struct ResolveRequest {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    from_namespace: Option<String>,
}

/// Request to resolve a schema type name
#[derive(Debug, Serialize)]
pub struct ResolveSchemaRequest {
    pub type_name: String,
    pub from_namespace: Option<String>,
}

// =============================================================================
// Response Types
// =============================================================================

#[derive(Deserialize, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Deserialize, Serialize)]
#[allow(dead_code)]
pub struct HealthResponse {
    pub status: String,
    pub database: String,
}

/// Raw atom response (with template and links array)
#[derive(Deserialize, Serialize)]
pub struct AtomResponse {
    pub id: Uuid,
    pub lineage_id: Uuid,
    pub content_type: String,
    pub content_template: String,
    pub links: Vec<Uuid>,
    pub properties: serde_json::Value,
    pub content_hash: String,
    pub predecessor_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Rendered atom response (with content instead of template)
#[derive(Deserialize, Serialize)]
pub struct AtomRenderedResponse {
    pub id: Uuid,
    pub lineage_id: Uuid,
    pub content_type: String,
    pub content: String,
    pub properties: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Block response with rendered atom content
#[derive(Deserialize, Serialize)]
pub struct BlockResponse {
    pub id: Uuid,
    pub lineage_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub namespace: String,
    pub name: String,
    pub position: String,
    pub content: String,
    pub content_type: String,
    pub properties: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Block creation response
#[derive(Deserialize, Serialize)]
pub struct CreateBlockResponse {
    pub block_id: Uuid,
    pub lineage_id: Uuid,
    pub namespace: String,
    pub name: String,
}

/// Backlink response
#[derive(Deserialize, Serialize)]
pub struct BacklinkResponse {
    pub lineage_id: Uuid,
    pub content: String,
    pub content_type: String,
    pub namespace: Option<String>,
}

/// Edge response
#[derive(Deserialize, Serialize)]
pub struct EdgeResponse {
    pub id: Uuid,
    pub from_lineage_id: Uuid,
    pub to_lineage_id: Uuid,
    pub edge_type: String,
    pub properties: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Edges list response (grouped by direction)
#[derive(Deserialize, Serialize)]
pub struct EdgesResponse {
    pub outgoing: Vec<EdgeResponse>,
    pub incoming: Vec<EdgeResponse>,
}

/// Graph neighborhood response
#[derive(Deserialize, Serialize)]
pub struct GraphResponse {
    pub atom: AtomRenderedResponse,
    pub backlinks: Vec<BacklinkResponse>,
    pub outlinks: Vec<BacklinkResponse>,
    pub edges: EdgesResponse,
}

/// Namespace response
#[derive(Deserialize, Serialize)]
pub struct NamespaceResponse {
    pub id: Uuid,
    pub namespace: String,
    pub name: String,
    pub lineage_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub position: String,
}

/// Path resolution response
#[derive(Deserialize, Serialize)]
pub struct ResolveResponse {
    pub lineage_id: Uuid,
    pub block_id: Uuid,
    pub namespace: String,
}

/// Schema block response with field definitions
#[derive(Debug, Deserialize, Serialize)]
pub struct SchemaResponse {
    pub block_id: Uuid,
    pub lineage_id: Uuid,
    pub namespace: String,
    pub name: String,
    pub version: i32,
    pub fields: serde_json::Value,
    pub content: String,
}

/// Subtree graph response
#[derive(Deserialize, Serialize)]
pub struct SubtreeGraphResponse {
    pub content_links: Vec<ContentLinkResponse>,
    pub edges: Vec<EdgeResponse>,
}

/// Content link (wiki-link) between lineages
#[derive(Deserialize, Serialize)]
pub struct ContentLinkResponse {
    pub from_lineage_id: Uuid,
    pub to_lineage_id: Uuid,
}

/// Server log entry
#[derive(Deserialize, Serialize)]
pub struct LogEntry {
    pub id: u64,
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}
