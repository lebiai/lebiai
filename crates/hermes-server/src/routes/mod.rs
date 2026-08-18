//! Route table. All endpoints under `/api/v1`.
//! Serves the Flutter client surface: a subset of the `hermes-gui`
//! Tauri commands (chat via WebSocket frames) — NOT a 1:1 mirror.

use std::sync::Arc;

use axum::extract::State;
use axum::middleware::from_fn_with_state;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::auth::{auth_middleware, AuthState};
use crate::state::AppState;
use crate::tickets::TicketStore;

pub mod chat;
pub mod config;
pub mod inbox;
pub mod mcp;
pub mod memory;
pub mod reflect;
pub mod sessions;
pub mod skills;
pub mod uploads;

/// Build the full router with shared `Arc<AppState>`, every route gated by
/// auth middleware (Bearer, short-lived `?ticket=`, or legacy `?token=`).
pub fn build(state: Arc<AppState>, token: Arc<String>, tickets: Arc<TicketStore>) -> Router {
    let auth = AuthState {
        token: token.clone(),
        tickets: tickets.clone(),
    };
    Router::new()
        // health
        .route("/api/v1/health", get(health))
        // short-lived WS ticket (requires Bearer; avoids long-lived ?token= logs)
        .route("/api/v1/ws-ticket", post(issue_ws_ticket))
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
        // reflection + pending-review inbox (Flutter Evolve)
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
        .route(
            "/api/v1/inbox",
            get(inbox::list_pending).delete(inbox::reject_pending),
        )
        .route("/api/v1/inbox/count", get(inbox::count_pending))
        .route("/api/v1/inbox/accept", post(inbox::accept_pending))
        // document import
        .route(
            "/api/v1/uploads/converter",
            get(uploads::check_document_converter),
        )
        .route("/api/v1/uploads", post(uploads::import_document_handler))
        .layer(from_fn_with_state(auth, auth_middleware))
        .with_state(state)
}

async fn issue_ws_ticket(State(state): State<Arc<AppState>>) -> Json<Value> {
    // Behind auth middleware — only authenticated clients reach here.
    let ticket = state.ws_tickets.issue();
    Json(json!({
        "ticket": ticket,
        "expiresIn": TicketStore::ttl_secs(),
    }))
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}
