// Integration tests for ST6-Trainer backend
// These tests verify Tauri commands and backend functionality

use serial_test::serial;
use std::fs;
use tempfile::tempdir;

#[test]
#[serial]
fn test_temp_directory_creation() {
    // Test that we can create and use temp directories for testing
    let dir = tempdir().expect("Failed to create temp dir");
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "test content").expect("Failed to write test file");

    let content = fs::read_to_string(&file_path).expect("Failed to read test file");
    assert_eq!(content, "test content");

    // Temp dir is automatically cleaned up on drop
}

#[test]
#[serial]
fn test_json_serialization() {
    use serde_json::json;

    let data = json!({
        "character": "A.K.I.",
        "move": "Standing Light Punch",
        "startup": 5,
        "on_block": -1
    });

    let serialized = serde_json::to_string(&data).expect("Failed to serialize");
    let deserialized: serde_json::Value =
        serde_json::from_str(&serialized).expect("Failed to deserialize");

    assert_eq!(deserialized["character"], "A.K.I.");
    assert_eq!(deserialized["startup"], 5);
}

// Phase 7: Error code tests (§7 / 付録B)
#[test]
fn test_error_code_strings() {
    // Verify all documented error codes are valid string constants
    let error_codes = [
        "E_CALIB_AMBIGUOUS",
        "E_CALIB_LOW_CONF",
        "E_HUD_ABSENT",
        "E_ROI_OUT_OF_BOUNDS",
        "E_ROI_GROUP_OVERLAP",
        "E_ASPECT_UNSUPPORTED",
        "E_ANISOTROPIC_SCALE",
        "E_VFR_UNSUPPORTED",
        "E_INPUT_INVARIANT",
        "E_SCHEMA_INVARIANT",
        "ATTACK_PROPERTY_UNSUPPORTED",
    ];

    for code in &error_codes {
        assert!(!code.is_empty());
        assert!(code.starts_with('E') || code.starts_with('A'));
    }
}

// Phase 7: Pipeline invariant tests (§7.5)
#[test]
fn test_pipeline_input_invariants_all_pass() {
    use st6_trainer_lib::pipeline::{JsonlInputEvent, check_input_invariants};

    let inputs = vec![
        JsonlInputEvent {
            frame_start: 0,
            frame_end: 5,
            frame_span_video: 0,
            hold_frames: 0,
            counter_clipped: 0,
            hold_frames_lower_bound: 0,
            time_dilation_suspected: 0,
            resync_used: 0,
            unknown_glyph: 0,
            is_preround: 0,
            player: "P1".to_string(),
            round_index: 1,
            event_index: 0,
            directions: vec!["6".to_string()],
            buttons: vec!["LP".to_string()],
            quality_flags: vec![],
        },
        JsonlInputEvent {
            frame_start: 6,
            frame_end: 10,
            frame_span_video: 0,
            hold_frames: 0,
            counter_clipped: 0,
            hold_frames_lower_bound: 0,
            time_dilation_suspected: 0,
            resync_used: 0,
            unknown_glyph: 0,
            is_preround: 0,
            player: "P2".to_string(),
            round_index: 1,
            event_index: 0,
            directions: vec!["4".to_string()],
            buttons: vec!["HP".to_string()],
            quality_flags: vec![],
        },
    ];

    let mut violations = Vec::new();
    check_input_invariants(&inputs, &mut violations);
    assert!(
        violations.is_empty(),
        "Expected no violations, got: {:?}",
        violations
    );
}

#[test]
fn test_pipeline_schema_invariants_all_pass() {
    use st6_trainer_lib::pipeline::{JsonlFrameTelemetry, check_schema_invariants};

    let telemetry = vec![
        JsonlFrameTelemetry {
            frame_index: 0,
            source_frame_index: 0,
            timestamp_ms: 0,
            round_index: 1,
            hp_reset: 0,
            hp_p1: 1.0,
            hp_p2: 1.0,
            drive_p1: 6.0,
            drive_p2: 6.0,
            sa_p1: 3,
            sa_p2: 3,
            in_burnout_p1: 0,
            in_burnout_p2: 0,
            quality_flags: vec![],
        },
        JsonlFrameTelemetry {
            frame_index: 1,
            source_frame_index: 1,
            timestamp_ms: 16,
            round_index: 1,
            hp_reset: 0,
            hp_p1: 0.95,
            hp_p2: 1.0,
            drive_p1: 6.0,
            drive_p2: 6.0,
            sa_p1: 3,
            sa_p2: 3,
            in_burnout_p1: 0,
            in_burnout_p2: 0,
            quality_flags: vec![],
        },
    ];

    let inputs = Vec::new();
    let moves = Vec::new();
    let mut violations = Vec::new();
    check_schema_invariants(&telemetry, &inputs, &moves, &mut violations);
    assert!(
        violations.is_empty(),
        "Expected no violations, got: {:?}",
        violations
    );
}

// Phase 7: E2E smoke test via scanner_ffi
#[test]
#[serial]
fn test_scanner_e2e_round_trip() {
    use st6_trainer_lib::scanner_ffi::{Scanner, c_arr_to_string_pub};
    use std::path::PathBuf;

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let video_path = manifest_dir.join("videos/replay_00.mkv");
    if !video_path.exists() {
        eprintln!("SKIP: replay video not found at {}", video_path.display());
        return;
    }

    let scanner = Scanner::new_from_path(&video_path, None).expect("Scanner::new_from_path failed");
    let metadata = scanner.get_metadata().expect("get_metadata failed");

    // Verify metadata fields are populated
    assert!(!c_arr_to_string_pub(&metadata.match_id).is_empty());
    assert!(metadata.top_y_offset_px >= 0);
    assert!(metadata.bottom_y_offset_px >= 0);
    let ct1 = c_arr_to_string_pub(&metadata.control_type_p1);
    let ct2 = c_arr_to_string_pub(&metadata.control_type_p2);
    assert!(
        ct1 == "classic" || ct1 == "modern" || ct1.is_empty(),
        "unexpected control_type_p1: {}",
        ct1
    );
    assert!(
        ct2 == "classic" || ct2 == "modern" || ct2.is_empty(),
        "unexpected control_type_p2: {}",
        ct2
    );

    // Iterate telemetry
    let mut frame_count = 0u32;
    while let Some(telemetry) = scanner.next_telemetry().expect("next_telemetry failed") {
        frame_count += 1;
        // §7.5 項 6: HP 値は 0.0-1.0 の範囲
        assert!(
            telemetry.hp_p1 >= 0.0 && telemetry.hp_p1 <= 1.0,
            "hp_p1 out of range at frame {}",
            telemetry.frame_index
        );
        assert!(
            telemetry.hp_p2 >= 0.0 && telemetry.hp_p2 <= 1.0,
            "hp_p2 out of range at frame {}",
            telemetry.frame_index
        );
        // §7.5 項 7: SA 値は 0-3 の範囲
        assert!(
            telemetry.sa_p1 >= 0 && telemetry.sa_p1 <= 3,
            "sa_p1 out of range at frame {}",
            telemetry.frame_index
        );
        assert!(
            telemetry.sa_p2 >= 0 && telemetry.sa_p2 <= 3,
            "sa_p2 out of range at frame {}",
            telemetry.frame_index
        );

        if frame_count > 10000 {
            break;
        }
    }

    assert!(frame_count > 0, "Expected at least one telemetry frame");
    println!("E2E round-trip: {} telemetry frames read", frame_count);
}
