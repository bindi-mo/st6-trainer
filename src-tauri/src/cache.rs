//! Scan result disk cache module.
//!
//! Provides caching of scan results to avoid re-scanning videos when
//! post-scan processing needs to be re-run.

use crate::paths::resolve_scan_cache_dir;
use crate::scanner_ffi::{FrameTelemetryOut, InputEventOut, MatchMetadataOut, MoveEventOut};
use crate::pipeline::InvariantResult;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// Cached scan result containing all data needed to re-run post-scan processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedScanResult {
    pub metadata: MatchMetadataOut,
    pub telemetry: Vec<FrameTelemetryOut>,
    pub input_events: Vec<InputEventOut>,
    pub move_events: Vec<MoveEventOut>,
    pub invariant_result: InvariantResult,
    /// Cache metadata
    pub cached_at: String, // ISO 8601 timestamp
    pub video_sha256: String,
    pub config_version: String,
    pub template_pack_version: String,
}

/// Information about a cache entry for UI display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntryInfo {
    pub cache_key: String,
    pub video_path: String,
    pub video_sha256: String,
    pub match_id: String,
    pub character_p1: String,
    pub character_p2: String,
    pub total_frames: i32,
    pub cached_at: String,
    pub file_size_bytes: u64,
}

/// Compute SHA256 hash of a file.
pub fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = std::io::Read::read(&mut file, &mut buffer)
            .map_err(|e| format!("Failed to read file: {}", e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Generate cache key from video hash and scanner versions.
pub fn generate_cache_key(
    video_sha256: &str,
    config_version: &str,
    template_pack_version: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(video_sha256.as_bytes());
    hasher.update(config_version.as_bytes());
    hasher.update(template_pack_version.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Get the cache file path for a given cache key.
fn cache_file_path(cache_key: &str) -> Result<PathBuf, String> {
    let cache_dir = resolve_scan_cache_dir()?;
    Ok(cache_dir.join(format!("{}.json", cache_key)))
}

/// Save scan result to cache.
pub fn save_scan_cache(result: &CachedScanResult) -> Result<(), String> {
    let cache_key = generate_cache_key(
        &result.video_sha256,
        &result.config_version,
        &result.template_pack_version,
    );
    let path = cache_file_path(&cache_key)?;
    let json = serde_json::to_string_pretty(result)
        .map_err(|e| format!("Failed to serialize cache: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write cache file: {}", e))?;
    Ok(())
}

/// Load scan result from cache.
pub fn load_scan_cache(cache_key: &str) -> Result<CachedScanResult, String> {
    let path = cache_file_path(cache_key)?;
    if !path.exists() {
        return Err("Cache file not found".to_string());
    }
    let json = fs::read_to_string(&path).map_err(|e| format!("Failed to read cache file: {}", e))?;
    let result: CachedScanResult = serde_json::from_str(&json)
        .map_err(|e| format!("Failed to deserialize cache: {}", e))?;
    Ok(result)
}

/// Check if a valid cache exists for the given video and scanner versions.
pub fn check_cache_exists(
    video_path: &Path,
    config_version: &str,
    template_pack_version: &str,
) -> Result<Option<String>, String> {
    let video_sha256 = sha256_file(video_path)?;
    let cache_key = generate_cache_key(&video_sha256, config_version, template_pack_version);
    let path = cache_file_path(&cache_key)?;
    if path.exists() {
        Ok(Some(cache_key))
    } else {
        Ok(None)
    }
}

/// List all cache entries with metadata.
pub fn list_cache_entries() -> Result<Vec<CacheEntryInfo>, String> {
    let cache_dir = resolve_scan_cache_dir()?;
    let mut entries = Vec::new();

    for entry in fs::read_dir(&cache_dir).map_err(|e| format!("Failed to read cache dir: {}", e))? {
        let entry = entry.map_err(|e| format!("Failed to read dir entry: {}", e))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Ok(json) = fs::read_to_string(&path) {
                if let Ok(cached) = serde_json::from_str::<CachedScanResult>(&json) {
                    let file_size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    entries.push(CacheEntryInfo {
                        cache_key: path.file_stem().unwrap().to_string_lossy().to_string(),
                        video_path: cached.metadata.source_resolution.clone(), // placeholder
                        video_sha256: cached.video_sha256,
                        match_id: cached.metadata.match_id.clone(),
                        character_p1: cached.metadata.character_name_p1.clone(),
                        character_p2: cached.metadata.character_name_p2.clone(),
                        total_frames: cached.metadata.total_frames,
                        cached_at: cached.cached_at,
                        file_size_bytes: file_size,
                    });
                }
            }
        }
    }

    // Sort by cached_at descending (newest first)
    entries.sort_by(|a, b| b.cached_at.cmp(&a.cached_at));
    Ok(entries)
}

/// Clear all scan cache files.
pub fn clear_scan_cache() -> Result<usize, String> {
    let cache_dir = resolve_scan_cache_dir()?;
    let mut count = 0;
    for entry in fs::read_dir(&cache_dir).map_err(|e| format!("Failed to read cache dir: {}", e))? {
        let entry = entry.map_err(|e| format!("Failed to read dir entry: {}", e))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            fs::remove_file(&path).map_err(|e| format!("Failed to remove cache file: {}", e))?;
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_sha256_file() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"test content").unwrap();
        let hash = sha256_file(file.path()).unwrap();
        assert_eq!(hash.len(), 64); // SHA256 hex string length
    }

    #[test]
    fn test_generate_cache_key() {
        let key1 = generate_cache_key("abc", "v1", "tp1");
        let key2 = generate_cache_key("abc", "v1", "tp1");
        let key3 = generate_cache_key("def", "v1", "tp1");
        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
        assert_eq!(key1.len(), 64);
    }
}