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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
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
            commands::commitment::list_commitments,
            commands::commitment::create_commitment,
            commands::commitment::accept_commitment,
            commands::commitment::reject_commitment,
            commands::commitment::close_commitment,
            commands::commitment::merge_commitments,
            commands::commitment::split_commitment,
            commands::commitment::update_commitment,
            commands::commitment::find_session_path,
            commands::memory::list_memories,
            commands::memory::create_memory,
            commands::memory::delete_memory,
            commands::memory::toggle_pin_memory,
            commands::skills::list_skills,
            commands::skills::get_skill,
            commands::skills::save_skill,
            commands::skills::delete_skill,
            commands::onboarding::onboarding_seed_set,
            commands::onboarding::onboarding_seed_get,
            commands::config::get_config,
            commands::config::update_config,
            commands::config::open_api_key_guide,
            commands::config::app_debug_build,
            commands::data_dir::data_dir_migrate,
            commands::data_dir::data_dir_pick,
            commands::data_dir::data_dir_reset,
            commands::review::get_review_prefs,
            commands::review::set_review_prefs,
            commands::review::dismiss_review_invite,
            commands::review::list_reviews,
            commands::review::run_period_review,
            commands::reflect::run_session_end_reflection,
            commands::reflect::session_needs_distill,
            commands::reflect::mark_pending_leave,
            commands::reflect::drain_pending_leave,
            commands::reflect::accept_skill_candidate,
            commands::reflect::accept_memory_candidate,
            commands::reflect::handle_conflict,
            commands::inbox::list_pending_review,
            commands::inbox::count_pending_review,
            commands::inbox::accept_pending_review,
            commands::inbox::reject_pending_review,
            commands::source::list_sources,
            commands::source::delete_source,
            commands::source::undo_source,
            commands::source::open_source,
            commands::source::keep_source,
            commands::source::preview_source,
            commands::upload::import_document,
            commands::wechat::wechat_login_start,
            commands::wechat::wechat_login_poll,
            commands::wechat::wechat_status,
            commands::wechat::wechat_start,
            commands::wechat::wechat_stop,
            commands::wechat::wechat_logout,
            commands::license::get_license_status,
            commands::license::apply_license,
            commands::license::mark_license_nudge_seen,
            commands::license::license_dev_tools_enabled,
            commands::license::license_dev_has_backup,
            commands::license::license_dev_simulate_expired,
            commands::license::license_dev_restore_backup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
