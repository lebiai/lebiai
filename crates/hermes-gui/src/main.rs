#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod context;
mod error;
mod events;
mod state;

use state::AppState;

fn main() {
    tracing_subscriber::fmt::init();

    if hermes_core::maybe_migrate_data_root() {
        tracing::info!("migrated legacy data directory to ~/.lebi-ai");
    }

    let state = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(AppState::init())
        .expect("failed to initialize app state");

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::chat::send_message,
            commands::chat::regenerate_turn,
            commands::chat::truncate_after_last_user,
            commands::chat::truncate_session,
            commands::chat::cancel_stream,
            commands::chat::respond_confirm,
            commands::session::list_sessions,
            commands::session::new_session,
            commands::session::load_session,
            commands::session::delete_session,
            commands::memory::list_memories,
            commands::memory::create_memory,
            commands::memory::delete_memory,
            commands::memory::toggle_pin_memory,
            commands::skills::list_skills,
            commands::skills::get_skill,
            commands::skills::save_skill,
            commands::skills::delete_skill,
            commands::mcp::list_mcp_tools,
            commands::mcp::list_mcp_servers,
            commands::onboarding::onboarding_seed_set,
            commands::onboarding::onboarding_seed_get,
            commands::config::get_config,
            commands::config::update_config,
            commands::config::open_api_key_guide,
            commands::data_dir::data_dir_get,
            commands::data_dir::data_dir_migrate,
            commands::data_dir::data_dir_reset,
            commands::reflect::run_reflection,
            commands::reflect::run_session_end_reflection,
            commands::reflect::accept_skill_candidate,
            commands::reflect::accept_memory_candidate,
            commands::reflect::handle_conflict,
            commands::inbox::list_pending_review,
            commands::inbox::count_pending_review,
            commands::inbox::accept_pending_review,
            commands::inbox::reject_pending_review,
            commands::upload::check_document_converter,
            commands::upload::check_markitdown,
            commands::upload::import_document,
            commands::wechat::wechat_login_start,
            commands::wechat::wechat_login_poll,
            commands::wechat::wechat_status,
            commands::wechat::wechat_start,
            commands::wechat::wechat_stop,
            commands::wechat::wechat_logout,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
