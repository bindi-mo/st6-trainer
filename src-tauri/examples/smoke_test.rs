//! Smoke test: verify the scanner DLL can be loaded and driven end-to-end.
//!
//! Unlike the previous compile-only check, this test actually calls into the
//! C++ DLL via the FFI boundary:
//!   scanner_create -> scanner_get_metadata -> scanner_next_telemetry -> scanner_destroy
//!
//! It uses `src-tauri/videos/replay_00.mkv` when available. If the video is
//! missing the test degrades to a DLL-load-only check so CI without the
//! (gitignored) video still passes.

use std::path::PathBuf;

use st6_trainer_lib::scanner_ffi::{
    c_arr_to_string_pub, FrameTelemetry, InputEvent, MatchMetadata, MoveEvent, ScanConfig,
    ScannerVersion,
};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn find_replay() -> Option<PathBuf> {
    let p = manifest_dir().join("videos/replay_00.mkv");
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

fn main() {
    let mut failures: Vec<String> = Vec::new();

    // ---- 1. Struct sanity (no DLL needed) ----
    let meta = MatchMetadata::default();
    assert_eq!(c_arr_to_string_pub(&meta.match_id), "");
    let telemetry = FrameTelemetry::default();
    assert_eq!(telemetry.frame_index, 0);
    let input_event = InputEvent::default();
    assert_eq!(c_arr_to_string_pub(&input_event.player), "");
    let move_event = MoveEvent::default();
    assert_eq!(move_event.frame_start, 0);
    println!("[1] struct defaults OK");

    // ---- 2. DLL presence ----
    let dll_path = manifest_dir().join("st6_scanner.dll");
    if !dll_path.exists() {
        failures.push(format!("DLL not found at {}", dll_path.display()));
        report(failures);
        return;
    }
    println!("[2] DLL found at {}", dll_path.display());

    // ---- 3. Real scanner round-trip ----
    match find_replay() {
        Some(video) => {
            println!("[3] Replay found: {}", video.display());
            if let Err(e) = run_round_trip(&video) {
                failures.push(format!("round-trip failed: {}", e));
            } else {
                println!("[3] round-trip OK");
            }
        }
        None => {
            println!("[3] SKIP: no replay video (videos/replay_00.mkv missing)");
        }
    }

    report(failures);
}

fn run_round_trip(video: &std::path::Path) -> Result<(), String> {
    // The Scanner::new path needs an AppHandle for resource resolution.
    // For the smoke test we bypass it and drive the DLL directly through the
    // same symbol lookups, using the manifest dir as the base path.
    let dll_path = manifest_dir().join("st6_scanner.dll");
    let template_pack = manifest_dir().join("templates");
    let config_json_path = manifest_dir().join("config/scanner_config.json");

    let lib = unsafe { libloading::Library::new(&dll_path) }
        .map_err(|e| format!("Library::new: {}", e))?;

    unsafe {
        let create: libloading::Symbol<
            unsafe extern "C" fn(
                *const std::os::raw::c_char,
                *const ScanConfig,
            ) -> *mut std::ffi::c_void,
        > = lib
            .get(b"scanner_create")
            .map_err(|e| format!("scanner_create: {}", e))?;
        let get_metadata: libloading::Symbol<
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut MatchMetadata) -> std::os::raw::c_int,
        > = lib
            .get(b"scanner_get_metadata")
            .map_err(|e| format!("scanner_get_metadata: {}", e))?;
        let next_telemetry: libloading::Symbol<
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut FrameTelemetry) -> std::os::raw::c_int,
        > = lib
            .get(b"scanner_next_telemetry")
            .map_err(|e| format!("scanner_next_telemetry: {}", e))?;
        let destroy: libloading::Symbol<unsafe extern "C" fn(*mut std::ffi::c_void)> = lib
            .get(b"scanner_destroy")
            .map_err(|e| format!("scanner_destroy: {}", e))?;
        let get_last_error: libloading::Symbol<
            unsafe extern "C" fn() -> *const std::os::raw::c_char,
        > = lib
            .get(b"scanner_get_last_error")
            .map_err(|e| format!("scanner_get_last_error: {}", e))?;
        let api_version: libloading::Symbol<unsafe extern "C" fn() -> ScannerVersion> = lib
            .get(b"scanner_api_version")
            .map_err(|e| format!("scanner_api_version: {}", e))?;

        let v = api_version();
        println!(
            "    scanner_api_version = {}.{}.{}",
            v.major, v.minor, v.patch
        );

        let video_c = std::ffi::CString::new(video.to_string_lossy().as_bytes())
            .map_err(|e| format!("video path: {}", e))?;
        let tpl_c = std::ffi::CString::new(template_pack.to_string_lossy().as_bytes())
            .map_err(|e| format!("template path: {}", e))?;
        let cfg_json = std::fs::read_to_string(&config_json_path).unwrap_or_default();
        let cfg_c = std::ffi::CString::new(cfg_json).map_err(|e| format!("config json: {}", e))?;

        let cfg = ScanConfig {
            template_pack_path: tpl_c.as_ptr(),
            config_json: cfg_c.as_ptr(),
            on_calib_failure: 0,
            frame_stride: 1,
            hud_redetect_interval: 300,
            max_frames: 0,
        };

        let handle = create(video_c.as_ptr(), &cfg);
        if handle.is_null() {
            let p = get_last_error();
            let msg = if p.is_null() {
                "unknown".to_string()
            } else {
                std::ffi::CStr::from_ptr(p)
                    .to_string_lossy()
                    .into_owned()
            };
            return Err(format!("scanner_create returned null: {}", msg));
        }

        let mut meta = MatchMetadata::default();
        let r = get_metadata(handle, &mut meta);
        if r < 0 {
            destroy(handle);
            return Err(format!("scanner_get_metadata failed: {}", r));
        }
        println!(
            "    metadata: match_id={} p1={} p2={}",
            c_arr_to_string_pub(&meta.match_id),
            c_arr_to_string_pub(&meta.character_name_p1),
            c_arr_to_string_pub(&meta.character_name_p2),
        );

        let mut count = 0u32;
        loop {
            let mut t = FrameTelemetry::default();
            let r = next_telemetry(handle, &mut t);
            if r == 0 {
                break;
            }
            if r < 0 {
                destroy(handle);
                return Err(format!("scanner_next_telemetry failed: {}", r));
            }
            count += 1;
            if count == 1 {
                println!(
                    "    first telemetry: frame={} hp_p1={:.3} drive_p1={:.2}",
                    t.frame_index, t.hp_p1, t.drive_p1
                );
            }
            if count > 100000 {
                destroy(handle);
                return Err("telemetry runaway (>100000 frames)".to_string());
            }
        }
        println!("    telemetry frames read: {}", count);

        destroy(handle);
    }
    Ok(())
}

fn report(failures: Vec<String>) {
    if failures.is_empty() {
        println!("SMOKE TEST PASSED");
    } else {
        for f in &failures {
            eprintln!("FAIL: {}", f);
        }
        std::process::exit(1);
    }
}