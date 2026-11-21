//! Phase 8: LLM model file management.
//!
//! Manages `$HOME/.st6-trainer/models/`:
//! - GGUF header validation (magic bytes + version)
//! - SHA256 checksum computation and verification
//! - Model existence check

use crate::paths;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::PathBuf;

/// GGUF magic bytes ("GGUF" in little-endian: 0x46554747)
const GGUF_MAGIC: u32 = 0x4655_4747;
/// Supported GGUF versions
const GGUF_VERSION_MIN: u32 = 1;
const GGUF_VERSION_MAX: u32 = 3;
/// Minimum GGUF file size to read header (magic + version + tensor_count + metadata_kv_count)
const GGUF_HEADER_MIN_SIZE: usize = 32;

/// Result of model validation
#[derive(Debug, Clone, Serialize)]
pub struct ModelValidationResult {
    pub valid: bool,
    pub gguf_version: Option<u32>,
    pub error: Option<String>,
}

/// List all model files in the models directory.
pub fn list_models() -> Result<Vec<PathBuf>, String> {
    let models_dir = paths::resolve_models_dir()?;
    let mut models = Vec::new();
    for entry in fs::read_dir(&models_dir).map_err(|e| {
        format!(
            "Failed to read models directory {}: {}",
            models_dir.display(),
            e
        )
    })? {
        let entry = entry.map_err(|e| format!("Failed to read dir entry: {}", e))?;
        let path = entry.path();
        if path.is_file() {
            models.push(path);
        }
    }
    models.sort();
    Ok(models)
}

/// Check whether a model file exists at the given path.
pub fn model_exists(model_file_name: &str) -> Result<bool, String> {
    let path = paths::resolve_model_path(model_file_name)?;
    Ok(path.exists())
}

/// Validate a GGUF model file: checks magic bytes, version, and minimum size.
pub fn validate_gguf_header(model_file_name: &str) -> Result<ModelValidationResult, String> {
    let path = paths::resolve_model_path(model_file_name)?;

    if !path.exists() {
        return Ok(ModelValidationResult {
            valid: false,
            gguf_version: None,
            error: Some(format!("Model file not found: {}", path.display())),
        });
    }

    let metadata = fs::metadata(&path).map_err(|e| {
        format!(
            "Failed to stat model file {}: {}",
            path.display(),
            e
        )
    })?;

    if metadata.len() < GGUF_HEADER_MIN_SIZE as u64 {
        return Ok(ModelValidationResult {
            valid: false,
            gguf_version: None,
            error: Some(format!(
                "Model file too small ({} bytes, need at least {})",
                metadata.len(),
                GGUF_HEADER_MIN_SIZE
            )),
        });
    }

    let mut file = fs::File::open(&path).map_err(|e| {
        format!(
            "Failed to open model file {}: {}",
            path.display(),
            e
        )
    })?;

    let mut header = [0u8; GGUF_HEADER_MIN_SIZE];
    file.read_exact(&mut header)
        .map_err(|e| format!("Failed to read GGUF header: {}", e))?;

    // Magic bytes (first 4 bytes, little-endian u32)
    let magic = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    if magic != GGUF_MAGIC {
        return Ok(ModelValidationResult {
            valid: false,
            gguf_version: None,
            error: Some(format!(
                "Invalid GGUF magic: 0x{:08X}, expected 0x{:08X}",
                magic, GGUF_MAGIC
            )),
        });
    }

    // Version (bytes 4-7, little-endian u32)
    let version = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    if !(GGUF_VERSION_MIN..=GGUF_VERSION_MAX).contains(&version) {
        return Ok(ModelValidationResult {
            valid: false,
            gguf_version: Some(version),
            error: Some(format!(
                "Unsupported GGUF version: {} (supported {}-{})",
                version, GGUF_VERSION_MIN, GGUF_VERSION_MAX
            )),
        });
    }

    Ok(ModelValidationResult {
        valid: true,
        gguf_version: Some(version),
        error: None,
    })
}

/// Compute the SHA256 checksum of a model file.
pub fn compute_sha256(model_file_name: &str) -> Result<String, String> {
    let path = paths::resolve_model_path(model_file_name)?;

    let mut file = fs::File::open(&path).map_err(|e| {
        format!(
            "Failed to open model file {}: {}",
            path.display(),
            e
        )
    })?;

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = file.read(&mut buffer).map_err(|e| {
            format!(
                "Failed to read model file {}: {}",
                path.display(),
                e
            )
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

/// Verify a model file's SHA256 checksum against an expected value.
pub fn verify_sha256(model_file_name: &str, expected_sha256: &str) -> Result<bool, String> {
    let actual = compute_sha256(model_file_name)?;
    Ok(actual.eq_ignore_ascii_case(expected_sha256))
}

/// Validate a model file completely (existence + GGUF header + optional SHA256).
pub fn validate_model(
    model_file_name: &str,
    expected_sha256: Option<&str>,
) -> Result<ModelValidationResult, String> {
    let path = paths::resolve_model_path(model_file_name)?;

    if !path.exists() {
        return Ok(ModelValidationResult {
            valid: false,
            gguf_version: None,
            error: Some(format!("Model file not found: {}", path.display())),
        });
    }

    let header_result = validate_gguf_header(model_file_name)?;
    if !header_result.valid {
        return Ok(header_result);
    }

    if let Some(expected) = expected_sha256 {
        let sha_ok = verify_sha256(model_file_name, expected)?;
        if !sha_ok {
            return Ok(ModelValidationResult {
                valid: false,
                gguf_version: header_result.gguf_version,
                error: Some("SHA256 checksum mismatch".to_string()),
            });
        }
    }

    Ok(ModelValidationResult {
        valid: true,
        gguf_version: header_result.gguf_version,
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_models_empty_dir() {
        // The models dir may or may not exist; list_models should not fail
        let result = list_models();
        assert!(result.is_ok(), "list_models should not fail: {:?}", result.err());
    }

    #[test]
    fn test_model_exists_nonexistent() {
        let exists = model_exists("nonexistent_model.gguf").expect("model_exists failed");
        assert!(!exists);
    }

    #[test]
    fn test_validate_gguf_header_nonexistent() {
        let result = validate_gguf_header("nonexistent_model.gguf")
            .expect("validate_gguf_header failed");
        assert!(!result.valid);
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("not found"));
    }

    #[test]
    fn test_compute_sha256_nonexistent() {
        let result = compute_sha256("nonexistent_model.gguf");
        assert!(result.is_err());
    }

    #[test]
    fn test_gguf_magic_constant() {
        assert_eq!(GGUF_MAGIC, 0x4655_4747);
    }
}
