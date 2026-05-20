#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod context;
mod error;
mod events;
mod state;

use state::AppState;

fn main() {
    tracing_subscriber::fmt::init();

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
            commands::config::get_config,
            commands::config::update_config,
            commands::reflect::run_reflection,
            commands::reflect::accept_skill_candidate,
            commands::reflect::accept_memory_candidate,
            commands::reflect::handle_conflict,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
