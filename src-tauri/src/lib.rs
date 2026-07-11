//! Library crate root. Modules are public so integration tests in `tests/` can
//! exercise the kernel directly with a MockClient.

pub mod anthropic;
pub mod ark;
pub mod commands;
pub mod config;
pub mod distill;
pub mod llm;
pub mod session;
pub mod workflow;

/// Deterministic mock LLM client, used by unit and integration tests.
pub mod mock;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            commands::load_settings(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::test_connection,
            commands::default_workflow_yaml,
            commands::coding_models,
            commands::validate_workflow,
            commands::list_workflows,
            commands::save_workflow,
            commands::load_workflow,
            commands::delete_workflow,
            commands::run_workflow,
            commands::list_sessions,
            commands::create_session,
            commands::chat_send,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
