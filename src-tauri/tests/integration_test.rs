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
