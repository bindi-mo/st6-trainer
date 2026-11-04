//! Video scanning module for ST6-Trainer
//!
//! Handles frame extraction, ROI detection, and signal extraction from SF6 match videos.

pub mod hud;
pub mod preprocessing;
pub mod roi;
pub mod scanner;
pub mod types;

pub use scanner::VideoScanner;
pub use types::*;
