//! Path resolution utilities for Phase 8.
//!
//! Resolves `$HOME/.st6-trainer/` and provides paths for:
//! - `config.json` — application configuration
//! - `models/` — LLM GGUF model files
//! - `st6_trainer.db` — SQLite structured match database

use std::fs;
use std::path::{Path, PathBuf};

const APP_DIR_NAME: &str = ".st6-trainer";
const CONFIG_FILE_NAME: &str = "config.json";
const MODELS_DIR_NAME: &str = "models";
const DB_FILE_NAME: &str = "st6_trainer.db";

/// Resolve the application data directory: `$HOME/.st6-trainer/`.
///
/// Creates the directory (and `models/` subdirectory) if they do not exist.
/// Returns an error if the home directory cannot be resolved or directories
/// cannot be created.
pub fn resolve_app_data_dir() -> Result<PathBuf, String> {
    let home = dirs_next::home_dir().ok_or_else(|| {
        "Failed to resolve home directory ($HOME/.st6-trainer)".to_string()
    })?;
    let app_dir = home.join(APP_DIR_NAME);

    fs::create_dir_all(&app_dir).map_err(|e| {
        format!(
            "Failed to create app data directory {}: {}",
            app_dir.display(),
            e
        )
    })?;

    // Ensure models/ subdirectory exists
    let models_dir = app_dir.join(MODELS_DIR_NAME);
    fs::create_dir_all(&models_dir).map_err(|e| {
        format!(
            "Failed to create models directory {}: {}",
            models_dir.display(),
            e
        )
    })?;

    Ok(app_dir)
}

/// Resolve the config file path: `$HOME/.st6-trainer/config.json`.
pub fn resolve_config_path() -> Result<PathBuf, String> {
    let app_dir = resolve_app_data_dir()?;
    Ok(app_dir.join(CONFIG_FILE_NAME))
}

/// Resolve the models directory path: `$HOME/.st6-trainer/models/`.
pub fn resolve_models_dir() -> Result<PathBuf, String> {
    let app_dir = resolve_app_data_dir()?;
    Ok(app_dir.join(MODELS_DIR_NAME))
}

/// Resolve the SQLite database path: `$HOME/.st6-trainer/st6_trainer.db`.
pub fn resolve_db_path() -> Result<PathBuf, String> {
    let app_dir = resolve_app_data_dir()?;
    Ok(app_dir.join(DB_FILE_NAME))
}

/// Resolve a model file path within the models directory.
pub fn resolve_model_path(model_file_name: &str) -> Result<PathBuf, String> {
    let models_dir = resolve_models_dir()?;
    Ok(models_dir.join(model_file_name))
}

/// Check that a path exists and is readable.
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
    fn test_resolve_app_data_dir_creates_directory() {
        let dir = resolve_app_data_dir().expect("resolve_app_data_dir failed");
        assert!(dir.exists(), "app data dir should exist: {}", dir.display());
        assert!(dir.ends_with(APP_DIR_NAME));
        let models_dir = dir.join(MODELS_DIR_NAME);
        assert!(
            models_dir.exists(),
            "models dir should exist: {}",
            models_dir.display()
        );
    }

    #[test]
    fn test_resolve_config_path() {
        let path = resolve_config_path().expect("resolve_config_path failed");
        assert_eq!(path.file_name().unwrap(), CONFIG_FILE_NAME);
        assert!(path.parent().unwrap().ends_with(APP_DIR_NAME));
    }

    #[test]
    fn test_resolve_models_dir() {
        let dir = resolve_models_dir().expect("resolve_models_dir failed");
        assert_eq!(dir.file_name().unwrap(), MODELS_DIR_NAME);
        assert!(dir.parent().unwrap().ends_with(APP_DIR_NAME));
    }

    #[test]
    fn test_resolve_db_path() {
        let path = resolve_db_path().expect("resolve_db_path failed");
        assert_eq!(path.file_name().unwrap(), DB_FILE_NAME);
    }

    #[test]
    fn test_resolve_model_path() {
        let path = resolve_model_path("LFM3-1.7B-Q8_0.gguf").expect("resolve_model_path failed");
        assert_eq!(path.file_name().unwrap(), "LFM3-1.7B-Q8_0.gguf");
        assert!(path.parent().unwrap().ends_with(MODELS_DIR_NAME));
    }

    #[test]
    fn test_validate_path_missing() {
        let missing = std::path::Path::new("/nonexistent/path");
        let result = validate_path(missing, "test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_validate_path_existing() {
        let dir = resolve_app_data_dir().expect("resolve_app_data_dir failed");
        let result = validate_path(&dir, "app data dir");
        assert!(result.is_ok());
    }
}
