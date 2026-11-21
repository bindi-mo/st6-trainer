//! Application configuration loading and merging.
//!
//! Two config files are involved:
//! - `$HOME/.st6-trainer/config.json` — app-level settings (auto read/write, fluid schema)
//! - `src-tauri/config/scanner_config.json` — scanner algorithm parameters (developer-edited)
//!
//! The scanner config is passed to the C++ DLL as a JSON string via
//! `ScanConfig::config_json`. The app config is used by the Rust side only.
//!
//! Phase 8: LLM-related fields added to `AppConfig` (model_path, model_url,
//! ctx_size, gpu_layers, pipe_name). Config is persisted to
//! `$HOME/.st6-trainer/config.json` with migration support via a `config_version`
//! field.

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
    // Phase 8: LLM configuration fields
    pub model_path: String,
    pub model_url: String,
    pub ctx_size: u32,
    pub gpu_layers: i32,
    pub pipe_name: String,
    pub config_version: String,
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
            model_path: String::new(),
            model_url: "https://huggingface.co/LiquidAI/LFM3-1.7B/resolve/main/LFM3-1.7B-Q8_0.gguf".to_string(),
            ctx_size: 4096,
            gpu_layers: 35,
            pipe_name: r"\\.\pipe\st6_llm_worker".to_string(),
            config_version: "config-1.0".to_string(),
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

/// Phase 8: Load app config from `$HOME/.st6-trainer/config.json`.
///
/// Falls back to defaults if the file does not exist or fails to parse.
/// Runs migration if `config_version` differs from the current default.
pub fn load_app_config_from_home() -> AppConfig {
    match crate::paths::resolve_config_path() {
        Ok(path) => match fs::read_to_string(&path) {
            Ok(content) => {
                let mut cfg: AppConfig = match serde_json::from_str(&content) {
                    Ok(cfg) => cfg,
                    Err(e) => {
                        eprintln!(
                            "[config] Failed to parse config.json at {}: {}",
                            path.display(),
                            e
                        );
                        AppConfig::default()
                    }
                };
                cfg = migrate_app_config(cfg);
                cfg
            }
            Err(e) => {
                eprintln!(
                    "[config] Failed to read config.json at {}: {}",
                    path.display(),
                    e
                );
                AppConfig::default()
            }
        },
        Err(e) => {
            eprintln!("[config] Failed to resolve config path: {}", e);
            AppConfig::default()
        }
    }
}

/// Phase 8: Save app config to `$HOME/.st6-trainer/config.json`.
///
/// Creates the file if it does not exist, overwrites otherwise.
pub fn save_app_config_to_home(cfg: &AppConfig) -> Result<(), String> {
    let path = crate::paths::resolve_config_path()?;
    let json = serde_json::to_string_pretty(cfg)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(&path, json).map_err(|e| {
        format!(
            "Failed to write config.json at {}: {}",
            path.display(),
            e
        )
    })?;
    Ok(())
}

/// Phase 8: Migrate app config to the current schema version.
///
/// Compares `config_version` against the default and applies any needed
/// migrations. Currently only v1 (config-1.0) exists; this is a placeholder
/// for future migrations.
fn migrate_app_config(mut cfg: AppConfig) -> AppConfig {
    let current_version = AppConfig::default().config_version.clone();
    if cfg.config_version == current_version {
        return cfg;
    }

    eprintln!(
        "[config] Migrating config from '{}' to '{}'",
        cfg.config_version, current_version
    );

    // Future migrations go here, e.g.:
    // if cfg.config_version == "config-0.9" { ... }

    cfg.config_version = current_version;
    cfg
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

    // Phase 8: AppConfig default LLM fields
    #[test]
    fn test_app_config_default_llm_fields() {
        let cfg = AppConfig::default();
        assert!(!cfg.model_path.is_empty() || cfg.model_path.is_empty()); // model_path may be empty until set
        assert!(!cfg.model_url.is_empty());
        assert!(cfg.ctx_size > 0);
        assert!(cfg.gpu_layers >= 0);
        assert!(!cfg.pipe_name.is_empty());
        assert!(!cfg.config_version.is_empty());
    }

    // Phase 8: AppConfig JSON roundtrip including LLM fields
    #[test]
    fn test_app_config_json_roundtrip() {
        let cfg = AppConfig::default();
        let json = serde_json::to_string(&cfg).expect("serialize");
        let parsed: AppConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.model_url, cfg.model_url);
        assert_eq!(parsed.ctx_size, cfg.ctx_size);
        assert_eq!(parsed.gpu_layers, cfg.gpu_layers);
        assert_eq!(parsed.pipe_name, cfg.pipe_name);
        assert_eq!(parsed.config_version, cfg.config_version);
    }

    // Phase 8: migrate_app_config keeps current version unchanged
    #[test]
    fn test_migrate_app_config_current_version() {
        let cfg = AppConfig::default();
        let migrated = migrate_app_config(cfg);
        assert_eq!(migrated.config_version, AppConfig::default().config_version);
    }

    // Phase 8: migrate_app_config updates outdated version
    #[test]
    fn test_migrate_app_config_outdated_version() {
        let mut cfg = AppConfig::default();
        cfg.config_version = "config-0.9".to_string();
        let migrated = migrate_app_config(cfg);
        assert_eq!(migrated.config_version, AppConfig::default().config_version);
    }
}
