use st6_trainer_lib::scanner_ffi::{FrameTelemetry, InputEvent, MatchMetadata, MoveEvent};
use std::path::PathBuf;

// Helper to convert fixed c_char array to String
fn c_arr_to_string(arr: &[i8]) -> String {
    let len = arr.iter().position(|&c| c == 0).unwrap_or(arr.len());
    let bytes: Vec<u8> = arr[..len].iter().map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a minimal Tauri AppHandle for resource resolution
    // Use tauri::test::mock_app() requires "test" feature, so we create a dummy app context
    // For smoke test, we just need the resource_dir resolution which works with a real app
    // Here we use a simpler approach: directly construct the DLL path
    let dll_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("st6_scanner.dll");
    if !dll_path.exists() {
        eprintln!("DLL not found at {:?}, trying relative path", dll_path);
        // Fallback: assume running from target/debug/examples
        let alt = PathBuf::from("../../../st6_scanner.dll");
        if alt.exists() {
            unsafe {
                std::env::set_var("ST6_SCANNER_DLL_OVERRIDE", alt);
            }
        }
    }

    // Use a dummy video file (create empty file for test)
    let test_video = PathBuf::from("test_dummy.mp4");
    if !test_video.exists() {
        std::fs::write(&test_video, b"dummy")?;
    }

    println!("Creating scanner...");
    // We need an AppHandle - use a minimal one via tauri::test (requires test feature)
    // For now, skip actual scanner creation and just verify the FFI module compiles
    println!("Scanner module compiled successfully");

    // Test struct conversions
    let meta = MatchMetadata::default();
    println!(
        "MatchMetadata default created: match_id={:?}",
        c_arr_to_string(&meta.match_id)
    );

    let telemetry = FrameTelemetry::default();
    println!(
        "FrameTelemetry default created: frame_index={}",
        telemetry.frame_index
    );

    let input_event = InputEvent::default();
    println!(
        "InputEvent default created: player={:?}",
        c_arr_to_string(&input_event.player)
    );

    let move_event = MoveEvent::default();
    println!(
        "MoveEvent default created: frame_start={}",
        move_event.frame_start
    );

    // Cleanup
    let _ = std::fs::remove_file(test_video);

    println!("SMOKE TEST PASSED (compile-time only)");
    Ok(())
}
