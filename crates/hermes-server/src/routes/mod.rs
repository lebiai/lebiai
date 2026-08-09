//! Route table. All endpoints under `/api/v1`.
//! Serves the Flutter client surface: a subset of the `hermes-gui`
//! Tauri commands (chat via WebSocket frames) — NOT a 1:1 mirror.

use std::sync::Arc;

use axum::middleware::from_fn_with_state;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::auth::auth_middleware;
use crate::state::AppState;

pub mod chat;
pub mod config;
pub mod mcp;
pub mod memory;
pub mod reflect;
pub mod sessions;
pub mod skills;
pub mod uploads;

/// Build the full router with shared `Arc<AppState>`, every route gated by
/// the bearer-token auth middleware (header **or** `?token=` query, so it
/// covers both REST and the WS upgrade handshake).
pub fn build(state: Arc<AppState>, token: Arc<String>) -> Router {
    Router::new()
        // health
        .route("/api/v1/health", get(health))
        // chat (WebSocket)
        .route("/api/v1/chat", get(chat::handler))
        // sessions
        .route(
            "/api/v1/sessions",
            get(sessions::list_sessions)
                .post(sessions::new_session)
                .delete(sessions::delete_session),
        )
        .route("/api/v1/sessions/load", get(sessions::load_session))
        // skills
        .route(
            "/api/v1/skills",
            get(skills::list_skills).post(skills::save_skill),
        )
        .route(
            "/api/v1/skills/{name}",
            get(skills::get_skill).delete(skills::delete_skill),
        )
        // memories
        .route(
            "/api/v1/memories",
            get(memory::list_memories).post(memory::create_memory),
        )
        .route(
            "/api/v1/memories/{id}/toggle-pin",
            post(memory::toggle_pin_memory),
        )
        .route("/api/v1/memories/{id}", delete(memory::delete_memory))
        // config
        .route(
            "/api/v1/config",
            get(config::get_config).put(config::update_config),
        )
        .route("/api/v1/data-dir", get(config::data_dir_get))
        .route("/api/v1/data-dir/migrate", post(config::data_dir_migrate))
        .route("/api/v1/data-dir/reset", post(config::data_dir_reset))
        // mcp
        .route("/api/v1/mcp/tools", get(mcp::list_mcp_tools))
        .route("/api/v1/mcp/servers", get(mcp::list_mcp_servers))
        // reflection
        .route("/api/v1/reflect/{sessionId}", post(reflect::run_reflection))
        .route(
            "/api/v1/reflect/accept-skill",
            post(reflect::accept_skill_candidate),
        )
        .route(
            "/api/v1/reflect/accept-memory",
            post(reflect::accept_memory_candidate),
        )
        .route("/api/v1/reflect/conflict", post(reflect::handle_conflict))
        // document import (1:1 GUI upload commands)
        .route(
            "/api/v1/uploads/converter",
            get(uploads::check_document_converter),
        )
        .route("/api/v1/uploads", post(uploads::import_document_handler))
        .layer(from_fn_with_state(token, auth_middleware))
        .with_state(state)
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}
