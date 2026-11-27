//! Direct FFI driver for `st6_scanner.dll`.
//!
//! Bypasses `Scanner::new` (which needs a Tauri `AppHandle` for resource
//! resolution) and drives the DLL through the same symbol lookups, using the
//! manifest dir as the base path.

use std::ffi::{CStr, CString};
use std::path::Path;

use st6_trainer_lib::scanner_ffi::{
    FrameTelemetry, MatchMetadata, ScanConfig, ScannerVersion, c_arr_to_string_pub,
};

use super::manifest_dir;

/// Handle to a live scanner instance created by [`ScannerLib::create`].
pub struct ScannerHandle<'lib> {
    lib: &'lib ScannerLib,
    raw: *mut std::ffi::c_void,
}

impl ScannerHandle<'_> {
    pub fn metadata(&self) -> Result<MatchMetadata, String> {
        let mut meta = MatchMetadata::default();
        let r = unsafe { (self.lib.get_metadata)(self.raw, &mut meta) };
        if r < 0 {
            return Err(format!("scanner_get_metadata failed: {}", r));
        }
        Ok(meta)
    }

    /// Pull the next telemetry frame. `Ok(None)` means end of stream.
    pub fn next_telemetry(&self) -> Result<Option<FrameTelemetry>, String> {
        let mut t = FrameTelemetry::default();
        let r = unsafe { (self.lib.next_telemetry)(self.raw, &mut t) };
        match r {
            0 => Ok(None),
            1 => Ok(Some(t)),
            _ => Err(format!("scanner_next_telemetry failed: {}", r)),
        }
    }
}

impl Drop for ScannerHandle<'_> {
    fn drop(&mut self) {
        unsafe { (self.lib.destroy)(self.raw) };
    }
}

/// Loaded DLL with the symbol table resolved.
pub struct ScannerLib {
    _lib: libloading::Library,
    create: unsafe extern "C" fn(*const std::os::raw::c_char, *const ScanConfig) -> *mut std::ffi::c_void,
    get_metadata: unsafe extern "C" fn(*mut std::ffi::c_void, *mut MatchMetadata) -> std::os::raw::c_int,
    next_telemetry: unsafe extern "C" fn(*mut std::ffi::c_void, *mut FrameTelemetry) -> std::os::raw::c_int,
    destroy: unsafe extern "C" fn(*mut std::ffi::c_void),
    get_last_error: unsafe extern "C" fn() -> *const std::os::raw::c_char,
    api_version: unsafe extern "C" fn() -> ScannerVersion,
}

impl ScannerLib {
    /// Load `st6_scanner.dll` from the manifest dir.
    pub fn load() -> Result<Self, String> {
        let dll_path = manifest_dir().join("st6_scanner.dll");
        Self::load_from(&dll_path)
    }

    pub fn load_from(dll_path: &Path) -> Result<Self, String> {
        let lib = unsafe { libloading::Library::new(dll_path) }
            .map_err(|e| format!("Library::new ({}): {}", dll_path.display(), e))?;
        unsafe {
            Ok(Self {
                create: *lib.get(b"scanner_create").map_err(|e| format!("scanner_create: {}", e))?,
                get_metadata: *lib
                    .get(b"scanner_get_metadata")
                    .map_err(|e| format!("scanner_get_metadata: {}", e))?,
                next_telemetry: *lib
                    .get(b"scanner_next_telemetry")
                    .map_err(|e| format!("scanner_next_telemetry: {}", e))?,
                destroy: *lib
                    .get(b"scanner_destroy")
                    .map_err(|e| format!("scanner_destroy: {}", e))?,
                get_last_error: *lib
                    .get(b"scanner_get_last_error")
                    .map_err(|e| format!("scanner_get_last_error: {}", e))?,
                api_version: *lib
                    .get(b"scanner_api_version")
                    .map_err(|e| format!("scanner_api_version: {}", e))?,
                _lib: lib,
            })
        }
    }

    pub fn api_version(&self) -> ScannerVersion {
        unsafe { (self.api_version)() }
    }

    fn last_error(&self) -> String {
        let p = unsafe { (self.get_last_error)() };
        if p.is_null() {
            "unknown".to_string()
        } else {
            unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
        }
    }

    /// `scanner_create` with the standard config, capped at `max_frames`
    /// (`0` = scan to end of stream).
    pub fn create(&self, video: &Path, max_frames: i32) -> Result<ScannerHandle<'_>, String> {
        let template_pack = manifest_dir().join("templates");
        let config_json_path = manifest_dir().join("config/scanner_config.json");

        let video_c =
            CString::new(video.to_string_lossy().as_bytes()).map_err(|e| format!("video path: {}", e))?;
        let tpl_c = CString::new(template_pack.to_string_lossy().as_bytes())
            .map_err(|e| format!("template path: {}", e))?;
        let cfg_json = std::fs::read_to_string(&config_json_path).unwrap_or_default();
        let cfg_c = CString::new(cfg_json).map_err(|e| format!("config json: {}", e))?;

        let cfg = ScanConfig {
            template_pack_path: tpl_c.as_ptr(),
            config_json: cfg_c.as_ptr(),
            on_calib_failure: 0,
            frame_stride: 1,
            hud_redetect_interval: 300,
            max_frames,
        };

        let raw = unsafe { (self.create)(video_c.as_ptr(), &cfg) };
        if raw.is_null() {
            return Err(format!("scanner_create returned null: {}", self.last_error()));
        }
        Ok(ScannerHandle { lib: self, raw })
    }
}

/// Format a metadata struct for logging.
pub fn format_metadata(meta: &MatchMetadata) -> String {
    format!(
        "match_id={} p1={} p2={}",
        c_arr_to_string_pub(&meta.match_id),
        c_arr_to_string_pub(&meta.character_name_p1),
        c_arr_to_string_pub(&meta.character_name_p2),
    )
}