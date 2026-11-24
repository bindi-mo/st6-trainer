//! Tauri commands for the scanner pipeline.
//!
//! Exposes scanner_ffi and pipeline functionality to the frontend.

use crate::pipeline::{InvariantResult, run_pipeline};
use crate::scanner_ffi::{
    FrameTelemetryOut, InputEventOut, MatchMetadataOut, MoveEventOut, Scanner,
};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Emitter;

/// Result of a scan operation
#[derive(Debug, Serialize)]
pub struct ScanResult {
    pub metadata: MatchMetadataOut,
    pub telemetry: Vec<FrameTelemetryOut>,
    pub input_events: Vec<InputEventOut>,
    pub move_events: Vec<MoveEventOut>,
    pub invariant_result: InvariantResult,
}

/// Run a full scan on a video file and return all results.
#[tauri::command]
pub async fn scan_video(
    app: tauri::AppHandle,
    video_path: String,
    output_dir: Option<String>,
) -> Result<ScanResult, String> {
    let scanner = Scanner::new(&app, &video_path, None).map_err(|e| e.to_string())?;

    // Get metadata
    let metadata = scanner.get_metadata().map_err(|e| e.to_string())?;
    let metadata_out = MatchMetadataOut::from(metadata);

    // Collect telemetry
    let mut telemetry = Vec::new();
    while let Some(t) = scanner.next_telemetry().map_err(|e| e.to_string())? {
        telemetry.push(FrameTelemetryOut::from(t));
    }

    // Re-create scanner for input events (separate stream)
    let scanner2 = Scanner::new(&app, &video_path, None).map_err(|e| e.to_string())?;
    scanner2.get_metadata().map_err(|e| e.to_string())?;

    let mut input_events = Vec::new();
    while let Some(e) = scanner2.next_input_event().map_err(|e| e.to_string())? {
        input_events.push(InputEventOut::from(e));
    }

    // Re-create scanner for move events
    let scanner3 = Scanner::new(&app, &video_path, None).map_err(|e| e.to_string())?;
    scanner3.get_metadata().map_err(|e| e.to_string())?;

    let mut move_events = Vec::new();
    while let Some(e) = scanner3.next_move_event().map_err(|e| e.to_string())? {
        move_events.push(MoveEventOut::from(e));
    }

    // Build JSONL for pipeline
    let telemetry_jsonl: String = telemetry
        .iter()
        .map(|t| serde_json::to_string(t).unwrap())
        .collect::<Vec<_>>()
        .join("\n");

    let input_jsonl: String = input_events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect::<Vec<_>>()
        .join("\n");

    let move_jsonl: String = move_events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect::<Vec<_>>()
        .join("\n");

    // Build match metadata record for DB insertion
    let match_record = crate::db::MatchSummary {
        id: metadata_out.match_id.clone(),
        played_at: Some(metadata_out.extracted_at_utc.clone()),
        my_character: metadata_out.character_name_p1.clone(),
        opponent_character: metadata_out.character_name_p2.clone(),
        my_control_type: metadata_out.control_type_p1.clone(),
        opponent_control_type: metadata_out.control_type_p2.clone(),
        total_rounds: 0,
        result: String::new(),
        source_video_path: Some(video_path.clone()),
        scan_config_version: Some(metadata_out.config_version.clone()),
    };

    // Run invariant pipeline (writes to SQLite via save_scan_result)
    let db_path = match &output_dir {
        Some(dir) => PathBuf::from(dir).join("extraction.db"),
        None => PathBuf::from("extraction.db"),
    };

    let invariant_result = run_pipeline(
        &match_record,
        &telemetry_jsonl,
        &input_jsonl,
        &move_jsonl,
        &db_path,
    )
    .map_err(|e| e.to_string())?;

    Ok(ScanResult {
        metadata: metadata_out,
        telemetry,
        input_events,
        move_events,
        invariant_result,
    })
}

/// Get scanner API version
#[tauri::command]
pub fn scanner_version() -> Result<[i32; 3], String> {
    // Load DLL directly to check version
    let dll_path = std::env::current_dir()
        .unwrap_or_default()
        .join("st6_scanner.dll");
    if !dll_path.exists() {
        return Err(format!("DLL not found at {}", dll_path.display()));
    }

    let lib = unsafe { libloading::Library::new(&dll_path) }
        .map_err(|e| format!("Library::new: {}", e))?;

    unsafe {
        let api_version: libloading::Symbol<
            unsafe extern "C" fn() -> crate::scanner_ffi::ScannerVersion,
        > = lib
            .get(b"scanner_api_version")
            .map_err(|e| format!("scanner_api_version: {}", e))?;
        let v = api_version();
        Ok([v.major, v.minor, v.patch])
    }
}

/// Set manual calibration offsets (Phase 6)
#[tauri::command]
pub fn set_manual_offsets(
    app: tauri::AppHandle,
    video_path: String,
    top_y_offset_px: i32,
    bottom_y_offset_px: i32,
    manual_verified: bool,
) -> Result<bool, String> {
    let scanner = Scanner::new(&app, &video_path, None).map_err(|e| e.to_string())?;

    // First get metadata to ensure calibration is done
    scanner.get_metadata().map_err(|e| e.to_string())?;

    let mv = if manual_verified { 1 } else { 0 };
    scanner
        .set_manual_offsets(top_y_offset_px, bottom_y_offset_px, mv)
        .map_err(|e| e.to_string())
}

// ===== Phase 8: Configuration & Model Management Commands =====

/// Get the current app configuration from `$HOME/.st6-trainer/config.json`.
#[tauri::command]
pub fn get_app_config() -> Result<crate::config::AppConfig, String> {
    Ok(crate::config::load_app_config_from_home())
}

/// Update the app configuration and persist to `$HOME/.st6-trainer/config.json`.
#[tauri::command]
pub fn update_app_config(config: crate::config::AppConfig) -> Result<(), String> {
    crate::config::save_app_config_to_home(&config)
}

/// Check whether a model file exists in `$HOME/.st6-trainer/models/`.
#[tauri::command]
pub fn check_model_exists(model_file_name: String) -> Result<bool, String> {
    crate::model_manager::model_exists(&model_file_name)
}

/// List all model files in `$HOME/.st6-trainer/models/`.
#[tauri::command]
pub fn list_models() -> Result<Vec<String>, String> {
    crate::model_manager::list_models()
        .map(|paths| paths.iter().filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).collect())
}

/// Validate a model file (GGUF header + optional SHA256).
#[tauri::command]
pub fn verify_model(
    model_file_name: String,
    expected_sha256: Option<String>,
) -> Result<crate::model_manager::ModelValidationResult, String> {
    crate::model_manager::validate_model(&model_file_name, expected_sha256.as_deref())
}

type ProgressCallback = Box<dyn Fn(u64, Option<u64>) + Send + Sync>;

/// Download a model file from a URL to `$HOME/.st6-trainer/models/`.
///
/// Emits progress via the `model-download-progress` Tauri event.
#[tauri::command]
pub async fn download_model(
    app: tauri::AppHandle,
    url: String,
    model_file_name: String,
) -> Result<crate::model_downloader::DownloadResult, String> {
    let app_clone = app.clone();
    let model_file_name_clone = model_file_name.clone();
    let progress: Option<ProgressCallback> =
        Some(Box::new(move |bytes_downloaded, total_bytes| {
            let _ = app_clone.emit("model-download-progress", serde_json::json!({
                "model_file_name": model_file_name_clone,
                "bytes_downloaded": bytes_downloaded,
                "total_bytes": total_bytes,
            }));
        }));

    crate::model_downloader::download_model(&url, &model_file_name, progress).await
}

// ===== Phase 13: Frame Data Master JSON Import Commands =====

/// Import frame data master JSON files into the SQLite `moves` table.
///
/// Performs differential import: only files whose mtime or SHA256 hash
/// have changed since the last import are processed.
#[tauri::command]
pub async fn import_frame_data(
    app: tauri::AppHandle,
) -> Result<crate::frame_data_importer::ImportStats, String> {
    crate::frame_data_importer::import_frame_data(Some(&app))
}

// ===== Phase 10: RAG & Context Assembly Commands =====
#[tauri::command]
pub async fn rag_generate_analyze_damage(
    match_id: String,
) -> Result<crate::rag::PromptResult, String> {
    let conn = crate::db::open_db().map_err(|e| e.to_string())?;
    crate::db::init_schema(&conn).map_err(|e| e.to_string())?;

    let engine = crate::rag::RagEngine::new(&std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prompts"))
        .map_err(|e| e.to_string())?;
    engine.render_analyze_damage(&conn, &match_id)
}

/// Generate a verify_punish prompt from a match in SQLite.
#[tauri::command]
pub async fn rag_generate_verify_punish(
    match_id: String,
    guarded_move: String,
    guard_frame: i32,
) -> Result<crate::rag::PromptResult, String> {
    let conn = crate::db::open_db().map_err(|e| e.to_string())?;
    crate::db::init_schema(&conn).map_err(|e| e.to_string())?;

    let engine = crate::rag::RagEngine::new(&std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prompts"))
        .map_err(|e| e.to_string())?;
    engine.render_verify_punish(&conn, &match_id, &guarded_move, guard_frame)
}

/// Generate a training_menu prompt from a match in SQLite.
#[tauri::command]
pub async fn rag_generate_training_menu(
    match_id: String,
) -> Result<crate::rag::PromptResult, String> {
    let conn = crate::db::open_db().map_err(|e| e.to_string())?;
    crate::db::init_schema(&conn).map_err(|e| e.to_string())?;

    let engine = crate::rag::RagEngine::new(&std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prompts"))
        .map_err(|e| e.to_string())?;
    engine.render_training_menu(&conn, &match_id)
}

/// List recent matches from SQLite for RAG context selection.
#[tauri::command]
pub async fn rag_list_matches(limit: Option<i32>) -> Result<Vec<crate::db::MatchSummary>, String> {
    let conn = crate::db::open_db().map_err(|e| e.to_string())?;
    crate::db::init_schema(&conn).map_err(|e| e.to_string())?;
    crate::db::list_matches(&conn, limit.unwrap_or(20))
}

/// Get frame data moves for a character/control type from SQLite.
#[tauri::command]
pub async fn rag_get_moves(
    character_id: String,
    control_type: String,
) -> Result<Vec<crate::db::MoveRecord>, String> {
    let conn = crate::db::open_db().map_err(|e| e.to_string())?;
    crate::db::init_schema(&conn).map_err(|e| e.to_string())?;
    crate::db::get_moves_for_character(&conn, &character_id, &control_type)
}

// ===== Phase 14: Frontend feature expansion commands =====

/// List distinct character IDs available in the moves table.
#[tauri::command]
pub async fn get_characters() -> Result<Vec<crate::db::CharacterSummary>, String> {
    let conn = crate::db::open_db().map_err(|e| e.to_string())?;
    crate::db::init_schema(&conn).map_err(|e| e.to_string())?;
    crate::db::list_characters(&conn)
}

/// Get the full timeline for a match: move events + input events merged chronologically.
#[tauri::command]
pub async fn get_match_timeline(match_id: String) -> Result<Vec<crate::db::TimelineEntry>, String> {
    let conn = crate::db::open_db().map_err(|e| e.to_string())?;
    crate::db::init_schema(&conn).map_err(|e| e.to_string())?;
    crate::db::get_match_timeline(&conn, &match_id)
}

/// Get frame telemetry for a match (for timeline frame-level detail).
#[tauri::command]
pub async fn get_match_telemetry(match_id: String) -> Result<Vec<crate::db::FrameTelemetryRecord>, String> {
    let conn = crate::db::open_db().map_err(|e| e.to_string())?;
    crate::db::init_schema(&conn).map_err(|e| e.to_string())?;
    crate::db::get_frame_telemetry(&conn, &match_id)
}

// ===== Phase 9: LLM Worker Commands =====

/// Global LLM worker instance.
static LLM_WORKER: std::sync::OnceLock<tokio::sync::Mutex<crate::llm_worker::LlmWorker>> =
    std::sync::OnceLock::new();

fn get_llm_worker() -> &'static tokio::sync::Mutex<crate::llm_worker::LlmWorker> {
    LLM_WORKER.get_or_init(|| {
        tokio::sync::Mutex::new(crate::llm_worker::LlmWorker::new(
            crate::llm_worker::WorkerConfig::default(),
        ))
    })
}

/// Start the LLM worker subprocess and connect via Named Pipe.
#[tauri::command]
pub async fn llm_worker_start(
    _app: tauri::AppHandle,
    model_path: String,
    ctx_size: Option<u32>,
    gpu_layers: Option<i32>,
) -> Result<crate::llm_worker::WorkerStatus, String> {
    let config = crate::llm_worker::WorkerConfig {
        model_path,
        ctx_size: ctx_size.unwrap_or(4096),
        gpu_layers: gpu_layers.unwrap_or(35),
        pipe_name: crate::named_pipe::DEFAULT_PIPE_NAME.to_string(),
        ..Default::default()
    };

    let worker = get_llm_worker();
    let mut w = worker.lock().await;
    *w = crate::llm_worker::LlmWorker::new(config);
    w.start().await.map_err(|e| e.to_string())?;

    Ok(w.status().await)
}

/// Get the current LLM worker status.
#[tauri::command]
pub async fn llm_worker_status() -> Result<crate::llm_worker::WorkerStatus, String> {
    let worker = get_llm_worker();
    let w = worker.lock().await;
    Ok(w.status().await)
}

/// Run non-streaming LLM inference.
#[tauri::command]
pub async fn llm_infer(
    prompt: String,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_tokens: Option<u32>,
) -> Result<crate::llm_worker::InferResult, String> {
    let worker = get_llm_worker();
    let w = worker.lock().await;

    let params = crate::llm_protocol::InferParams {
        temperature,
        top_p,
        max_tokens,
        stream: Some(false),
    };

    w.infer(&prompt, Some(params)).await
}

/// Run streaming LLM inference with token callbacks via Tauri events.
#[tauri::command]
pub async fn llm_stream(
    app: tauri::AppHandle,
    prompt: String,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_tokens: Option<u32>,
) -> Result<crate::llm_worker::InferResult, String> {
    let app_clone = app.clone();
    let worker = get_llm_worker();
    let w = worker.lock().await;

    let params = crate::llm_protocol::InferParams {
        temperature,
        top_p,
        max_tokens,
        stream: Some(true),
    };

    let token_callback: crate::llm_worker::TokenCallback = Arc::new(move |token: String| {
        let _ = app_clone.emit("llm-stream-token", serde_json::json!({
            "token": token,
        }));
    });

    w.infer_stream(&prompt, Some(params), token_callback).await
}

/// Cancel the current LLM inference.
#[tauri::command]
pub async fn llm_cancel() -> Result<(), String> {
    let worker = get_llm_worker();
    let w = worker.lock().await;
    w.cancel().await
}

/// Shutdown the LLM worker subprocess.
#[tauri::command]
pub async fn llm_worker_shutdown() -> Result<(), String> {
    let worker = get_llm_worker();
    let w = worker.lock().await;
    w.shutdown().await
}
