//! Core types for video scanning (FFI version - no OpenCV dependency)

use serde::{Deserialize, Serialize};
use std::fmt;

/// Video resolution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl Resolution {
    pub const TARGET_1080P: Resolution = Resolution { width: 1920, height: 1080 };
}

/// HUD layout with dynamic anchor positions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HudLayout {
    pub timer_center: (f32, f32),  // Normalized coordinates
    pub p1_health_start: (f32, f32),
    pub p2_health_start: (f32, f32),
    pub p1_drive_gauge: PixelRect,
    pub p2_drive_gauge: PixelRect,
    pub system_notification: PixelRect,
    pub frame_meter: PixelRect,
}

/// Pixel coordinates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PixelRect {
    pub x_min: i32,
    pub y_min: i32,
    pub x_max: i32,
    pub y_max: i32,
}

impl PixelRect {
    pub fn width(&self) -> i32 {
        self.x_max - self.x_min
    }

    pub fn height(&self) -> i32 {
        self.y_max - self.y_min
    }
}

/// Complete video scan result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoScanResult {
    pub video_path: String,
    pub resolution: Resolution,
    pub fps: f64,
    pub total_frames: usize,
    pub duration_ms: f64,
    pub frames: Vec<RawFrameLog>,
    pub hud_layout: HudLayout,
}

/// Raw frame log extracted from a single frame
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawFrameLog {
    pub frame_index: usize,
    pub timestamp_ms: f64,
    pub p1_inputs: Vec<InputSignal>,
    pub p2_inputs: Vec<InputSignal>,
    pub p1_drive_gauge: DriveGaugeState,
    pub p2_drive_gauge: DriveGaugeState,
    pub system_notification: Option<SystemNotification>,
    pub frame_meter: Option<FrameMeterData>,
}

/// Input signal from key history area
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSignal {
    pub direction: Direction,
    pub buttons: Vec<Button>,
    pub confidence: f32,
}

/// Direction input (numpad notation)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Neutral = 5,
    Up = 8,
    Down = 2,
    Left = 4,
    Right = 6,
    UpLeft = 7,
    UpRight = 9,
    DownLeft = 1,
    DownRight = 3,
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", *self as u8)
    }
}

/// Attack buttons
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Button {
    LP,
    MP,
    HP,
    LK,
    MK,
    HK,
    Drive,
    Super,
}

/// Drive gauge state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveGaugeState {
    pub blocks: u8,           // 0-6
    pub is_burnout: bool,
    pub pixel_intensity: f32, // Average pixel intensity for validation
}

/// System notification (COUNTER, PUNISH COUNTER, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemNotification {
    pub text: String,
    pub notification_type: NotificationType,
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationType {
    Counter,
    PunishCounter,
    Reversal,
    FirstAttack,
    WhiffPunish,
    CrushCounter,
}

/// Frame meter data (startup, active, recovery, advantage)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameMeterData {
    pub startup: Option<i32>,
    pub active: Option<i32>,
    pub recovery: Option<i32>,
    pub hit_advantage: Option<i32>,
    pub block_advantage: Option<i32>,
}