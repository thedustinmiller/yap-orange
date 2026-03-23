//! API handlers for the server
//!
//! Implements HTTP endpoints for atoms, blocks, and edges.
//! Phase 2.1-2.7 implementation.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::OpenApi;
use uuid::Uuid;

use yap_core::Store;
use yap_core::content::{deserialize_content, serialize_content};
use yap_core::error::Error as CoreError;
use yap_core::export::{
    ExportEdge, ExportNode, ExportOptions, ExportTree, ExternalLink, FailedEdge,
    FailedExternalLink, ImportMode, ImportOptions, ImportResult, InternalLink, MatchStrategy,
};
use yap_core::links::{format_namespace, parse_links, resolve_path};
use yap_core::models::{Atom, Block, CreateEdge, Edge, UpdateBlock};

use crate::AppState;

// =============================================================================
// OpenAPI Document
// =============================================================================

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(
    info(
        title = "yap-orange API",
        version = "0.1.0",
        description = "Hierarchical note-taking system with graph linking"
    ),
    paths(
        health,
        get_atom,
        get_atom_rendered,
        update_atom,
        get_atom_backlinks,
        get_atom_references,
        get_atom_graph,
        get_atom_edges,
        create_block,
        list_blocks,
        list_orphans,
        get_block,
        update_block,
        delete_block,
        delete_block_recursive,
        get_block_children,
        restore_block,
        move_block,
        export_block_tree,
        import_block_tree,
        create_edge,
        delete_edge,
        list_roots,
        resolve_link,
        list_schemas,
        resolve_schema,
        get_debug_logs,
    ),
    components(schemas(
        ErrorResponse,
        HealthResponse,
        AtomResponse,
        AtomRenderedResponse,
        BlockResponse,
        CreateBlockResponse,
        BacklinkResponse,
        EdgeResponse,
        EdgesResponse,
        GraphResponse,
        NamespaceResponse,
        ResolveResponse,
        SchemaResponse,
        UpdateAtomRequest,
        CreateBlockRequest,
        UpdateBlockRequest,
        MoveBlockRequest,
        CreateEdgeRequest,
        ResolveRequest,
        ResolveSchemaRequest,
        ExportTree,
        ExportNode,
        ExportEdge,
        InternalLink,
        ExternalLink,
        ImportResult,
        FailedExternalLink,
        FailedEdge,
        crate::log_buffer::LogEntry,
    )),
    tags(
        (name = "Health", description = "Health check"),
        (name = "Atoms", description = "Atom (content snapshot) endpoints"),
        (name = "Blocks", description = "Block (hierarchy node) endpoints"),
        (name = "Edges", description = "Edge (semantic relationship) endpoints"),
        (name = "Graph", description = "Graph neighborhood and link endpoints"),
        (name = "Export/Import", description = "Tree export and import"),
        (name = "Links", description = "Link resolution"),
        (name = "Schemas", description = "Schema type definitions"),
        (name = "Debug", description = "Debug and development endpoints"),
    )
)]
pub struct ApiDoc;

// =============================================================================
// Error Handling
// =============================================================================

/// API error type that converts to HTTP responses
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn not_found(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: msg.into(),
        }
    }

    fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }

    fn conflict(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: msg.into(),
        }
    }

    fn internal(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: msg.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(ErrorResponse {
            error: self.message,
        });
        (self.status, body).into_response()
    }
}

impl From<CoreError> for ApiError {
    fn from(err: CoreError) -> Self {
        match err {
            CoreError::NotFound(msg) => ApiError::not_found(msg),
            CoreError::InvalidInput(msg) => ApiError::bad_request(msg),
            CoreError::Conflict(msg) => ApiError::conflict(msg),
            CoreError::LinkResolution(msg) => ApiError::bad_request(msg),
            CoreError::Database(msg) => {
                tracing::error!("Database error: {}", msg);
                ApiError::internal(format!("Database error: {}", msg))
            }
            CoreError::Internal(msg) => ApiError::internal(msg),
        }
    }
}

// =============================================================================
// Response Types
// =============================================================================

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct HealthResponse {
    pub status: String,
    pub database: String,
}

/// Raw atom response (with template and links array)
#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AtomResponse {
    pub id: Uuid,
    pub lineage_id: Uuid,
    pub content_type: String,
    pub content_template: String,
    pub links: Vec<Uuid>,
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub properties: serde_json::Value,
    pub content_hash: String,
    pub predecessor_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Rendered atom response (with content instead of template)
#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AtomRenderedResponse {
    pub id: Uuid,
    pub lineage_id: Uuid,
    pub content_type: String,
    pub content: String,
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub properties: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Block response with rendered atom content
#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BlockResponse {
    pub id: Uuid,
    pub lineage_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub namespace: String,
    pub name: String,
    pub position: String,
    pub content: String,
    pub content_type: String,
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub properties: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Block creation response
#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateBlockResponse {
    pub block_id: Uuid,
    pub lineage_id: Uuid,
    pub namespace: String,
    pub name: String,
}

/// Backlink response - lineages that link to a target lineage
#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BacklinkResponse {
    pub lineage_id: Uuid,
    pub content: String,
    pub content_type: String,
    pub namespace: Option<String>,
}

/// Edge response
#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EdgeResponse {
    pub id: Uuid,
    pub from_lineage_id: Uuid,
    pub to_lineage_id: Uuid,
    pub edge_type: String,
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub properties: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<Edge> for EdgeResponse {
    fn from(edge: Edge) -> Self {
        Self {
            id: edge.id,
            from_lineage_id: edge.from_lineage_id,
            to_lineage_id: edge.to_lineage_id,
            edge_type: edge.edge_type,
            properties: edge.properties,
            created_at: edge.created_at,
        }
    }
}

/// Edges list response (grouped by direction)
#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EdgesResponse {
    pub outgoing: Vec<EdgeResponse>,
    pub incoming: Vec<EdgeResponse>,
}

/// Graph neighborhood response
#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GraphResponse {
    pub atom: AtomRenderedResponse,
    pub backlinks: Vec<BacklinkResponse>,
    pub outlinks: Vec<BacklinkResponse>,
    pub edges: EdgesResponse,
    /// Other blocks that share the same lineage (hard links).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hard_links: Vec<HardLinkResponse>,
}

/// A block that shares the same lineage as the current block (hard link).
#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct HardLinkResponse {
    pub block_id: Uuid,
    pub namespace: String,
    pub name: String,
}

/// Subtree graph request: set of lineage IDs to find connections between
#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SubtreeGraphRequest {
    pub lineage_ids: Vec<Uuid>,
}

/// A content link between two lineages
#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ContentLinkResponse {
    pub from_lineage_id: Uuid,
    pub to_lineage_id: Uuid,
}

/// Subtree graph response: connections between a set of lineages
#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SubtreeGraphResponse {
    pub content_links: Vec<ContentLinkResponse>,
    pub edges: Vec<EdgeResponse>,
}

/// Namespace block response
#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NamespaceResponse {
    pub id: Uuid,
    pub namespace: String,
    pub name: String,
    pub lineage_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub position: String,
}

/// Path resolution response
#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ResolveResponse {
    pub lineage_id: Uuid,
    pub block_id: Uuid,
    pub namespace: String,
}

/// Schema block response with field definitions
#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SchemaResponse {
    pub block_id: Uuid,
    pub lineage_id: Uuid,
    pub namespace: String,
    pub name: String,
    pub version: i32,
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub fields: serde_json::Value,
    pub content: String,
}

/// Request to resolve a schema type name
#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ResolveSchemaRequest {
    pub type_name: String,
    #[serde(default)]
    pub from_namespace: Option<String>,
}

// =============================================================================
// Request Types
// =============================================================================

/// Request to update an atom's content
#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateAtomRequest {
    pub content: String,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
    pub properties: Option<serde_json::Value>,
}

/// Request to create a new block
#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateBlockRequest {
    #[serde(default)]
    pub namespace: String,
    pub name: String,
    #[serde(default)]
    pub content: String,
    #[serde(default = "default_content_type")]
    pub content_type: String,
    #[serde(default = "default_properties")]
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub properties: serde_json::Value,
    #[serde(default)]
    pub position: Option<String>,
    /// Optional direct parent block ID. If provided, `namespace` is ignored
    /// and the block is created directly under this parent.
    #[serde(default)]
    pub parent_id: Option<Uuid>,
}

fn default_content_type() -> String {
    "content".to_string()
}

fn default_properties() -> serde_json::Value {
    serde_json::json!({})
}

/// Request to update a block
#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateBlockRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub position: Option<String>,
}

/// Request to move a block
#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MoveBlockRequest {
    pub parent_id: Option<Uuid>,
    #[serde(default)]
    pub position: Option<String>,
}

/// Query parameters for listing blocks
#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct ListBlocksQuery {
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub lineage_id: Option<Uuid>,
    #[serde(default)]
    pub content_type: Option<String>,
}

/// Request to create an edge between lineages
#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateEdgeRequest {
    pub from_lineage_id: Uuid,
    pub to_lineage_id: Uuid,
    pub edge_type: String,
    #[serde(default = "default_properties")]
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub properties: serde_json::Value,
}

/// Request to resolve a link path
#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ResolveRequest {
    pub path: String,
    #[serde(default)]
    pub from_namespace: Option<String>,
}

// =============================================================================
// Helpers
// =============================================================================

/// Build a BlockResponse from a Block + Atom, computing namespace via parent_id chain.
async fn build_block_response(
    db: &dyn Store,
    block: &Block,
    atom: &Atom,
    content: String,
) -> Result<BlockResponse, ApiError> {
    let namespace = db.compute_namespace(block.id).await?;

    Ok(BlockResponse {
        id: block.id,
        lineage_id: block.lineage_id,
        parent_id: block.parent_id,
        namespace,
        name: block.name.clone(),
        position: block.position.clone(),
        content,
        content_type: atom.content_type.clone(),
        properties: atom.properties.clone(),
        created_at: block.created_at,
    })
}

// =============================================================================
// Health Check
// =============================================================================

/// Health check endpoint
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/health",
    tag = "Health",
    responses(
        (status = 200, description = "Server is healthy", body = HealthResponse)
    )
))]
pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let db_status = match state.db.health_check().await {
        Ok(true) => "connected".to_string(),
        _ => "disconnected".to_string(),
    };

    Json(HealthResponse {
        status: "ok".to_string(),
        database: db_status,
    })
}

// =============================================================================
// Atom Endpoints (Phase 2.2)
// =============================================================================

/// Get atom by lineage ID (raw, with template)
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/atoms/{id}",
    tag = "Atoms",
    params(("id" = Uuid, Path, description = "Lineage ID")),
    responses(
        (status = 200, description = "Atom found", body = AtomResponse),
        (status = 404, description = "Atom not found", body = ErrorResponse)
    )
))]
pub async fn get_atom(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<AtomResponse>, ApiError> {
    let atom = state.db.get_atom(id).await?;
    Ok(Json(AtomResponse {
        id: atom.id,
        lineage_id: id,
        content_type: atom.content_type,
        content_template: atom.content_template,
        links: atom.links,
        properties: atom.properties,
        content_hash: atom.content_hash,
        predecessor_id: atom.predecessor_id,
        created_at: atom.created_at,
    }))
}

/// Get a specific atom snapshot by its own ID (not lineage ID).
/// Used to retrieve pinned schema versions for entry rendering.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/atoms/snapshot/{atom_id}",
    tag = "Atoms",
    params(("atom_id" = Uuid, Path, description = "Atom snapshot ID")),
    responses(
        (status = 200, description = "Atom snapshot found", body = AtomResponse),
        (status = 404, description = "Atom not found", body = ErrorResponse)
    )
))]
pub async fn get_atom_snapshot(
    State(state): State<AppState>,
    Path(atom_id): Path<Uuid>,
) -> Result<Json<AtomResponse>, ApiError> {
    let atom = state.db.get_atom_by_id(atom_id).await?;
    // Find the lineage that owns this atom (walk predecessors or just check lineages)
    let lineage_id = atom.id; // For the response — the caller already has the atom_id
    Ok(Json(AtomResponse {
        id: atom.id,
        lineage_id,
        content_type: atom.content_type,
        content_template: atom.content_template,
        links: atom.links,
        properties: atom.properties,
        content_hash: atom.content_hash,
        predecessor_id: atom.predecessor_id,
        created_at: atom.created_at,
    }))
}

/// Get atom with links resolved to paths
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/atoms/{id}/rendered",
    tag = "Atoms",
    params(("id" = Uuid, Path, description = "Lineage ID")),
    responses(
        (status = 200, description = "Rendered atom", body = AtomRenderedResponse),
        (status = 404, description = "Atom not found", body = ErrorResponse)
    )
))]
pub async fn get_atom_rendered(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<AtomRenderedResponse>, ApiError> {
    let atom = state.db.get_atom(id).await?;

    // Deserialize content (replace {N} placeholders with [[paths]])
    let content =
        deserialize_content(state.db.as_ref(), &atom.content_template, &atom.links).await?;

    Ok(Json(AtomRenderedResponse {
        id: atom.id,
        lineage_id: id,
        content_type: atom.content_type,
        content,
        properties: atom.properties,
        created_at: atom.created_at,
    }))
}

/// Edit lineage (creates new immutable atom snapshot)
#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/atoms/{id}",
    tag = "Atoms",
    params(("id" = Uuid, Path, description = "Lineage ID")),
    request_body = UpdateAtomRequest,
    responses(
        (status = 200, description = "Atom updated", body = AtomResponse),
        (status = 404, description = "Atom not found", body = ErrorResponse)
    )
))]
pub async fn update_atom(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateAtomRequest>,
) -> Result<Json<AtomResponse>, ApiError> {
    // Get the current atom to find its block context for link resolution
    let existing = state.db.get_atom(id).await?;

    // Find a block that references this lineage to get namespace context
    let blocks = state.db.get_blocks_for_lineage(id).await?;
    let context_namespace = if let Some(block) = blocks.first() {
        Some(state.db.compute_namespace(block.id).await?)
    } else {
        None
    };

    // Serialize content (extract [[links]] and convert to template with {N} placeholders)
    let serialized = serialize_content(
        state.db.as_ref(),
        &request.content,
        context_namespace.as_deref(),
    )
    .await?;

    // Edit lineage: creates a new immutable atom snapshot and updates the lineage pointer
    let content_type = request.content_type.unwrap_or(existing.content_type);

    // Preserve the "name" property — it's managed by update_block, not update_atom.
    // Extract it before consuming existing.properties in unwrap_or.
    let existing_name = existing.properties.get("name").cloned();
    let mut properties = request.properties.unwrap_or(existing.properties);
    if let Some(name_val) = existing_name
        && let Some(obj) = properties.as_object_mut()
    {
        obj.entry("name".to_string()).or_insert(name_val);
    }

    let (atom, _lineage) = state
        .db
        .edit_lineage(
            id,
            &content_type,
            &serialized.template,
            &serialized.links,
            &properties,
        )
        .await?;

    Ok(Json(AtomResponse {
        id: atom.id,
        lineage_id: id,
        content_type: atom.content_type,
        content_template: atom.content_template,
        links: atom.links,
        properties: atom.properties,
        content_hash: atom.content_hash,
        predecessor_id: atom.predecessor_id,
        created_at: atom.created_at,
    }))
}

// =============================================================================
// Block Endpoints (Phase 2.3)
// =============================================================================

/// Create block + atom (auto-creates namespace parents)
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/blocks",
    tag = "Blocks",
    request_body = CreateBlockRequest,
    responses(
        (status = 201, description = "Block created", body = CreateBlockResponse),
        (status = 400, description = "Invalid input", body = ErrorResponse)
    )
))]
pub async fn create_block(
    State(state): State<AppState>,
    Json(request): Json<CreateBlockRequest>,
) -> Result<(StatusCode, Json<CreateBlockResponse>), ApiError> {
    // Resolve parent: prefer parent_id if given, else resolve namespace
    let parent_id = if let Some(pid) = request.parent_id {
        Some(pid)
    } else if request.namespace.is_empty() {
        None
    } else {
        // Ensure the namespace exists (mkdir -p behavior)
        Some(state.db.ensure_namespace_block(&request.namespace).await?)
    };

    // Compute context namespace for link resolution
    let context_ns = if let Some(pid) = parent_id {
        Some(state.db.compute_namespace(pid).await?)
    } else if !request.namespace.is_empty() {
        Some(request.namespace.clone())
    } else {
        None
    };

    // Serialize content to extract links
    let serialized = serialize_content(
        state.db.as_ref(),
        &request.content,
        context_ns.as_deref(),
    )
    .await?;

    // Create the block with its atom
    let (block, _atom) = state
        .db
        .create_block_with_content(
            parent_id,
            &request.name,
            &serialized.template,
            &serialized.links,
            &request.content_type,
            &request.properties,
        )
        .await?;

    // If a specific position was requested, update it (create_block_with_content always appends)
    let block = if let Some(ref pos) = request.position {
        state
            .db
            .update_block(
                block.id,
                &yap_core::models::UpdateBlock {
                    name: None,
                    position: Some(pos.clone()),
                },
            )
            .await?
    } else {
        block
    };

    // Compute display namespace for response
    let namespace = state.db.compute_namespace(block.id).await?;

    Ok((
        StatusCode::CREATED,
        Json(CreateBlockResponse {
            block_id: block.id,
            lineage_id: block.lineage_id,
            namespace,
            name: block.name,
        }),
    ))
}

/// List blocks (optionally filtered by namespace, search, lineage, or content type)
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/blocks",
    tag = "Blocks",
    params(ListBlocksQuery),
    responses(
        (status = 200, description = "List of blocks", body = Vec<BlockResponse>)
    )
))]
pub async fn list_blocks(
    State(state): State<AppState>,
    Query(query): Query<ListBlocksQuery>,
) -> Result<Json<Vec<BlockResponse>>, ApiError> {
    let blocks = if let Some(search) = &query.search {
        state.db.search_blocks(search).await?
    } else if let Some(lineage_id) = query.lineage_id {
        state.db.get_blocks_for_lineage(lineage_id).await?
    } else if let Some(ct) = &query.content_type {
        state.db.list_blocks_by_content_type(ct).await?
    } else if let Some(ns) = &query.namespace {
        state.db.list_blocks_by_namespace(ns).await?
    } else {
        state.db.get_root_blocks().await?
    };

    let mut responses = Vec::new();
    for block in blocks {
        let atom = state.db.get_atom(block.lineage_id).await?;
        let content =
            deserialize_content(state.db.as_ref(), &atom.content_template, &atom.links).await?;
        responses.push(build_block_response(state.db.as_ref(), &block, &atom, content).await?);
    }

    Ok(Json(responses))
}

/// List orphaned blocks (deleted parent but not themselves deleted)
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/blocks/orphans",
    tag = "Blocks",
    responses(
        (status = 200, description = "List of orphaned blocks", body = Vec<BlockResponse>)
    )
))]
pub async fn list_orphans(
    State(state): State<AppState>,
) -> Result<Json<Vec<BlockResponse>>, ApiError> {
    let blocks = state.db.list_orphaned_blocks().await?;

    let mut responses = Vec::new();
    for block in blocks {
        let atom = state.db.get_atom(block.lineage_id).await?;
        let content =
            deserialize_content(state.db.as_ref(), &atom.content_template, &atom.links).await?;
        responses.push(build_block_response(state.db.as_ref(), &block, &atom, content).await?);
    }

    Ok(Json(responses))
}

/// Get block with rendered atom content
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/blocks/{id}",
    tag = "Blocks",
    params(("id" = Uuid, Path, description = "Block ID")),
    responses(
        (status = 200, description = "Block found", body = BlockResponse),
        (status = 404, description = "Block not found", body = ErrorResponse)
    )
))]
pub async fn get_block(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<BlockResponse>, ApiError> {
    let (block, atom) = state.db.get_block_with_atom(id).await?;
    let content =
        deserialize_content(state.db.as_ref(), &atom.content_template, &atom.links).await?;

    Ok(Json(
        build_block_response(state.db.as_ref(), &block, &atom, content).await?,
    ))
}

/// Update block metadata (name, position)
#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/blocks/{id}",
    tag = "Blocks",
    params(("id" = Uuid, Path, description = "Block ID")),
    request_body = UpdateBlockRequest,
    responses(
        (status = 200, description = "Block updated", body = BlockResponse),
        (status = 404, description = "Block not found", body = ErrorResponse)
    )
))]
pub async fn update_block(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateBlockRequest>,
) -> Result<Json<BlockResponse>, ApiError> {
    let update = UpdateBlock {
        name: request.name,
        position: request.position,
    };

    let block = state.db.update_block(id, &update).await?;
    let atom = state.db.get_atom(block.lineage_id).await?;
    let content =
        deserialize_content(state.db.as_ref(), &atom.content_template, &atom.links).await?;

    Ok(Json(
        build_block_response(state.db.as_ref(), &block, &atom, content).await?,
    ))
}

/// Soft delete block (orphans children)
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/blocks/{id}",
    tag = "Blocks",
    params(("id" = Uuid, Path, description = "Block ID")),
    responses(
        (status = 204, description = "Block deleted"),
        (status = 404, description = "Block not found", body = ErrorResponse)
    )
))]
pub async fn delete_block(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    // Prevent orphaning: reject if block has children
    let children = state.db.get_block_children(id).await?;
    if !children.is_empty() {
        return Err(ApiError::bad_request(format!(
            "Block has {} children. Use recursive delete or move children first.",
            children.len()
        )));
    }
    state.db.delete_block(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Soft delete block and all descendants
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/blocks/{id}/recursive",
    tag = "Blocks",
    params(("id" = Uuid, Path, description = "Block ID")),
    responses(
        (status = 204, description = "Block and descendants deleted"),
        (status = 404, description = "Block not found", body = ErrorResponse)
    )
))]
pub async fn delete_block_recursive(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.db.delete_block_recursive(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Get child blocks
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/blocks/{id}/children",
    tag = "Blocks",
    params(("id" = Uuid, Path, description = "Parent block ID")),
    responses(
        (status = 200, description = "List of child blocks", body = Vec<BlockResponse>),
        (status = 404, description = "Block not found", body = ErrorResponse)
    )
))]
pub async fn get_block_children(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<BlockResponse>>, ApiError> {
    // First verify the parent exists
    let _ = state.db.get_block(id).await?;

    let children = state.db.get_block_children(id).await?;

    let mut responses = Vec::new();
    for block in children {
        let atom = state.db.get_atom(block.lineage_id).await?;
        let content =
            deserialize_content(state.db.as_ref(), &atom.content_template, &atom.links).await?;
        responses.push(build_block_response(state.db.as_ref(), &block, &atom, content).await?);
    }

    Ok(Json(responses))
}

/// Restore soft-deleted block
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/blocks/{id}/restore",
    tag = "Blocks",
    params(("id" = Uuid, Path, description = "Block ID")),
    responses(
        (status = 200, description = "Block restored", body = BlockResponse),
        (status = 404, description = "Block not found", body = ErrorResponse)
    )
))]
pub async fn restore_block(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<BlockResponse>, ApiError> {
    let block = state.db.restore_block(id).await?;
    let atom = state.db.get_atom(block.lineage_id).await?;
    let content =
        deserialize_content(state.db.as_ref(), &atom.content_template, &atom.links).await?;

    Ok(Json(
        build_block_response(state.db.as_ref(), &block, &atom, content).await?,
    ))
}

/// Restore a soft-deleted block and all its descendants
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/blocks/{id}/restore-recursive",
    tag = "Blocks",
    params(("id" = Uuid, Path, description = "Block ID")),
    responses(
        (status = 200, description = "Blocks restored", body = serde_json::Value),
        (status = 404, description = "Block not found", body = ErrorResponse)
    )
))]
pub async fn restore_block_recursive(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let restored = state.db.restore_block_recursive(id).await?;
    Ok(Json(serde_json::json!({ "restored": restored })))
}

/// Move block to new parent
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/blocks/{id}/move",
    tag = "Blocks",
    params(("id" = Uuid, Path, description = "Block ID")),
    request_body = MoveBlockRequest,
    responses(
        (status = 200, description = "Block moved", body = BlockResponse),
        (status = 404, description = "Block not found", body = ErrorResponse)
    )
))]
pub async fn move_block(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<MoveBlockRequest>,
) -> Result<Json<BlockResponse>, ApiError> {
    let block = state
        .db
        .move_block(id, request.parent_id, request.position)
        .await?;

    let atom = state.db.get_atom(block.lineage_id).await?;
    let content =
        deserialize_content(state.db.as_ref(), &atom.content_template, &atom.links).await?;

    Ok(Json(
        build_block_response(state.db.as_ref(), &block, &atom, content).await?,
    ))
}

// =============================================================================
// Link/Graph Endpoints (Phase 2.4)
// =============================================================================

/// Get lineages that link to this lineage (via links array)
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/atoms/{id}/backlinks",
    tag = "Graph",
    params(("id" = Uuid, Path, description = "Lineage ID")),
    responses(
        (status = 200, description = "List of backlinks", body = Vec<BacklinkResponse>),
        (status = 404, description = "Atom not found", body = ErrorResponse)
    )
))]
pub async fn get_atom_backlinks(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<BacklinkResponse>>, ApiError> {
    // Verify the lineage exists first
    let _ = state.db.get_atom(id).await?;

    // Get lineages whose links[] contain this lineage_id (GIN-eligible query)
    let backlinks = state.db.get_backlinks(id).await?;

    let mut responses = Vec::new();
    for bl in backlinks {
        let content =
            deserialize_content(state.db.as_ref(), &bl.atom.content_template, &bl.atom.links)
                .await?;
        let namespace = state.db.get_canonical_path(bl.lineage_id).await?;

        responses.push(BacklinkResponse {
            lineage_id: bl.lineage_id,
            content,
            content_type: bl.atom.content_type,
            namespace,
        });
    }

    Ok(Json(responses))
}

/// Get lineages with edges pointing to this lineage
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/atoms/{id}/references",
    tag = "Graph",
    params(("id" = Uuid, Path, description = "Lineage ID")),
    responses(
        (status = 200, description = "List of references", body = Vec<BacklinkResponse>),
        (status = 404, description = "Atom not found", body = ErrorResponse)
    )
))]
pub async fn get_atom_references(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<BacklinkResponse>>, ApiError> {
    // Verify the lineage exists first
    let _ = state.db.get_atom(id).await?;

    // Get edges pointing to this lineage
    let edges = state.db.get_edges_to(id).await?;

    // Collect unique source lineages
    let mut seen = std::collections::HashSet::new();
    let mut responses = Vec::new();

    for edge in edges {
        if seen.insert(edge.from_lineage_id) {
            let atom = state.db.get_atom(edge.from_lineage_id).await?;
            let content =
                deserialize_content(state.db.as_ref(), &atom.content_template, &atom.links).await?;
            let namespace = state.db.get_canonical_path(edge.from_lineage_id).await?;

            responses.push(BacklinkResponse {
                lineage_id: edge.from_lineage_id,
                content,
                content_type: atom.content_type,
                namespace,
            });
        }
    }

    Ok(Json(responses))
}

/// Get full graph neighborhood (atom + backlinks + outlinks + edges)
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/atoms/{id}/graph",
    tag = "Graph",
    params(("id" = Uuid, Path, description = "Lineage ID")),
    responses(
        (status = 200, description = "Graph neighborhood", body = GraphResponse),
        (status = 404, description = "Atom not found", body = ErrorResponse)
    )
))]
pub async fn get_atom_graph(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<GraphResponse>, ApiError> {
    // Get the atom
    let atom = state.db.get_atom(id).await?;
    let content =
        deserialize_content(state.db.as_ref(), &atom.content_template, &atom.links).await?;

    // Build the rendered atom response
    let atom_response = AtomRenderedResponse {
        id: atom.id,
        lineage_id: id,
        content_type: atom.content_type.clone(),
        content,
        properties: atom.properties.clone(),
        created_at: atom.created_at,
    };

    // Get backlinks (lineages linking TO this lineage)
    let backlink_results = state.db.get_backlinks(id).await?;
    let mut backlinks = Vec::new();
    for bl in backlink_results {
        let content =
            deserialize_content(state.db.as_ref(), &bl.atom.content_template, &bl.atom.links)
                .await?;
        let namespace = state.db.get_canonical_path(bl.lineage_id).await?;
        backlinks.push(BacklinkResponse {
            lineage_id: bl.lineage_id,
            content,
            content_type: bl.atom.content_type,
            namespace,
        });
    }

    // Get outlinks (lineages this links TO via its links array)
    let mut outlinks = Vec::new();
    for link_id in &atom.links {
        if let Ok(linked_atom) = state.db.get_atom(*link_id).await {
            let content = deserialize_content(
                state.db.as_ref(),
                &linked_atom.content_template,
                &linked_atom.links,
            )
            .await?;
            let namespace = state.db.get_canonical_path(*link_id).await?;
            outlinks.push(BacklinkResponse {
                lineage_id: *link_id,
                content,
                content_type: linked_atom.content_type,
                namespace,
            });
        }
    }

    // Get edges
    let outgoing_edges = state.db.get_edges_from(id).await?;
    let incoming_edges = state.db.get_edges_to(id).await?;

    let edges = EdgesResponse {
        outgoing: outgoing_edges.into_iter().map(EdgeResponse::from).collect(),
        incoming: incoming_edges.into_iter().map(EdgeResponse::from).collect(),
    };

    // Get hard links (other blocks sharing the same lineage).
    // Only populated when 2+ blocks reference the same lineage.
    let all_blocks = state.db.get_blocks_for_lineage(id).await?;
    let hard_links = if all_blocks.len() > 1 {
        let mut links = Vec::with_capacity(all_blocks.len());
        for block in &all_blocks {
            let ns = state.db.compute_namespace(block.id).await?;
            links.push(HardLinkResponse {
                block_id: block.id,
                namespace: ns,
                name: block.name.clone(),
            });
        }
        links
    } else {
        Vec::new()
    };

    Ok(Json(GraphResponse {
        atom: atom_response,
        backlinks,
        outlinks,
        edges,
        hard_links,
    }))
}

/// Get connections between a set of lineages (subtree graph)
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/graph/subtree",
    tag = "Graph",
    request_body = SubtreeGraphRequest,
    responses(
        (status = 200, description = "Connections between lineages", body = SubtreeGraphResponse),
        (status = 400, description = "Too many lineage IDs", body = ErrorResponse)
    )
))]
pub async fn get_subtree_graph(
    State(state): State<AppState>,
    Json(request): Json<SubtreeGraphRequest>,
) -> Result<Json<SubtreeGraphResponse>, ApiError> {
    if request.lineage_ids.is_empty() {
        return Ok(Json(SubtreeGraphResponse {
            content_links: vec![],
            edges: vec![],
        }));
    }

    if request.lineage_ids.len() > 1000 {
        return Err(ApiError::bad_request("Too many lineage IDs (max 1000)"));
    }

    let edges = state
        .db
        .get_edges_between(&request.lineage_ids)
        .await?
        .into_iter()
        .map(EdgeResponse::from)
        .collect();

    let content_links = state
        .db
        .get_content_links_between(&request.lineage_ids)
        .await?
        .into_iter()
        .map(|(from, to)| ContentLinkResponse {
            from_lineage_id: from,
            to_lineage_id: to,
        })
        .collect();

    Ok(Json(SubtreeGraphResponse {
        content_links,
        edges,
    }))
}

// =============================================================================
// Edge Endpoints (Phase 2.5)
// =============================================================================

/// Create edge between lineages
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/edges",
    tag = "Edges",
    request_body = CreateEdgeRequest,
    responses(
        (status = 201, description = "Edge created", body = EdgeResponse),
        (status = 404, description = "Lineage not found", body = ErrorResponse),
        (status = 409, description = "Edge already exists", body = ErrorResponse)
    )
))]
pub async fn create_edge(
    State(state): State<AppState>,
    Json(request): Json<CreateEdgeRequest>,
) -> Result<(StatusCode, Json<EdgeResponse>), ApiError> {
    // Verify both lineages exist
    let _ = state.db.get_atom(request.from_lineage_id).await?;
    let _ = state.db.get_atom(request.to_lineage_id).await?;

    let create = CreateEdge {
        from_lineage_id: request.from_lineage_id,
        to_lineage_id: request.to_lineage_id,
        edge_type: request.edge_type,
        properties: request.properties,
    };

    let edge = state.db.create_edge(&create).await?;

    Ok((StatusCode::CREATED, Json(EdgeResponse::from(edge))))
}

/// Delete edge
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/edges/{id}",
    tag = "Edges",
    params(("id" = Uuid, Path, description = "Edge ID")),
    responses(
        (status = 204, description = "Edge deleted"),
        (status = 404, description = "Edge not found", body = ErrorResponse)
    )
))]
pub async fn delete_edge(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.db.delete_edge(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Get all edges for an atom (both directions)
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/atoms/{id}/edges",
    tag = "Edges",
    params(("id" = Uuid, Path, description = "Lineage ID")),
    responses(
        (status = 200, description = "Edges in both directions", body = EdgesResponse),
        (status = 404, description = "Atom not found", body = ErrorResponse)
    )
))]
pub async fn get_atom_edges(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<EdgesResponse>, ApiError> {
    // Verify the lineage exists first
    let _ = state.db.get_atom(id).await?;

    let outgoing = state.db.get_edges_from(id).await?;
    let incoming = state.db.get_edges_to(id).await?;

    Ok(Json(EdgesResponse {
        outgoing: outgoing.into_iter().map(EdgeResponse::from).collect(),
        incoming: incoming.into_iter().map(EdgeResponse::from).collect(),
    }))
}

// =============================================================================
// Utility Endpoints (Phase 2.6)
// =============================================================================

/// List all root-level blocks (parent_id IS NULL)
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/blocks/roots",
    tag = "Blocks",
    responses(
        (status = 200, description = "List of root blocks", body = Vec<NamespaceResponse>)
    )
))]
pub async fn list_roots(
    State(state): State<AppState>,
) -> Result<Json<Vec<NamespaceResponse>>, ApiError> {
    let blocks = state.db.get_root_blocks().await?;

    let mut responses = Vec::new();
    for block in blocks {
        let namespace = state.db.compute_namespace(block.id).await?;

        responses.push(NamespaceResponse {
            id: block.id,
            namespace,
            name: block.name.clone(),
            lineage_id: block.lineage_id,
            parent_id: block.parent_id,
            position: block.position.clone(),
        });
    }

    Ok(Json(responses))
}

// =============================================================================
// Property Keys Endpoint
// =============================================================================

/// Get all unique property keys in a block's subtree
pub async fn get_property_keys(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<String>>, ApiError> {
    let _ = state.db.get_block(id).await?;
    let keys = state.db.list_property_keys_in_subtree(id).await?;
    Ok(Json(keys))
}

// =============================================================================
// Export / Import Endpoints
// =============================================================================

#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct ExportQueryParams {
    /// Comma-separated list of property keys to include (default: all non-underscore keys)
    #[serde(default)]
    pub include_keys: Option<String>,
}

#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct ImportQueryParams {
    #[serde(default = "default_import_mode")]
    pub mode: String,
    #[serde(default)]
    pub match_by: Option<String>,
    /// When true in merge mode, search globally for matching lineages to hard-link
    #[serde(default)]
    pub global_link: Option<bool>,
    /// When true, delete the existing matching subtree after successful import
    #[serde(default)]
    pub replace: Option<bool>,
}

fn default_import_mode() -> String {
    "merge".to_string()
}

fn parse_import_options(params: &ImportQueryParams) -> ImportOptions {
    let mode = match params.mode.as_str() {
        "copy" => ImportMode::Copy,
        _ => ImportMode::Merge,
    };
    let match_strategy = match params.match_by.as_deref() {
        Some("export_hash") => MatchStrategy::ExportHash,
        Some("content_identity") => MatchStrategy::ContentIdentity,
        Some("merkle") => MatchStrategy::Merkle,
        Some("topology") => MatchStrategy::Topology,
        _ => MatchStrategy::Auto,
    };
    ImportOptions {
        mode,
        match_strategy,
        global_link: params.global_link.unwrap_or(false),
        replace_existing: params.replace.unwrap_or(false),
    }
}

/// Export a subtree to portable JSON
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/blocks/{id}/export",
    tag = "Export/Import",
    params(("id" = Uuid, Path, description = "Root block ID to export")),
    responses(
        (status = 200, description = "Exported tree", body = ExportTree),
        (status = 404, description = "Block not found", body = ErrorResponse)
    )
))]
pub async fn export_block_tree(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<ExportQueryParams>,
) -> Result<Json<ExportTree>, ApiError> {
    let options = ExportOptions {
        include_keys: params
            .include_keys
            .map(|s| s.split(',').map(|k| k.trim().to_string()).collect()),
    };
    let tree = yap_core::export::export_tree(state.db.as_ref(), id, &options).await?;
    Ok(Json(tree))
}

/// Import a subtree under a block
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/blocks/{id}/import",
    tag = "Export/Import",
    params(
        ("id" = Uuid, Path, description = "Parent block ID to import under"),
        ImportQueryParams,
    ),
    request_body = ExportTree,
    responses(
        (status = 201, description = "Import result", body = ImportResult),
        (status = 400, description = "Invalid input", body = ErrorResponse),
        (status = 404, description = "Block not found", body = ErrorResponse)
    )
))]
pub async fn import_block_tree(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<ImportQueryParams>,
    Json(tree): Json<ExportTree>,
) -> Result<(StatusCode, Json<ImportResult>), ApiError> {
    let options = parse_import_options(&params);
    let result = yap_core::export::import_tree(state.db.as_ref(), &tree, Some(id), options).await?;
    Ok((StatusCode::CREATED, Json(result)))
}

/// Import a subtree at the root level (no parent block)
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/import",
    tag = "Export/Import",
    params(ImportQueryParams),
    request_body = ExportTree,
    responses(
        (status = 201, description = "Import result", body = ImportResult),
        (status = 400, description = "Invalid input", body = ErrorResponse)
    )
))]
pub async fn import_at_root(
    State(state): State<AppState>,
    Query(params): Query<ImportQueryParams>,
    Json(tree): Json<ExportTree>,
) -> Result<(StatusCode, Json<ImportResult>), ApiError> {
    let options = parse_import_options(&params);
    let result = yap_core::export::import_tree(state.db.as_ref(), &tree, None, options).await?;
    Ok((StatusCode::CREATED, Json(result)))
}

// =============================================================================
// ZIP Export/Import (with media files) — gated behind zip-export feature
// (lzma-sys can't compile to wasm32-unknown-unknown)
// =============================================================================

/// Export a subtree as a ZIP containing tree.json + blob files.
///
/// GET /api/blocks/{id}/export-zip
pub async fn export_block_tree_zip(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<ExportQueryParams>,
) -> Result<Response, ApiError> {
    use std::io::Write;

    let options = ExportOptions {
        include_keys: params
            .include_keys
            .map(|s| s.split(',').map(|k| k.trim().to_string()).collect()),
    };
    let tree = yap_core::export::export_tree(state.db.as_ref(), id, &options).await?;
    let file_hashes = yap_core::export::collect_file_hashes(&tree);

    let mut zip_buffer = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_buffer));
        let zip_options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        // Write tree.json
        let tree_json = serde_json::to_vec_pretty(&tree)
            .map_err(|e| ApiError::internal(format!("JSON serialize failed: {}", e)))?;
        zip.start_file("tree.json", zip_options)
            .map_err(|e| ApiError::internal(format!("ZIP error: {}", e)))?;
        zip.write_all(&tree_json)
            .map_err(|e| ApiError::internal(format!("ZIP write error: {}", e)))?;

        // Write blob files
        for hash in &file_hashes {
            if let Some(data) = state.files.get_file(hash).await.map_err(ApiError::from)? {
                zip.start_file(format!("files/{}", hash), zip_options)
                    .map_err(|e| ApiError::internal(format!("ZIP error: {}", e)))?;
                zip.write_all(&data)
                    .map_err(|e| ApiError::internal(format!("ZIP write error: {}", e)))?;
            }
        }

        zip.finish()
            .map_err(|e| ApiError::internal(format!("ZIP finish error: {}", e)))?;
    }

    Ok((
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/zip".to_string(),
            ),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"export.zip\"".to_string(),
            ),
        ],
        zip_buffer,
    )
        .into_response())
}

/// Core ZIP import logic shared by the two handler variants.
async fn do_import_zip(
    state: &AppState,
    parent_id: Option<Uuid>,
    params: &ImportQueryParams,
    body_bytes: &[u8],
) -> Result<ImportResult, ApiError> {
    use std::io::Read;

    // Extract all data from ZIP synchronously (no .await while borrowing zip)
    let (tree, file_blobs) = {
        let cursor = std::io::Cursor::new(body_bytes);
        let mut zip = zip::ZipArchive::new(cursor)
            .map_err(|e| ApiError::bad_request(format!("Invalid ZIP: {}", e)))?;

        // Collect file blobs
        let mut blobs: Vec<Vec<u8>> = Vec::new();
        for i in 0..zip.len() {
            let mut file = zip
                .by_index(i)
                .map_err(|e| ApiError::bad_request(format!("ZIP read error: {}", e)))?;
            let name = file.name().to_string();
            if name.starts_with("files/") && name.len() > 6 {
                let mut data = Vec::new();
                file.read_to_end(&mut data)
                    .map_err(|e| ApiError::internal(format!("ZIP extract error: {}", e)))?;
                blobs.push(data);
            }
        }

        // Extract tree.json
        let tree: ExportTree = {
            let mut file = zip
                .by_name("tree.json")
                .map_err(|_| ApiError::bad_request("ZIP missing tree.json"))?;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)
                .map_err(|e| ApiError::internal(format!("ZIP extract error: {}", e)))?;
            serde_json::from_slice(&buf)
                .map_err(|e| ApiError::bad_request(format!("Invalid tree.json: {}", e)))?
        };

        (tree, blobs)
    };

    // Now store blobs (async) — zip is dropped, no borrow issues
    for data in &file_blobs {
        state.files.put_file(data).await.map_err(ApiError::from)?;
    }

    let options = parse_import_options(params);
    let result =
        yap_core::export::import_tree(state.db.as_ref(), &tree, parent_id, options).await?;
    Ok(result)
}

#[derive(Deserialize)]
pub struct ZipImportRequest {
    /// Base64-encoded ZIP data
    pub data: String,
}

/// Import ZIP under a specific parent block.
pub async fn import_zip_under_block(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<ImportQueryParams>,
    Json(request): Json<ZipImportRequest>,
) -> Result<(StatusCode, Json<ImportResult>), ApiError> {
    use base64::Engine;
    let body_bytes = base64::engine::general_purpose::STANDARD
        .decode(&request.data)
        .map_err(|e| ApiError::bad_request(format!("Invalid base64: {}", e)))?;
    let result = do_import_zip(&state, Some(id), &params, &body_bytes).await?;
    Ok((StatusCode::CREATED, Json(result)))
}

/// Import ZIP at root level.
pub async fn import_zip_at_root(
    State(state): State<AppState>,
    Query(params): Query<ImportQueryParams>,
    Json(request): Json<ZipImportRequest>,
) -> Result<(StatusCode, Json<ImportResult>), ApiError> {
    use base64::Engine;
    let body_bytes = base64::engine::general_purpose::STANDARD
        .decode(&request.data)
        .map_err(|e| ApiError::bad_request(format!("Invalid base64: {}", e)))?;
    let result = do_import_zip(&state, None, &params, &body_bytes).await?;
    Ok((StatusCode::CREATED, Json(result)))
}

// =============================================================================
// Schema Endpoints (Phase 6.1 - Custom Types)
// =============================================================================

/// List all schema blocks with field definitions
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/schemas",
    tag = "Schemas",
    responses(
        (status = 200, description = "List of schemas", body = Vec<SchemaResponse>)
    )
))]
pub async fn list_schemas(
    State(state): State<AppState>,
) -> Result<Json<Vec<SchemaResponse>>, ApiError> {
    let blocks = state.db.list_schemas().await?;

    let mut responses = Vec::new();
    for block in blocks {
        let atom = state.db.get_atom(block.lineage_id).await?;
        let lineage = state.db.get_lineage(block.lineage_id).await?;
        let content =
            deserialize_content(state.db.as_ref(), &atom.content_template, &atom.links).await?;
        let namespace = state.db.compute_namespace(block.id).await?;
        let fields = atom
            .properties
            .get("fields")
            .cloned()
            .unwrap_or(serde_json::json!([]));

        responses.push(SchemaResponse {
            block_id: block.id,
            lineage_id: block.lineage_id,
            namespace,
            name: block.name.clone(),
            version: lineage.version,
            fields,
            content,
        });
    }

    Ok(Json(responses))
}

/// Resolve type name with namespace walk-up
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/schemas/resolve",
    tag = "Schemas",
    request_body = ResolveSchemaRequest,
    responses(
        (status = 200, description = "Schema resolved", body = SchemaResponse),
        (status = 404, description = "Schema not found", body = ErrorResponse)
    )
))]
pub async fn resolve_schema(
    State(state): State<AppState>,
    Json(request): Json<ResolveSchemaRequest>,
) -> Result<Json<SchemaResponse>, ApiError> {
    let block = state
        .db
        .resolve_schema(&request.type_name, request.from_namespace.as_deref())
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Schema '{}' not found", request.type_name)))?;

    let atom = state.db.get_atom(block.lineage_id).await?;
    let lineage = state.db.get_lineage(block.lineage_id).await?;
    let content =
        deserialize_content(state.db.as_ref(), &atom.content_template, &atom.links).await?;
    let namespace = state.db.compute_namespace(block.id).await?;
    let fields = atom
        .properties
        .get("fields")
        .cloned()
        .unwrap_or(serde_json::json!([]));

    Ok(Json(SchemaResponse {
        block_id: block.id,
        lineage_id: block.lineage_id,
        namespace,
        name: block.name.clone(),
        version: lineage.version,
        fields,
        content,
    }))
}

/// Resolve link path to block/lineage ID
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/resolve",
    tag = "Links",
    request_body = ResolveRequest,
    responses(
        (status = 200, description = "Link resolved", body = ResolveResponse),
        (status = 400, description = "Invalid path", body = ErrorResponse),
        (status = 404, description = "Block not found", body = ErrorResponse)
    )
))]
pub async fn resolve_link(
    State(state): State<AppState>,
    Json(request): Json<ResolveRequest>,
) -> Result<Json<ResolveResponse>, ApiError> {
    // Parse the path as a link
    let link_syntax = format!("[[{}]]", request.path);
    let parsed = parse_links(&link_syntax);

    if parsed.is_empty() {
        return Err(ApiError::bad_request("Invalid link path"));
    }

    let link = &parsed[0];

    // Resolve the path
    let resolved_segments = resolve_path(
        &link.segments,
        link.is_relative,
        link.parent_levels,
        request.from_namespace.as_deref(),
    );

    let Some(segments) = resolved_segments else {
        return Err(ApiError::bad_request(
            "Could not resolve relative path without context",
        ));
    };

    let namespace = format_namespace(&segments);

    // Look up the block at this namespace
    let block = state
        .db
        .find_block_by_namespace(&namespace)
        .await?
        .ok_or_else(|| {
            ApiError::not_found(format!("No block found at namespace: {}", namespace))
        })?;

    let display_namespace = state.db.compute_namespace(block.id).await?;

    Ok(Json(ResolveResponse {
        lineage_id: block.lineage_id,
        block_id: block.id,
        namespace: display_namespace,
    }))
}

// =============================================================================
// Debug Log Endpoint
// =============================================================================

#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct DebugLogQuery {
    pub since: Option<u64>,
}

/// Get recent log entries from the tracing ring buffer
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/debug/logs",
    tag = "Debug",
    params(DebugLogQuery),
    responses(
        (status = 200, description = "Log entries", body = Vec<crate::log_buffer::LogEntry>)
    )
))]
pub async fn get_debug_logs(
    State(state): State<AppState>,
    Query(query): Query<DebugLogQuery>,
) -> Json<Vec<crate::log_buffer::LogEntry>> {
    let since = query.since.unwrap_or(0);
    Json(state.log_buffer.entries_since(since))
}

// =============================================================================
// File Storage
// =============================================================================

/// Response for file upload
#[derive(Serialize)]
pub struct FileUploadResponse {
    pub hash: String,
    pub size: usize,
}

/// Upload a file via multipart form or JSON base64.
///
/// Accepts either:
/// - `multipart/form-data` with a `file` field (standard HTTP upload)
/// - `application/json` with `{ "data": "<base64>", "filename": "...", "mime": "..." }` (WASM mode)
pub async fn upload_file(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    body: axum::body::Body,
) -> Result<Json<FileUploadResponse>, ApiError> {
    use axum::body::Bytes;
    use http_body_util::BodyExt;

    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let file_data: Vec<u8> = if content_type.starts_with("multipart/form-data") {
        // Multipart upload — extract the first file field
        let boundary = content_type
            .split("boundary=")
            .nth(1)
            .ok_or_else(|| ApiError::bad_request("Missing multipart boundary"))?
            .to_string();

        let body_bytes: Bytes = body
            .collect()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to read body: {}", e)))?
            .to_bytes();

        let mut multipart = multer::Multipart::new(
            futures_util::stream::once(async move { Ok::<_, std::io::Error>(body_bytes) }),
            boundary,
        );

        let field = multipart
            .next_field()
            .await
            .map_err(|e| ApiError::bad_request(format!("Invalid multipart: {}", e)))?
            .ok_or_else(|| ApiError::bad_request("No file field in multipart body"))?;

        field
            .bytes()
            .await
            .map_err(|e| ApiError::bad_request(format!("Failed to read field: {}", e)))?
            .to_vec()
    } else if content_type.starts_with("application/json") {
        // JSON upload with base64 data (WASM mode)
        #[derive(Deserialize)]
        struct JsonUpload {
            data: String, // base64-encoded
        }

        let body_bytes: Bytes = body
            .collect()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to read body: {}", e)))?
            .to_bytes();

        let upload: JsonUpload = serde_json::from_slice(&body_bytes)
            .map_err(|e| ApiError::bad_request(format!("Invalid JSON: {}", e)))?;

        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(&upload.data)
            .map_err(|e| ApiError::bad_request(format!("Invalid base64: {}", e)))?
    } else {
        return Err(ApiError::bad_request(
            "Expected Content-Type: multipart/form-data or application/json",
        ));
    };

    // Enforce size limit (50 MB)
    const MAX_FILE_SIZE: usize = 50 * 1024 * 1024;
    if file_data.len() > MAX_FILE_SIZE {
        return Err(ApiError::bad_request(format!(
            "File too large ({} bytes, max {} bytes)",
            file_data.len(),
            MAX_FILE_SIZE
        )));
    }

    let size = file_data.len();
    let hash = state
        .files
        .put_file(&file_data)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(FileUploadResponse { hash, size }))
}

/// Download a file by its content hash.
pub async fn download_file(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    headers: axum::http::HeaderMap,
    Query(query): Query<FileDownloadQuery>,
) -> Result<Response, ApiError> {
    let data = state
        .files
        .get_file(&hash)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found(format!("File not found: {}", hash)))?;

    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // WASM mode: return base64 JSON when Accept: application/json or ?format=json
    let wants_json = accept.contains("application/json")
        || query.format.as_deref() == Some("json");
    if wants_json {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
        let mime = query.mime.as_deref().unwrap_or("application/octet-stream");
        let json = serde_json::json!({
            "data": b64,
            "mime": mime,
            "size": data.len(),
        });
        return Ok(Json(json).into_response());
    }

    // Standard binary response
    let mime = query
        .mime
        .as_deref()
        .unwrap_or("application/octet-stream")
        .to_string();

    Ok((
        [
            (axum::http::header::CONTENT_TYPE, mime),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "inline".to_string(),
            ),
        ],
        data,
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct FileDownloadQuery {
    pub mime: Option<String>,
    /// Set to "json" to get base64-encoded response (used by WASM mode)
    pub format: Option<String>,
}

/// Check if a file exists by its content hash.
pub async fn check_file(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<StatusCode, ApiError> {
    let exists = state
        .files
        .file_exists(&hash)
        .await
        .map_err(ApiError::from)?;

    if exists {
        Ok(StatusCode::OK)
    } else {
        Ok(StatusCode::NOT_FOUND)
    }
}

// =============================================================================
// Benchmarks (feature-gated)
// =============================================================================

#[cfg(feature = "bench")]
pub async fn run_benchmarks_handler(
    State(state): State<AppState>,
    Json(config): Json<yap_bench::BenchmarkConfig>,
) -> Json<yap_bench::BenchmarkResults> {
    let results = yap_bench::run_benchmarks(state.db.as_ref(), config).await;
    Json(results)
}
