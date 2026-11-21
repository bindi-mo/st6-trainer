//! Application configuration loading and merging.
//!
//! Two config files are involved:
//! - `config/app_config.json` — app-level settings (auto read/write, fluid schema)
//! - `src-tauri/config/scanner_config.json` — scanner algorithm parameters (developer-edited)
//!
//! The scanner config is passed to the C++ DLL as a JSON string via
//! `ScanConfig::config_json`. The app config is used by the Rust side only.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub last_input_video: String,
    pub default_output_dir: String,
    pub on_calib_failure: String,
    pub log_level: String,
    pub num_threads: u32,
    pub use_gpu: bool,
    pub scanner_config_path: String,
    pub template_pack_path: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            last_input_video: String::new(),
            default_output_dir: "./output".to_string(),
            on_calib_failure: "fail".to_string(),
            log_level: "info".to_string(),
            num_threads: 4,
            use_gpu: true,
            scanner_config_path: "src-tauri/config/scanner_config.json".to_string(),
            template_pack_path: "src-tauri/templates".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScannerConfig {
    pub coordinate_space: String,
    pub canonical_raster: [u32; 2],
    pub fps: f64,
    pub rows_visible: u32,
    pub fm_latch_frames: u32,
    pub gauge_occluded_threshold: u32,
    pub calibration: CalibrationConfig,
    pub template_thresholds: TemplateThresholds,
    pub input: InputConfig,
    pub attack_property_locale: Vec<String>,
    pub template_pack_version: String,
    pub config_version: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CalibrationConfig {
    pub upper_band_y: [f64; 2],
    pub lower_band_y: [f64; 2],
    pub search_x_tolerance_px: u32,
    pub confidence_threshold: f64,
    pub prominence_threshold: f64,
    pub max_dx_px: u32,
    pub consensus_survival_min: f64,
    pub consensus_vote_window_px: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TemplateThresholds {
    pub display: f64,
    pub calibration: f64,
    pub frame_meter: f64,
    pub input_glyph: f64,
    pub attack_property: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InputConfig {
    pub row_slots_p1: Vec<String>,
    pub row_slots_p2: Vec<String>,
    pub output_order: Vec<String>,
}

/// Resolve the app config path.
///
/// Looks in the resource dir (production bundle) then falls back to
/// `src-tauri/config/app_config.json` (development).
pub fn resolve_app_config_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    let resolver = app.path();
    if let Ok(resource_dir) = resolver.resource_dir() {
        let p = resource_dir.join("config/app_config.json");
        if p.exists() {
            return Some(p);
        }
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dev = manifest_dir.join("config/app_config.json");
    if dev.exists() {
        return Some(dev);
    }
    None
}

/// Resolve the scanner config path relative to the project root.
pub fn resolve_scanner_config_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    let resolver = app.path();
    if let Ok(resource_dir) = resolver.resource_dir() {
        let p = resource_dir.join("config/scanner_config.json");
        if p.exists() {
            return Some(p);
        }
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dev = manifest_dir.join("config/scanner_config.json");
    if dev.exists() {
        return Some(dev);
    }
    None
}

/// Load app config from the resolved path, falling back to defaults.
pub fn load_app_config(app: &tauri::AppHandle) -> AppConfig {
    match resolve_app_config_path(app) {
        Some(path) => match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
                eprintln!(
                    "[config] Failed to parse app_config.json at {}: {}",
                    path.display(),
                    e
                );
                AppConfig::default()
            }),
            Err(e) => {
                eprintln!(
                    "[config] Failed to read app_config.json at {}: {}",
                    path.display(),
                    e
                );
                AppConfig::default()
            }
        },
        None => {
            eprintln!("[config] app_config.json not found, using defaults");
            AppConfig::default()
        }
    }
}

/// Load scanner config from the resolved path, falling back to defaults.
pub fn load_scanner_config(app: &tauri::AppHandle) -> Option<ScannerConfig> {
    let path = resolve_scanner_config_path(app)?;
    let content = fs::read_to_string(&path).ok()?;
    match serde_json::from_str::<ScannerConfig>(&content) {
        Ok(cfg) => Some(cfg),
        Err(e) => {
            eprintln!(
                "[config] Failed to parse scanner_config.json at {}: {}",
                path.display(),
                e
            );
            None
        }
    }
}

/// Serialize scanner config to a JSON string for passing to the C++ DLL.
pub fn scanner_config_to_json(cfg: &ScannerConfig) -> Option<String> {
    serde_json::to_string(cfg).ok()
}

/// Resolve the template pack path relative to the project root.
pub fn resolve_template_pack_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    let resolver = app.path();
    if let Ok(resource_dir) = resolver.resource_dir() {
        let p = resource_dir.join("templates");
        if p.exists() {
            return Some(p);
        }
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dev = manifest_dir.join("templates");
    if dev.exists() {
        return Some(dev);
    }
    None
}

/// Resolve the scanner DLL path relative to the project root.
pub fn resolve_scanner_dll_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    let resolver = app.path();
    if let Ok(resource_dir) = resolver.resource_dir() {
        let p = resource_dir.join("st6_scanner.dll");
        if p.exists() {
            return Some(p);
        }
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dev = manifest_dir.join("st6_scanner.dll");
    if dev.exists() {
        return Some(dev);
    }
    None
}

/// Check that a path exists and is readable, returning a Result.
pub fn validate_path(path: &Path, label: &str) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("{} not found: {}", label, path.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_config_default() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.on_calib_failure, "fail");
        assert_eq!(cfg.num_threads, 4);
        assert!(cfg.use_gpu);
    }

    #[test]
    fn test_scanner_config_json_roundtrip() {
        let cfg = ScannerConfig {
            coordinate_space: "canonical".to_string(),
            canonical_raster: [1920, 1080],
            fps: 60.0,
            rows_visible: 19,
            fm_latch_frames: 3,
            gauge_occluded_threshold: 10,
            calibration: CalibrationConfig {
                upper_band_y: [0.00, 0.25],
                lower_band_y: [0.70, 1.00],
                search_x_tolerance_px: 32,
                confidence_threshold: 0.90,
                prominence_threshold: 0.10,
                max_dx_px: 2,
                consensus_survival_min: 0.80,
                consensus_vote_window_px: 2,
            },
            template_thresholds: TemplateThresholds {
                display: 0.85,
                calibration: 0.90,
                frame_meter: 0.90,
                input_glyph: 0.92,
                attack_property: 0.92,
            },
            input: InputConfig {
                row_slots_p1: vec!["counter".into(), "direction".into(), "buttons".into()],
                row_slots_p2: vec!["buttons".into(), "direction".into(), "counter".into()],
                output_order: vec!["directions".into(), "buttons".into()],
            },
            attack_property_locale: vec!["en".into(), "ja".into()],
            template_pack_version: "sf6-ui-2026.11".to_string(),
            config_version: "cfg-1.0".to_string(),
        };
        let json = scanner_config_to_json(&cfg).expect("serialize");
        let parsed: ScannerConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.canonical_raster, [1920, 1080]);
        assert_eq!(parsed.rows_visible, 19);
    }
}
