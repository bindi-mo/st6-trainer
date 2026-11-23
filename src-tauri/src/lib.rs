// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

pub mod commands;
pub mod config;
pub mod db;
pub mod llm_protocol;
pub mod llm_worker;
pub mod model_downloader;
pub mod model_manager;
pub mod named_pipe;
pub mod paths;
pub mod pipeline;
pub mod rag;
pub mod scanner_ffi;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|_app| {
            // Phase 8: Write default config to $HOME/.st6-trainer/config.json on first launch
            let _ = crate::config::save_app_config_to_home(&crate::config::AppConfig::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::scan_video,
            commands::scanner_version,
            commands::set_manual_offsets,
            // Phase 8: Configuration & model management commands
            commands::get_app_config,
            commands::update_app_config,
            commands::check_model_exists,
            commands::verify_model,
            commands::download_model,
            // Phase 9: LLM worker commands
            commands::llm_worker_start,
            commands::llm_worker_status,
            commands::llm_infer,
            commands::llm_stream,
            commands::llm_cancel,
            commands::llm_worker_shutdown,
            // Phase 10: RAG & context assembly commands
            commands::rag_generate_analyze_damage,
            commands::rag_generate_verify_punish,
            commands::rag_generate_training_menu,
            commands::rag_list_matches,
            commands::rag_get_moves,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greet() {
        let result = greet("World");
        assert_eq!(result, "Hello, World! You've been greeted from Rust!");
    }

    #[test]
    fn test_greet_empty() {
        let result = greet("");
        assert_eq!(result, "Hello, ! You've been greeted from Rust!");
    }
}
