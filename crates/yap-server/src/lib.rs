//! yap-server library — exposes the router and state for embedding in other crates.

pub mod api;
pub mod log_buffer;

use std::sync::Arc;

use axum::{
    Router,
    routing::{delete, get, post, put},
};
#[cfg(feature = "http-layers")]
use tower_http::cors::{Any, CorsLayer};
#[cfg(feature = "http-layers")]
use tower_http::trace::TraceLayer;
#[cfg(feature = "openapi")]
use utoipa::OpenApi;
#[cfg(feature = "openapi")]
use utoipa_swagger_ui::SwaggerUi;
use yap_core::Store;

pub use log_buffer::{BufferLayer, LogBuffer};

// Re-export bootstrap functions for backward compatibility.
pub use yap_core::bootstrap::{
    ensure_block_content_type, ensure_meta_schema, ensure_person_schema, ensure_settings,
    ensure_todo_schema,
};

/// Application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<dyn Store>,
    pub log_buffer: Arc<LogBuffer>,
}

/// Build the Axum router with all API routes attached to the given state.
pub fn build_router(state: AppState) -> Router {
    let router = Router::new()
        .route("/health", get(api::health))
        .route("/api/atoms/snapshot/{atom_id}", get(api::get_atom_snapshot))
        .route("/api/atoms/{id}", get(api::get_atom))
        .route("/api/atoms/{id}/rendered", get(api::get_atom_rendered))
        .route("/api/atoms/{id}", put(api::update_atom))
        .route("/api/atoms/{id}/backlinks", get(api::get_atom_backlinks))
        .route("/api/atoms/{id}/references", get(api::get_atom_references))
        .route("/api/atoms/{id}/graph", get(api::get_atom_graph))
        .route("/api/atoms/{id}/edges", get(api::get_atom_edges))
        .route("/api/blocks", post(api::create_block))
        .route("/api/blocks", get(api::list_blocks))
        .route("/api/blocks/orphans", get(api::list_orphans))
        .route("/api/blocks/{id}", get(api::get_block))
        .route("/api/blocks/{id}", put(api::update_block))
        .route("/api/blocks/{id}", delete(api::delete_block))
        .route(
            "/api/blocks/{id}/recursive",
            delete(api::delete_block_recursive),
        )
        .route("/api/blocks/{id}/children", get(api::get_block_children))
        .route("/api/blocks/{id}/restore", post(api::restore_block))
        .route(
            "/api/blocks/{id}/restore-recursive",
            post(api::restore_block_recursive),
        )
        .route("/api/blocks/{id}/move", post(api::move_block))
        .route(
            "/api/blocks/{id}/property-keys",
            get(api::get_property_keys),
        )
        .route("/api/blocks/{id}/export", get(api::export_block_tree))
        .route("/api/blocks/{id}/import", post(api::import_block_tree))
        .route("/api/import", post(api::import_at_root))
        .route("/api/edges", post(api::create_edge))
        .route("/api/edges/{id}", delete(api::delete_edge))
        .route("/api/blocks/roots", get(api::list_roots))
        .route("/api/resolve", post(api::resolve_link))
        .route("/api/schemas", get(api::list_schemas))
        .route("/api/schemas/resolve", post(api::resolve_schema))
        .route("/api/graph/subtree", post(api::get_subtree_graph))
        .route("/api/debug/logs", get(api::get_debug_logs));

    #[cfg(feature = "bench")]
    let router = router.route("/api/debug/benchmarks", post(api::run_benchmarks_handler));

    #[cfg(feature = "openapi")]
    let router = router
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", api::ApiDoc::openapi()));

    let router = router.with_state(state);

    #[cfg(feature = "http-layers")]
    let router = {
        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);
        router.layer(cors).layer(TraceLayer::new_for_http())
    };

    router
}
