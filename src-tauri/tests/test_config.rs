//! Phase 15.1: Unit tests for configuration & model management (Phase 8).
//! Covers path resolution, config load/save/migration, model validation,
//! and model downloader result types.

use serial_test::serial;

#[test]
#[serial]
fn test_paths_resolve_app_data_dir() {
    let dir = st6_trainer_lib::paths::resolve_app_data_dir().expect("resolve_app_data_dir");
    assert!(dir.exists());
    assert!(dir.ends_with(".st6-trainer"));
    let models_dir = dir.join("models");
    assert!(models_dir.exists());
}

#[test]
#[serial]
fn test_paths_resolve_config_and_db() {
    let config_path = st6_trainer_lib::paths::resolve_config_path().expect("resolve_config_path");
    assert_eq!(config_path.file_name().unwrap(), "config.json");
    let db_path = st6_trainer_lib::paths::resolve_db_path().expect("resolve_db_path");
    assert_eq!(db_path.file_name().unwrap(), "st6_trainer.db");
    let model_path = st6_trainer_lib::paths::resolve_model_path("LFM3-1.7B-Q8_0.gguf")
        .expect("resolve_model_path");
    assert_eq!(model_path.file_name().unwrap(), "LFM3-1.7B-Q8_0.gguf");
}

#[test]
#[serial]
fn test_config_default_and_roundtrip() {
    let cfg = st6_trainer_lib::config::AppConfig::default();
    assert_eq!(cfg.num_threads, 4);
    assert!(cfg.use_gpu);
    assert!(!cfg.model_url.is_empty());
    assert_eq!(cfg.ctx_size, 4096);
    assert_eq!(cfg.gpu_layers, 35);
    assert!(!cfg.pipe_name.is_empty());
    assert!(!cfg.config_version.is_empty());

    // JSON roundtrip
    let json = serde_json::to_string(&cfg).expect("serialize");
    let parsed: st6_trainer_lib::config::AppConfig =
        serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed.model_url, cfg.model_url);
    assert_eq!(parsed.ctx_size, cfg.ctx_size);
    assert_eq!(parsed.gpu_layers, cfg.gpu_layers);
    assert_eq!(parsed.pipe_name, cfg.pipe_name);
    assert_eq!(parsed.config_version, cfg.config_version);
}

#[test]
#[serial]
fn test_config_save_and_load_from_home() {
    let cfg = st6_trainer_lib::config::AppConfig::default();
    let result = st6_trainer_lib::config::save_app_config_to_home(&cfg);
    assert!(
        result.is_ok(),
        "save_app_config_to_home: {:?}",
        result.err()
    );

    let loaded = st6_trainer_lib::config::load_app_config_from_home();
    assert_eq!(loaded.model_url, cfg.model_url);
    assert_eq!(loaded.ctx_size, cfg.ctx_size);
    assert_eq!(loaded.gpu_layers, cfg.gpu_layers);
    assert_eq!(loaded.pipe_name, cfg.pipe_name);
}

#[test]
#[serial]
fn test_config_migrate_outdated_version_via_save_load() {
    // migrate_app_config is private; test migration indirectly via save/load cycle.
    // Save a config with an outdated version, then load it back and verify version is updated.
    let cfg = st6_trainer_lib::config::AppConfig {
        config_version: "config-0.9".to_string(),
        ..Default::default()
    };
    let save_result = st6_trainer_lib::config::save_app_config_to_home(&cfg);
    assert!(
        save_result.is_ok(),
        "save_app_config_to_home: {:?}",
        save_result.err()
    );

    let loaded = st6_trainer_lib::config::load_app_config_from_home();
    // load_app_config_from_home runs migration, so version should be updated to current
    assert_eq!(
        loaded.config_version,
        st6_trainer_lib::config::AppConfig::default().config_version
    );
}

#[test]
#[serial]
fn test_model_manager_validation_nonexistent() {
    let result = st6_trainer_lib::model_manager::validate_model("nonexistent.gguf", None)
        .expect("validate_model should not error");
    assert!(!result.valid);
    assert!(result.error.is_some());
    assert!(result.error.unwrap().contains("not found"));
}

#[test]
#[serial]
fn test_model_manager_list_models() {
    let result = st6_trainer_lib::model_manager::list_models();
    assert!(
        result.is_ok(),
        "list_models should not fail: {:?}",
        result.err()
    );
}

#[test]
#[serial]
fn test_model_manager_model_exists_nonexistent() {
    let exists = st6_trainer_lib::model_manager::model_exists("nonexistent_model.gguf")
        .expect("model_exists failed");
    assert!(!exists);
}

#[test]
#[serial]
fn test_model_downloader_download_result_serialization() {
    let result = st6_trainer_lib::model_downloader::DownloadResult {
        model_file_name: "test.gguf".to_string(),
        total_bytes: 1024,
        sha256: Some("abc123".to_string()),
    };
    let json = serde_json::to_string(&result).expect("serialize");
    assert!(json.contains("test.gguf"));
    assert!(json.contains("1024"));
    assert!(json.contains("abc123"));
}

#[test]
#[serial]
fn test_config_paths_validate_path() {
    let dir = st6_trainer_lib::paths::resolve_app_data_dir().expect("resolve_app_data_dir");
    assert!(st6_trainer_lib::paths::validate_path(&dir, "app data dir").is_ok());
    let missing = std::path::Path::new("/nonexistent/path");
    assert!(st6_trainer_lib::paths::validate_path(missing, "test").is_err());
}
