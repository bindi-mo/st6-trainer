//! Purpose: verify the FFI struct definitions and the DLL load path.
//!
//! No video, no scanning — this is the CI gate for "are the FFI definitions
//! and the DLL placement still intact?".
//!
//! Run: `cargo run --example check_ffi`

#[path = "common/mod.rs"]
mod common;

use st6_trainer_lib::scanner_ffi::{
    FrameTelemetry, InputEvent, MatchMetadata, MoveEvent, c_arr_to_string_pub,
};

use common::{manifest_dir, report};

fn main() {
    let mut failures: Vec<String> = Vec::new();

    // Struct defaults must be zero-initialized at the FFI boundary.
    let meta = MatchMetadata::default();
    assert_eq!(c_arr_to_string_pub(&meta.match_id), "");
    let telemetry = FrameTelemetry::default();
    assert_eq!(telemetry.frame_index, 0);
    let input_event = InputEvent::default();
    assert_eq!(c_arr_to_string_pub(&input_event.player), "");
    let move_event = MoveEvent::default();
    assert_eq!(move_event.frame_start, 0);
    println!("struct defaults OK");

    // DLL must sit next to the manifest.
    let dll_path = manifest_dir().join("st6_scanner.dll");
    if !dll_path.exists() {
        failures.push(format!("DLL not found at {}", dll_path.display()));
        report(failures);
        return;
    }
    println!("DLL found at {}", dll_path.display());

    // Symbols must resolve.
    match common::scanner::ScannerLib::load_from(&dll_path) {
        Ok(lib) => {
            let v = lib.api_version();
            println!("scanner_api_version = {}.{}.{}", v.major, v.minor, v.patch);
        }
        Err(e) => failures.push(format!("symbol lookup failed: {}", e)),
    }

    report(failures);
}