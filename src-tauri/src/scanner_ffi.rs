//! Rust FFI bindings for st6_scanner C++ DLL

use libloading::{Library, Symbol};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_double, c_int, c_uchar};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScannerError {
    #[error("Failed to load scanner DLL: {0}")]
    DllLoad(#[from] libloading::Error),
    #[error("Scanner creation failed: {0}")]
    Creation(String),
    #[error("Scanner operation failed: {0}")]
    Operation(String),
    #[error("Invalid frame data")]
    InvalidFrame,
    #[error("DLL symbol not found: {0}")]
    SymbolNotFound(String),
    #[error("Invalid path: {0}")]
    InvalidPath(#[from] std::ffi::NulError),
}

pub type ScannerResult<T> = std::result::Result<T, ScannerError>;

// Frame data matching C struct
#[repr(C)]
#[derive(Debug, Clone)]
pub struct FrameData {
    pub frame_index: c_int,
    pub timestamp_ms: c_double,

    pub p1_inputs: [c_uchar; 64],
    pub p1_input_count: c_int,

    pub p2_inputs: [c_uchar; 64],
    pub p2_input_count: c_int,

    pub p1_drive_blocks: c_uchar,
    pub p1_drive_burnout: c_uchar,
    pub p2_drive_blocks: c_uchar,
    pub p2_drive_burnout: c_uchar,

    pub has_notification: c_uchar,
    pub notification_type: c_uchar,
    pub notification_confidence: f32,

    pub has_frame_meter: c_uchar,
    pub startup: c_int,
    pub active: c_int,
    pub recovery: c_int,
    pub hit_advantage: c_int,
    pub block_advantage: c_int,
}

impl Default for FrameData {
    fn default() -> Self {
        Self {
            frame_index: 0,
            timestamp_ms: 0.0,
            p1_inputs: [0; 64],
            p1_input_count: 0,
            p2_inputs: [0; 64],
            p2_input_count: 0,
            p1_drive_blocks: 0,
            p1_drive_burnout: 0,
            p2_drive_blocks: 0,
            p2_drive_burnout: 0,
            has_notification: 0,
            notification_type: 0,
            notification_confidence: 0.0,
            has_frame_meter: 0,
            startup: 0,
            active: 0,
            recovery: 0,
            hit_advantage: 0,
            block_advantage: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct ScanConfig {
    pub frame_stride: c_int,
    pub hud_redetect_interval: c_int,
    pub max_frames: c_int,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            frame_stride: 1,
            hud_redetect_interval: 300,
            max_frames: 0,
        }
    }
}

// Opaque handle
type ScannerHandle = *mut std::ffi::c_void;

/// High-level scanner wrapper
pub struct Scanner {
    lib: Library,
    handle: ScannerHandle,
}

fn get_dll_path() -> PathBuf {
    // CARGO_MANIFEST_DIR = src-tauri/
    // DLL is at project root (st6-trainer/st6_scanner.dll) - ONE level up
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project_root = manifest_dir.parent().unwrap(); // src-tauri/.. = st6-trainer/
    let path = project_root.join("st6_scanner.dll");

    // For tests, also check the test output directory
    if cfg!(test) {
        let test_output = manifest_dir
            .join("target")
            .join("debug")
            .join("deps")
            .join("st6_scanner.dll");
        if test_output.exists() {
            eprintln!("DEBUG: Using test output DLL: {:?}", test_output);
            eprintln!("DEBUG: test_output parent = {:?}", test_output.parent());
            eprintln!("DEBUG: current_dir = {:?}", std::env::current_dir());
            return test_output;
        }
    }

    eprintln!("DEBUG: manifest_dir = {:?}", manifest_dir);
    eprintln!("DEBUG: project_root = {:?}", project_root);
    eprintln!("DEBUG: DLL path = {:?}", path);
    eprintln!("DEBUG: DLL exists = {}", path.exists());
    eprintln!("DEBUG: current_dir = {:?}", std::env::current_dir());
    path
}

impl Scanner {
    /// Create a new scanner from video file
    pub fn new<P: AsRef<Path>>(video_path: P, config: Option<ScanConfig>) -> ScannerResult<Self> {
        let dll_path = get_dll_path();
        let lib = unsafe { Library::new(&dll_path) }?;

        let create: Symbol<
            unsafe extern "C" fn(*const c_char, *const ScanConfig) -> ScannerHandle,
        > = unsafe { lib.get(b"scanner_create") }
            .map_err(|e| ScannerError::SymbolNotFound(format!("scanner_create: {}", e)))?;

        let path_cstr = CString::new(video_path.as_ref().to_string_lossy().as_bytes())?;
        let config = config.unwrap_or_default();

        let handle = unsafe { create(path_cstr.as_ptr(), &config) };
        if handle.is_null() {
            let get_error: Symbol<unsafe extern "C" fn() -> *const c_char> = unsafe {
                lib.get(b"scanner_get_last_error")
            }
            .map_err(|e| ScannerError::SymbolNotFound(format!("scanner_get_last_error: {}", e)))?;
            let err_ptr = unsafe { get_error() };
            let err_msg = if err_ptr.is_null() {
                "Unknown error".to_string()
            } else {
                unsafe { CStr::from_ptr(err_ptr) }
                    .to_string_lossy()
                    .into_owned()
            };
            return Err(ScannerError::Creation(err_msg));
        }

        Ok(Self { lib, handle })
    }

    /// Get next frame data
    pub fn next_frame(&mut self) -> ScannerResult<Option<FrameData>> {
        let next_frame: Symbol<unsafe extern "C" fn(ScannerHandle, *mut FrameData) -> c_int> =
            unsafe { self.lib.get(b"scanner_next_frame") }
                .map_err(|e| ScannerError::SymbolNotFound(format!("scanner_next_frame: {}", e)))?;

        let mut frame = FrameData::default();
        let ret = unsafe { next_frame(self.handle, &mut frame) };

        match ret {
            1 => Ok(Some(frame)),
            0 => Ok(None), // End of video
            -1 => {
                let get_error: Symbol<unsafe extern "C" fn() -> *const c_char> =
                    unsafe { self.lib.get(b"scanner_get_last_error") }.map_err(|e| {
                        ScannerError::SymbolNotFound(format!("scanner_get_last_error: {}", e))
                    })?;
                let err_ptr = unsafe { get_error() };
                let err_msg = if err_ptr.is_null() {
                    "Unknown error".to_string()
                } else {
                    unsafe { CStr::from_ptr(err_ptr) }
                        .to_string_lossy()
                        .into_owned()
                };
                Err(ScannerError::Operation(err_msg))
            }
            _ => Err(ScannerError::InvalidFrame),
        }
    }

    /// Get video metadata
    pub fn total_frames(&self) -> c_int {
        let get_total: Symbol<unsafe extern "C" fn(ScannerHandle) -> c_int> =
            unsafe { self.lib.get(b"scanner_get_total_frames") }.unwrap();
        unsafe { get_total(self.handle) }
    }

    pub fn fps(&self) -> f64 {
        let get_fps: Symbol<unsafe extern "C" fn(ScannerHandle) -> c_double> =
            unsafe { self.lib.get(b"scanner_get_fps") }.unwrap();
        unsafe { get_fps(self.handle) }
    }

    pub fn width(&self) -> c_int {
        let get_w: Symbol<unsafe extern "C" fn(ScannerHandle) -> c_int> =
            unsafe { self.lib.get(b"scanner_get_width") }.unwrap();
        unsafe { get_w(self.handle) }
    }

    pub fn height(&self) -> c_int {
        let get_h: Symbol<unsafe extern "C" fn(ScannerHandle) -> c_int> =
            unsafe { self.lib.get(b"scanner_get_height") }.unwrap();
        unsafe { get_h(self.handle) }
    }

    /// Iterate over all frames
    pub fn frames(&mut self) -> FrameIterator<'_> {
        FrameIterator { scanner: self }
    }
}

pub struct FrameIterator<'a> {
    scanner: &'a mut Scanner,
}

impl<'a> Iterator for FrameIterator<'a> {
    type Item = ScannerResult<FrameData>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.scanner.next_frame() {
            Ok(Some(frame)) => Some(Ok(frame)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

impl Drop for Scanner {
    fn drop(&mut self) {
        let destroy: Symbol<unsafe extern "C" fn(ScannerHandle)> =
            unsafe { self.lib.get(b"scanner_destroy") }.unwrap();
        unsafe { destroy(self.handle) };
    }
}

// Convenience function for Tauri command
pub async fn scan_video_file(video_path: String) -> Result<VideoScanResult, String> {
    let mut scanner = Scanner::new(&video_path, None).map_err(|e| e.to_string())?;

    let mut frames = Vec::new();
    while let Ok(Some(frame)) = scanner.next_frame() {
        // Convert FrameData (C FFI) to RawFrameLog (Rust types)
        frames.push(convert_frame_data(frame));
    }

    Ok(VideoScanResult {
        video_path,
        resolution: Resolution::TARGET_1080P,
        fps: scanner.fps(),
        total_frames: scanner.total_frames() as usize,
        duration_ms: (scanner.total_frames() as f64 / scanner.fps()) * 1000.0,
        frames,
        hud_layout: HudLayout::default(), // TODO: extract from scanner
    })
}

// Convert C FFI FrameData to Rust RawFrameLog
fn convert_frame_data(frame: FrameData) -> RawFrameLog {
    RawFrameLog {
        frame_index: frame.frame_index as usize,
        timestamp_ms: frame.timestamp_ms,
        p1_inputs: Vec::new(), // TODO: decode p1_inputs
        p2_inputs: Vec::new(), // TODO: decode p2_inputs
        p1_drive_gauge: DriveGaugeState {
            blocks: frame.p1_drive_blocks,
            is_burnout: frame.p1_drive_burnout != 0,
            pixel_intensity: 0.0,
        },
        p2_drive_gauge: DriveGaugeState {
            blocks: frame.p2_drive_blocks,
            is_burnout: frame.p2_drive_burnout != 0,
            pixel_intensity: 0.0,
        },
        system_notification: if frame.has_notification != 0 {
            Some(SystemNotification {
                text: format!("{:?}", frame.notification_type),
                notification_type: match frame.notification_type {
                    0 => NotificationType::Counter,
                    1 => NotificationType::PunishCounter,
                    2 => NotificationType::Reversal,
                    3 => NotificationType::FirstAttack,
                    4 => NotificationType::WhiffPunish,
                    5 => NotificationType::CrushCounter,
                    _ => NotificationType::Counter,
                },
                confidence: frame.notification_confidence,
            })
        } else {
            None
        },
        frame_meter: if frame.has_frame_meter != 0 {
            Some(FrameMeterData {
                startup: Some(frame.startup),
                active: Some(frame.active),
                recovery: Some(frame.recovery),
                hit_advantage: Some(frame.hit_advantage),
                block_advantage: Some(frame.block_advantage),
            })
        } else {
            None
        },
    }
}

// Types needed by Tauri (defined in video_types.rs)
pub use crate::video_types::{
    Button, Direction, DriveGaugeState, FrameMeterData, HudLayout, InputSignal, NotificationType,
    PixelRect, RawFrameLog, Resolution, SystemNotification, VideoScanResult,
};
