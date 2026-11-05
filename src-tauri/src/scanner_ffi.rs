//! Rust FFI bindings for st6_scanner C++ DLL

use libloading::{Library, Symbol};
use std::ffi::{CStr, CString};
use std::fs;
use std::os::raw::{c_char, c_double, c_int, c_uchar};
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
    #[error("Invalid frame data")]
    InvalidFrame,
    #[error("DLL symbol not found: {0}")]
    SymbolNotFound(String),
    #[error("Invalid path: {0}")]
    InvalidPath(#[from] std::ffi::NulError),
}

pub type ScannerResult<T> = std::result::Result<T, ScannerError>;

// Frame data matching C struct in scanner_api_2.h
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

/// DLLのパスを解決し、必要に応じて .exe と同じフォルダへ配置する
fn get_dll_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let resolver = app.path();
    let exe_dir = resolver.executable_dir().map_err(|e| e.to_string())?;
    let target_dll = exe_dir.join("st6_scanner.dll");

    if !target_dll.exists() {
        #[cfg(debug_assertions)]
        {
            // 開発時 (tauri dev): src-tauri/st6_scanner.dll をコピー
            let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let dev_dll = manifest_dir.join("st6_scanner.dll");
            if dev_dll.exists() {
                let _ = fs::copy(&dev_dll, &target_dll);
            }
        }

        #[cfg(not(debug_assertions))]
        {
            // 本番ビルド時 (tauri build): bundle.resources から取得
            let resource_dir = resolver.resource_dir().map_err(|e| e.to_string())?;
            let source_dll = resource_dir.join("st6_scanner.dll");
            if source_dll.exists() {
                fs::copy(&source_dll, &target_dll).map_err(|e| e.to_string())?;
            }
        }
    }

    if target_dll.exists() {
        Ok(target_dll)
    } else {
        Err("st6_scanner.dll could not be resolved or deployed.".into())
    }
}

impl Scanner {
    /// Create a new scanner from video file
    pub fn new<P: AsRef<Path>>(
        app: &tauri::AppHandle,
        video_path: P,
        config: Option<ScanConfig>,
    ) -> ScannerResult<Self> {
        let dll_path = get_dll_path(app).map_err(ScannerError::Creation)?;

        eprintln!(
            "[Rust] Scanner::new called with video_path: {:?}",
            video_path.as_ref()
        );
        eprintln!("[Rust] DLL path: {:?}", dll_path);
        eprintln!("[Rust] DLL exists: {}", dll_path.exists());

        // OpenCVが静的リンクされているため、追加の環境変数や検索パスの設定は不要です。
        eprintln!("[Rust] Calling Library::new...");
        let lib = unsafe { Library::new(&dll_path) }.map_err(ScannerError::DllLoad)?;
        eprintln!("[Rust] Library loaded successfully");

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

// Decode packed input byte into Direction and Buttons
fn decode_inputs(input_bytes: &[c_uchar], count: c_int) -> Vec<InputSignal> {
    let mut signals = Vec::new();
    let limit = (count as usize).min(input_bytes.len());

    // インデックスの代わりに .iter() と .take() を使用
    for &val in input_bytes.iter().take(limit) {
        let dir_val = val & 0x0F;
        let btn_val = (val & 0xF0) >> 4;

        let direction = match dir_val {
            1 => Direction::DownLeft,
            2 => Direction::Down,
            3 => Direction::DownRight,
            4 => Direction::Left,
            5 => Direction::Neutral,
            6 => Direction::Right,
            7 => Direction::UpLeft,
            8 => Direction::Up,
            9 => Direction::UpRight,
            _ => Direction::Neutral,
        };

        let mut buttons = Vec::new();
        if (btn_val & 0x01) != 0 {
            buttons.push(Button::LP);
        }
        if (btn_val & 0x02) != 0 {
            buttons.push(Button::MP);
        }
        if (btn_val & 0x04) != 0 {
            buttons.push(Button::HP);
        }
        if (btn_val & 0x08) != 0 {
            buttons.push(Button::LK);
        }
        if (btn_val & 0x10) != 0 {
            buttons.push(Button::MK);
        }
        if (btn_val & 0x20) != 0 {
            buttons.push(Button::HK);
        }

        signals.push(InputSignal {
            direction,
            buttons,
            confidence: 1.0,
        });
    }
    signals
}

// Convenience function for Tauri command
pub async fn scan_video_file(
    app: tauri::AppHandle,
    video_path: String,
) -> Result<VideoScanResult, String> {
    eprintln!("[Rust] scan_video_file called with: {}", video_path);
    let mut scanner = Scanner::new(&app, &video_path, None).map_err(|e| {
        eprintln!("[Rust] Scanner creation failed: {}", e);
        e.to_string()
    })?;
    eprintln!("[Rust] Scanner created successfully");

    let mut frames = Vec::new();
    let mut frame_count = 0;
    while let Ok(Some(frame)) = scanner.next_frame() {
        frames.push(convert_frame_data(frame));
        frame_count += 1;
        if frame_count % 100 == 0 {
            eprintln!("[Rust] Processed {} frames...", frame_count);
        }
    }
    eprintln!("[Rust] Scan completed, total frames: {}", frame_count);

    let result = VideoScanResult {
        video_path,
        resolution: Resolution::TARGET_1080P,
        fps: scanner.fps(),
        total_frames: scanner.total_frames() as usize,
        duration_ms: (scanner.total_frames() as f64 / scanner.fps()) * 1000.0,
        frames,
        hud_layout: HudLayout::default(),
    };
    Ok(result)
}

// Convert C FFI FrameData to Rust RawFrameLog
fn convert_frame_data(frame: FrameData) -> RawFrameLog {
    RawFrameLog {
        frame_index: frame.frame_index as usize,
        timestamp_ms: frame.timestamp_ms,
        p1_inputs: decode_inputs(&frame.p1_inputs, frame.p1_input_count),
        p2_inputs: decode_inputs(&frame.p2_inputs, frame.p2_input_count),
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

pub use crate::video_types::{
    Button, Direction, DriveGaugeState, FrameMeterData, HudLayout, InputSignal, NotificationType,
    PixelRect, RawFrameLog, Resolution, SystemNotification, VideoScanResult,
};
