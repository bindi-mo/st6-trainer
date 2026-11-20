//! Tauri commands for the scanner pipeline.
//!
//! Exposes scanner_ffi and pipeline functionality to the frontend.

use crate::pipeline::{run_pipeline, InvariantResult};
use crate::scanner_ffi::{
    FrameTelemetryOut, InputEventOut, MatchMetadataOut, MoveEventOut, Scanner,
};
use serde::Serialize;
use std::path::PathBuf;

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
    loop {
        match scanner.next_telemetry().map_err(|e| e.to_string())? {
            Some(t) => telemetry.push(FrameTelemetryOut::from(t)),
            None => break,
        }
    }

    // Re-create scanner for input events (separate stream)
    let scanner2 = Scanner::new(&app, &video_path, None).map_err(|e| e.to_string())?;
    scanner2.get_metadata().map_err(|e| e.to_string())?;

    let mut input_events = Vec::new();
    loop {
        match scanner2.next_input_event().map_err(|e| e.to_string())? {
            Some(e) => input_events.push(InputEventOut::from(e)),
            None => break,
        }
    }

    // Re-create scanner for move events
    let scanner3 = Scanner::new(&app, &video_path, None).map_err(|e| e.to_string())?;
    scanner3.get_metadata().map_err(|e| e.to_string())?;

    let mut move_events = Vec::new();
    loop {
        match scanner3.next_move_event().map_err(|e| e.to_string())? {
            Some(e) => move_events.push(MoveEventOut::from(e)),
            None => break,
        }
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

    // Run invariant pipeline
    let db_path = match &output_dir {
        Some(dir) => PathBuf::from(dir).join("extraction.db"),
        None => PathBuf::from("extraction.db"),
    };

    let invariant_result =
        run_pipeline(&telemetry_jsonl, &input_jsonl, &move_jsonl, &db_path)
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
        let api_version: libloading::Symbol<unsafe extern "C" fn() -> crate::scanner_ffi::ScannerVersion> =
            lib.get(b"scanner_api_version")
                .map_err(|e| format!("scanner_api_version: {}", e))?;
        let v = api_version();
        Ok([v.major, v.minor, v.patch])
    }
}
