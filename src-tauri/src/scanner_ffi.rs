//! Rust FFI bindings for st6_scanner C++ DLL (new API per spec §7)

use libloading::{Library, Symbol};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_double, c_int, c_longlong};
use std::path::{Path, PathBuf};
use tauri::Manager;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScannerError {
    #[error("Failed to load scanner DLL: {0}")]
    DllLoad(#[from] libloading::Error),
    #[error("Scanner creation failed: {0}")]
    Creation(String),
    #[error("Scanner operation failed: {0}")]
    Operation(String),
    #[error("DLL symbol not found: {0}")]
    SymbolNotFound(String),
    #[error("Invalid path: {0}")]
    InvalidPath(#[from] std::ffi::NulError),
    #[error("UTF-8 conversion failed: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),
}

pub type ScannerResult<T> = std::result::Result<T, ScannerError>;

// ===== C ABI matching structs (scanner_api.h) =====

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct ScannerVersion {
    pub major: c_int,
    pub minor: c_int,
    pub patch: c_int,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct ScanConfig {
    pub template_pack_path: *const c_char,
    pub config_json: *const c_char,
    pub on_calib_failure: c_int, // 0=fail, 1=ask
    pub frame_stride: c_int,
    pub hud_redetect_interval: c_int,
    pub max_frames: c_int,
}

#[allow(clippy::derivable_impls)]
impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            template_pack_path: std::ptr::null(),
            config_json: std::ptr::null(),
            on_calib_failure: 0,
            frame_stride: 1,
            hud_redetect_interval: 300,
            max_frames: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct MatchMetadata {
    pub match_id: [c_char; 64],
    pub character_name_p1: [c_char; 32],
    pub character_name_p2: [c_char; 32],
    pub control_type_p1: [c_char; 16],
    pub control_type_p2: [c_char; 16],
    pub detected_at_frame: c_int,
    pub source_resolution: [c_char; 32],
    pub source_fps: c_double,
    pub canonical_raster: [c_int; 2],
    pub top_y_offset_px: c_int,
    pub bottom_y_offset_px: c_int,
    pub offset_snapped: c_int,
    pub manual_verified: c_int, // -1=null, 0=false, 1=true
    pub input_history_available: c_int,
    pub attack_property_locale: [c_char; 32],
    pub extracted_at_utc: [c_char; 32],
    pub config_version: [c_char; 32],
    pub template_pack_version: [c_char; 32],
    pub extractor_version: [c_char; 32],
}

impl Default for MatchMetadata {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct FrameTelemetry {
    pub frame_index: c_int,
    pub source_frame_index: c_int,
    pub timestamp_ms: c_longlong,
    pub round_index: c_int,
    pub hp_reset: c_int,
    pub hp_p1: c_double,
    pub drive_p1: c_double,
    pub sa_p1: c_int,
    pub in_burnout_p1: c_int,
    pub hp_p2: c_double,
    pub drive_p2: c_double,
    pub sa_p2: c_int,
    pub in_burnout_p2: c_int,
    pub quality_flags: [[c_char; 32]; 16],
    pub quality_flags_count: c_int,
}

impl Default for FrameTelemetry {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct InputEvent {
    pub frame_start: c_int,
    pub frame_end: c_int,
    pub frame_span_video: c_int,
    pub hold_frames: c_int,
    pub counter_clipped: c_int,
    pub hold_frames_lower_bound: c_int,
    pub time_dilation_suspected: c_int,
    pub resync_used: c_int,
    pub unknown_glyph: c_int,
    pub is_preround: c_int,
    pub player: [c_char; 4],
    pub round_index: c_int,
    pub event_index: c_int,
    pub directions: [[c_char; 8]; 8],
    pub directions_count: c_int,
    pub buttons: [[c_char; 8]; 7],
    pub buttons_count: c_int,
    pub quality_flags: [[c_char; 32]; 16],
    pub quality_flags_count: c_int,
}

impl Default for InputEvent {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct MoveEvent {
    pub frame_start: c_int,
    pub frame_end: c_int,
    pub round_index: c_int,
    pub fm_startup_p1: c_int,
    pub fm_total_p1: c_int,
    pub fm_advantage_p1: c_int,
    pub fm_startup_p2: c_int,
    pub fm_total_p2: c_int,
    pub fm_advantage_p2: c_int,
    pub damage_p1: c_int,
    pub combo_damage_p1: c_int,
    pub damage_p2: c_int,
    pub combo_damage_p2: c_int,
    pub attack_property_p1: [c_char; 16],
    pub attack_property_p2: [c_char; 16],
    pub attack_property_locale: [c_char; 16],
    pub conf: c_double,
    pub quality_flags: [[c_char; 32]; 16],
    pub quality_flags_count: c_int,
}

impl Default for MoveEvent {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

// Opaque handle
type ScannerHandle = *mut std::ffi::c_void;

/// High-level scanner wrapper
pub struct Scanner {
    handle: ScannerHandle,
    _template_pack_cstr: Option<CString>,
    _config_json_cstr: Option<CString>,
    fn_get_metadata:
        Symbol<'static, unsafe extern "C" fn(ScannerHandle, *mut MatchMetadata) -> c_int>,
    fn_next_telemetry:
        Symbol<'static, unsafe extern "C" fn(ScannerHandle, *mut FrameTelemetry) -> c_int>,
    fn_next_input_event:
        Symbol<'static, unsafe extern "C" fn(ScannerHandle, *mut InputEvent) -> c_int>,
    fn_next_move_event:
        Symbol<'static, unsafe extern "C" fn(ScannerHandle, *mut MoveEvent) -> c_int>,
    fn_destroy: Symbol<'static, unsafe extern "C" fn(ScannerHandle)>,
    fn_get_last_error: Symbol<'static, unsafe extern "C" fn() -> *const c_char>,
    fn_set_manual_offsets: Symbol<'static, unsafe extern "C" fn(ScannerHandle, c_int, c_int, c_int) -> c_int>,
}

/// Resolve DLL path from src-tauri/ (copied by CMake POST_BUILD)
fn get_dll_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let resolver = app.path();
    // Try resource_dir first (bundled), then fallback to src-tauri/ (dev)
    let resource_dir = resolver.resource_dir().map_err(|e| e.to_string())?;
    let dll_path = resource_dir.join("st6_scanner.dll");
    if dll_path.exists() {
        return Ok(dll_path);
    }
    // Fallback: src-tauri/ directory (development)
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dev_dll = manifest_dir.join("st6_scanner.dll");
    if dev_dll.exists() {
        return Ok(dev_dll);
    }
    Err(format!(
        "st6_scanner.dll not found at {} or {}",
        dll_path.display(),
        dev_dll.display()
    ))
}

/// Resolve DLL path without Tauri (for CLI mode)
fn get_dll_path_plain() -> Result<PathBuf, String> {
    // Try current directory first
    let curr = std::env::current_dir().unwrap_or_default();
    let dll_path = curr.join("st6_scanner.dll");
    if dll_path.exists() {
        return Ok(dll_path);
    }
    // Fallback: src-tauri/ directory (development)
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dev_dll = manifest_dir.join("st6_scanner.dll");
    if dev_dll.exists() {
        return Ok(dev_dll);
    }
    Err(format!(
        "st6_scanner.dll not found at {} or {}",
        dll_path.display(),
        dev_dll.display()
    ))
}

/// Resolve template pack path (src-tauri/templates/)
fn get_template_pack_path() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("templates")
}

/// Resolve scanner config JSON path (src-tauri/config/scanner_config.json)
fn get_scanner_config_path() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("config/scanner_config.json")
}

/// Load scanner config JSON as a string, returning None if not found.
fn load_scanner_config_json() -> Option<String> {
    let path = get_scanner_config_path();
    match std::fs::read_to_string(&path) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!(
                "[Rust] Failed to read scanner config at {}: {}",
                path.display(),
                e
            );
            None
        }
    }
}

/// Common initialization logic shared by Tauri and CLI constructors.
fn init_scanner(
    dll_path: PathBuf,
    template_pack_path: PathBuf,
    video_path: &Path,
    config: Option<ScanConfig>,
) -> ScannerResult<Scanner> {
    eprintln!("[Rust] Scanner::new video_path: {:?}", video_path);
    eprintln!("[Rust] DLL path: {:?}", dll_path);
    eprintln!("[Rust] Template pack: {:?}", template_pack_path);

    // Load library and leak it so Symbols can have 'static lifetime.
    // The DLL remains loaded for the process lifetime.
    let lib = Box::leak(Box::new(
        unsafe { Library::new(&dll_path) }.map_err(ScannerError::DllLoad)?,
    ));
    eprintln!("[Rust] Library loaded successfully");

    // Load all symbols from library
    let create: Symbol<
        unsafe extern "C" fn(*const c_char, *const ScanConfig) -> ScannerHandle,
    > = unsafe { lib.get(b"scanner_create") }
        .map_err(|e| ScannerError::SymbolNotFound(format!("scanner_create: {}", e)))?;
    let get_metadata: Symbol<unsafe extern "C" fn(ScannerHandle, *mut MatchMetadata) -> c_int> =
        unsafe { lib.get(b"scanner_get_metadata") }
            .map_err(|e| ScannerError::SymbolNotFound(format!("scanner_get_metadata: {}", e)))?;
    let next_telemetry: Symbol<
        unsafe extern "C" fn(ScannerHandle, *mut FrameTelemetry) -> c_int,
    > = unsafe { lib.get(b"scanner_next_telemetry") }
        .map_err(|e| ScannerError::SymbolNotFound(format!("scanner_next_telemetry: {}", e)))?;
    let next_input_event: Symbol<
        unsafe extern "C" fn(ScannerHandle, *mut InputEvent) -> c_int,
    > = unsafe { lib.get(b"scanner_next_input_event") }.map_err(|e| {
        ScannerError::SymbolNotFound(format!("scanner_next_input_event: {}", e))
    })?;
    let next_move_event: Symbol<unsafe extern "C" fn(ScannerHandle, *mut MoveEvent) -> c_int> =
        unsafe { lib.get(b"scanner_next_move_event") }.map_err(|e| {
            ScannerError::SymbolNotFound(format!("scanner_next_move_event: {}", e))
        })?;
    let destroy: Symbol<unsafe extern "C" fn(ScannerHandle)> =
        unsafe { lib.get(b"scanner_destroy") }
            .map_err(|e| ScannerError::SymbolNotFound(format!("scanner_destroy: {}", e)))?;
    let get_last_error: Symbol<unsafe extern "C" fn() -> *const c_char> = unsafe {
        lib.get(b"scanner_get_last_error")
    }
    .map_err(|e| ScannerError::SymbolNotFound(format!("scanner_get_last_error: {}", e)))?;
    let set_manual_offsets: Symbol<
        unsafe extern "C" fn(ScannerHandle, c_int, c_int, c_int) -> c_int,
    > = unsafe { lib.get(b"scanner_set_manual_offsets") }
        .map_err(|e| ScannerError::SymbolNotFound(format!("scanner_set_manual_offsets: {}", e)))?;
    let api_version: Symbol<unsafe extern "C" fn() -> ScannerVersion> =
        unsafe { lib.get(b"scanner_api_version") }
            .map_err(|e| ScannerError::SymbolNotFound(format!("scanner_api_version: {}", e)))?;

    // Check API version
    let version = unsafe { api_version() };
    eprintln!(
        "[Rust] Scanner API version: {}.{}.{}",
        version.major, version.minor, version.patch
    );

    // Prepare config with template pack path and scanner config JSON.
    // CStrings are stored in the Scanner so their pointers remain valid
    // for the lifetime of the Scanner (the DLL may retain them).
    let mut cfg = config.unwrap_or_default();
    let template_pack_cstr = CString::new(template_pack_path.to_string_lossy().as_bytes())?;
    cfg.template_pack_path = template_pack_cstr.as_ptr();

    let config_json_cstr = if cfg.config_json.is_null() {
        load_scanner_config_json().and_then(|json| {
            CString::new(json).ok().inspect(|cstr| {
                cfg.config_json = cstr.as_ptr();
            })
        })
    } else {
        None
    };

    let path_cstr = CString::new(video_path.to_string_lossy().as_bytes())?;
    let handle = unsafe { create(path_cstr.as_ptr(), &cfg) };
    if handle.is_null() {
        let err_ptr = unsafe { get_last_error() };
        let err_msg = if err_ptr.is_null() {
            "Unknown error".to_string()
        } else {
            unsafe { CStr::from_ptr(err_ptr) }.to_str()?.to_string()
        };
        return Err(ScannerError::Creation(err_msg));
    }

    Ok(Scanner {
        handle,
        _template_pack_cstr: Some(template_pack_cstr),
        _config_json_cstr: config_json_cstr,
        fn_get_metadata: get_metadata,
        fn_next_telemetry: next_telemetry,
        fn_next_input_event: next_input_event,
        fn_next_move_event: next_move_event,
        fn_destroy: destroy,
        fn_get_last_error: get_last_error,
        fn_set_manual_offsets: set_manual_offsets,
    })
}

impl Scanner {
    /// Create a new scanner from video file (Tauri mode)
    pub fn new<P: AsRef<Path>>(
        app: &tauri::AppHandle,
        video_path: P,
        config: Option<ScanConfig>,
    ) -> ScannerResult<Self> {
        let dll_path = get_dll_path(app).map_err(ScannerError::Creation)?;
        let template_pack_path = get_template_pack_path();
        init_scanner(dll_path, template_pack_path, video_path.as_ref(), config)
    }

    /// Create a new scanner from video file (CLI mode, no Tauri AppHandle needed)
    pub fn new_from_path<P: AsRef<Path>>(
        video_path: P,
        config: Option<ScanConfig>,
    ) -> ScannerResult<Self> {
        let dll_path = get_dll_path_plain().map_err(ScannerError::Creation)?;
        let template_pack_path = get_template_pack_path();
        init_scanner(dll_path, template_pack_path, video_path.as_ref(), config)
    }

    /// Get match metadata (call once after creation)
    pub fn get_metadata(&self) -> ScannerResult<MatchMetadata> {
        let mut meta = MatchMetadata::default();
        let ret = unsafe { (self.fn_get_metadata)(self.handle, &mut meta) };
        if ret < 0 {
            let err_ptr = unsafe { (self.fn_get_last_error)() };
            let err_msg = if err_ptr.is_null() {
                "Unknown error".to_string()
            } else {
                unsafe { CStr::from_ptr(err_ptr) }.to_str()?.to_string()
            };
            return Err(ScannerError::Operation(err_msg));
        }
        Ok(meta)
    }

    /// Set manual calibration offsets (Phase 6)
    /// Returns true if successful
    pub fn set_manual_offsets(
        &self,
        top_y_offset_px: i32,
        bottom_y_offset_px: i32,
        manual_verified: i32,
    ) -> ScannerResult<bool> {
        let ret = unsafe {
            (self.fn_set_manual_offsets)(
                self.handle,
                top_y_offset_px,
                bottom_y_offset_px,
                manual_verified,
            )
        };
        if ret < 0 {
            let err_ptr = unsafe { (self.fn_get_last_error)() };
            let err_msg = if err_ptr.is_null() {
                "Unknown error".to_string()
            } else {
                unsafe { CStr::from_ptr(err_ptr) }.to_str()?.to_string()
            };
            return Err(ScannerError::Operation(err_msg));
        }
        Ok(ret == 1)
    }

    /// Get next frame telemetry (iterator)
    pub fn next_telemetry(&self) -> ScannerResult<Option<FrameTelemetry>> {
        let mut telemetry = FrameTelemetry::default();
        let ret = unsafe { (self.fn_next_telemetry)(self.handle, &mut telemetry) };
        match ret {
            1 => Ok(Some(telemetry)),
            0 => Ok(None),
            -1 => {
                let err_ptr = unsafe { (self.fn_get_last_error)() };
                let err_msg = if err_ptr.is_null() {
                    "Unknown error".to_string()
                } else {
                    unsafe { CStr::from_ptr(err_ptr) }.to_str()?.to_string()
                };
                Err(ScannerError::Operation(err_msg))
            }
            _ => Err(ScannerError::Operation("Unexpected return code".into())),
        }
    }

    /// Get next input event (iterator)
    pub fn next_input_event(&self) -> ScannerResult<Option<InputEvent>> {
        let mut event = InputEvent::default();
        let ret = unsafe { (self.fn_next_input_event)(self.handle, &mut event) };
        match ret {
            1 => Ok(Some(event)),
            0 => Ok(None),
            -1 => {
                let err_ptr = unsafe { (self.fn_get_last_error)() };
                let err_msg = if err_ptr.is_null() {
                    "Unknown error".to_string()
                } else {
                    unsafe { CStr::from_ptr(err_ptr) }.to_str()?.to_string()
                };
                Err(ScannerError::Operation(err_msg))
            }
            _ => Err(ScannerError::Operation("Unexpected return code".into())),
        }
    }

    /// Get next move event (iterator)
    pub fn next_move_event(&self) -> ScannerResult<Option<MoveEvent>> {
        let mut event = MoveEvent::default();
        let ret = unsafe { (self.fn_next_move_event)(self.handle, &mut event) };
        match ret {
            1 => Ok(Some(event)),
            0 => Ok(None),
            -1 => {
                let err_ptr = unsafe { (self.fn_get_last_error)() };
                let err_msg = if err_ptr.is_null() {
                    "Unknown error".to_string()
                } else {
                    unsafe { CStr::from_ptr(err_ptr) }.to_str()?.to_string()
                };
                Err(ScannerError::Operation(err_msg))
            }
            _ => Err(ScannerError::Operation("Unexpected return code".into())),
        }
    }
}

impl Drop for Scanner {
    fn drop(&mut self) {
        unsafe { (self.fn_destroy)(self.handle) };
    }
}

// Helper to convert fixed char arrays to String
fn c_arr_to_string(arr: &[c_char]) -> String {
    let len = arr.iter().position(|&c| c == 0).unwrap_or(arr.len());
    let bytes: Vec<u8> = arr[..len].iter().map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Public wrapper for external callers (smoke tests, examples).
pub fn c_arr_to_string_pub(arr: &[c_char]) -> String {
    c_arr_to_string(arr)
}

fn c_matrix_to_vec(arr: &[[c_char; 32]; 16], count: c_int) -> Vec<String> {
    (0..count as usize)
        .map(|i| c_arr_to_string(&arr[i]))
        .collect()
}

fn c_matrix_to_vec_8(arr: &[[c_char; 8]; 8], count: c_int) -> Vec<String> {
    (0..count as usize)
        .map(|i| c_arr_to_string(&arr[i]))
        .collect()
}

fn c_matrix_to_vec_7(arr: &[[c_char; 8]; 7], count: c_int) -> Vec<String> {
    (0..count as usize)
        .map(|i| c_arr_to_string(&arr[i]))
        .collect()
}

// Rust-friendly output types (for Tauri commands / pipeline)
#[derive(Debug, Clone, serde::Serialize)]
pub struct MatchMetadataOut {
    pub match_id: String,
    pub character_name_p1: String,
    pub character_name_p2: String,
    pub control_type_p1: String,
    pub control_type_p2: String,
    pub detected_at_frame: i32,
    pub source_resolution: String,
    pub source_fps: f64,
    pub canonical_raster: [i32; 2],
    pub top_y_offset_px: i32,
    pub bottom_y_offset_px: i32,
    pub offset_snapped: bool,
    pub manual_verified: Option<bool>,
    pub input_history_available: bool,
    pub attack_property_locale: String,
    pub extracted_at_utc: String,
    pub config_version: String,
    pub template_pack_version: String,
    pub extractor_version: String,
}

impl From<MatchMetadata> for MatchMetadataOut {
    fn from(m: MatchMetadata) -> Self {
        Self {
            match_id: c_arr_to_string(&m.match_id),
            character_name_p1: c_arr_to_string(&m.character_name_p1),
            character_name_p2: c_arr_to_string(&m.character_name_p2),
            control_type_p1: c_arr_to_string(&m.control_type_p1),
            control_type_p2: c_arr_to_string(&m.control_type_p2),
            detected_at_frame: m.detected_at_frame,
            source_resolution: c_arr_to_string(&m.source_resolution),
            source_fps: m.source_fps,
            canonical_raster: [m.canonical_raster[0], m.canonical_raster[1]],
            top_y_offset_px: m.top_y_offset_px,
            bottom_y_offset_px: m.bottom_y_offset_px,
            offset_snapped: m.offset_snapped != 0,
            manual_verified: match m.manual_verified {
                -1 => None,
                0 => Some(false),
                1 => Some(true),
                _ => None,
            },
            input_history_available: m.input_history_available != 0,
            attack_property_locale: c_arr_to_string(&m.attack_property_locale),
            extracted_at_utc: c_arr_to_string(&m.extracted_at_utc),
            config_version: c_arr_to_string(&m.config_version),
            template_pack_version: c_arr_to_string(&m.template_pack_version),
            extractor_version: c_arr_to_string(&m.extractor_version),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FrameTelemetryOut {
    pub frame_index: i32,
    pub source_frame_index: i32,
    pub timestamp_ms: i64,
    pub round_index: i32,
    pub hp_reset: bool,
    pub hp_p1: f64,
    pub drive_p1: f64,
    pub sa_p1: i32,
    pub in_burnout_p1: bool,
    pub hp_p2: f64,
    pub drive_p2: f64,
    pub sa_p2: i32,
    pub in_burnout_p2: bool,
    pub quality_flags: Vec<String>,
}

impl From<FrameTelemetry> for FrameTelemetryOut {
    fn from(t: FrameTelemetry) -> Self {
        Self {
            frame_index: t.frame_index,
            source_frame_index: t.source_frame_index,
            timestamp_ms: t.timestamp_ms,
            round_index: t.round_index,
            hp_reset: t.hp_reset != 0,
            hp_p1: t.hp_p1,
            drive_p1: t.drive_p1,
            sa_p1: t.sa_p1,
            in_burnout_p1: t.in_burnout_p1 != 0,
            hp_p2: t.hp_p2,
            drive_p2: t.drive_p2,
            sa_p2: t.sa_p2,
            in_burnout_p2: t.in_burnout_p2 != 0,
            quality_flags: c_matrix_to_vec(&t.quality_flags, t.quality_flags_count),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InputEventOut {
    pub frame_start: i32,
    pub frame_end: i32,
    pub frame_span_video: i32,
    pub hold_frames: i32,
    pub counter_clipped: bool,
    pub hold_frames_lower_bound: bool,
    pub time_dilation_suspected: bool,
    pub resync_used: bool,
    pub unknown_glyph: bool,
    pub is_preround: bool,
    pub player: String,
    pub round_index: i32,
    pub event_index: i32,
    pub directions: Vec<String>,
    pub buttons: Vec<String>,
    pub quality_flags: Vec<String>,
}

impl From<InputEvent> for InputEventOut {
    fn from(e: InputEvent) -> Self {
        Self {
            frame_start: e.frame_start,
            frame_end: e.frame_end,
            frame_span_video: e.frame_span_video,
            hold_frames: e.hold_frames,
            counter_clipped: e.counter_clipped != 0,
            hold_frames_lower_bound: e.hold_frames_lower_bound != 0,
            time_dilation_suspected: e.time_dilation_suspected != 0,
            resync_used: e.resync_used != 0,
            unknown_glyph: e.unknown_glyph != 0,
            is_preround: e.is_preround != 0,
            player: c_arr_to_string(&e.player),
            round_index: e.round_index,
            event_index: e.event_index,
            directions: c_matrix_to_vec_8(&e.directions, e.directions_count),
            buttons: c_matrix_to_vec_7(&e.buttons, e.buttons_count),
            quality_flags: c_matrix_to_vec(&e.quality_flags, e.quality_flags_count),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MoveEventOut {
    pub frame_start: i32,
    pub frame_end: i32,
    pub round_index: i32,
    pub fm_startup_p1: i32,
    pub fm_total_p1: i32,
    pub fm_advantage_p1: i32,
    pub fm_startup_p2: i32,
    pub fm_total_p2: i32,
    pub fm_advantage_p2: i32,
    pub damage_p1: i32,
    pub combo_damage_p1: i32,
    pub damage_p2: i32,
    pub combo_damage_p2: i32,
    pub attack_property_p1: String,
    pub attack_property_p2: String,
    pub attack_property_locale: String,
    pub conf: f64,
    pub quality_flags: Vec<String>,
}

impl From<MoveEvent> for MoveEventOut {
    fn from(m: MoveEvent) -> Self {
        Self {
            frame_start: m.frame_start,
            frame_end: m.frame_end,
            round_index: m.round_index,
            fm_startup_p1: m.fm_startup_p1,
            fm_total_p1: m.fm_total_p1,
            fm_advantage_p1: m.fm_advantage_p1,
            fm_startup_p2: m.fm_startup_p2,
            fm_total_p2: m.fm_total_p2,
            fm_advantage_p2: m.fm_advantage_p2,
            damage_p1: m.damage_p1,
            combo_damage_p1: m.combo_damage_p1,
            damage_p2: m.damage_p2,
            combo_damage_p2: m.combo_damage_p2,
            attack_property_p1: c_arr_to_string(&m.attack_property_p1),
            attack_property_p2: c_arr_to_string(&m.attack_property_p2),
            attack_property_locale: c_arr_to_string(&m.attack_property_locale),
            conf: m.conf,
            quality_flags: c_matrix_to_vec(&m.quality_flags, m.quality_flags_count),
        }
    }
}
