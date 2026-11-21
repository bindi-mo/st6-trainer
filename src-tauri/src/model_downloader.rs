//! Phase 8: LLM model downloader.
//!
//! Downloads GGUF model files from Hugging Face with progress reporting and
//! resume support (HTTP range requests).

use crate::paths;
use futures_util::StreamExt;
use reqwest::Client;
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::Write;

/// Progress callback type: (bytes_downloaded, total_bytes)
pub type ProgressCallback = Box<dyn Fn(u64, Option<u64>) + Send + Sync>;

/// Download result
#[derive(Debug, Clone, Serialize)]
pub struct DownloadResult {
    pub model_file_name: String,
    pub total_bytes: u64,
    pub sha256: Option<String>,
}

/// Download a model file from a URL to the models directory.
///
/// Supports resume: if a partial file exists, it will attempt to continue
/// from where it left off using HTTP range requests.
pub async fn download_model(
    url: &str,
    model_file_name: &str,
    progress: Option<ProgressCallback>,
) -> Result<DownloadResult, String> {
    let models_dir = paths::resolve_models_dir()?;
    let dest_path = models_dir.join(model_file_name);

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    // Check existing partial file size for resume
    let mut start_byte: u64 = 0;
    if dest_path.exists() {
        let metadata = std::fs::metadata(&dest_path).map_err(|e| {
            format!("Failed to stat partial file {}: {}", dest_path.display(), e)
        })?;
        start_byte = metadata.len();
    }

    // Build request with range header if resuming
    let mut request = client.get(url);
    if start_byte > 0 {
        request = request.header("Range", format!("bytes={}-", start_byte));
    }

    let response = request.send().await.map_err(|e| {
        format!("Failed to send download request to {}: {}", url, e)
    })?;

    let status = response.status();
    let total_size = response
        .content_length()
        .map(|cl| if start_byte > 0 { cl + start_byte } else { cl });

    // Determine if server supports resume (206 Partial Content)
    let supports_resume = status.as_u16() == 206;

    if start_byte > 0 && !supports_resume {
        // Server doesn't support resume; restart from scratch
        start_byte = 0;
    }

    // Open file for writing (append if resuming, create/truncate otherwise)
    let mut file = if start_byte > 0 {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&dest_path)
            .map_err(|e| {
                format!(
                    "Failed to open partial file for append {}: {}",
                    dest_path.display(),
                    e
                )
            })?
    } else {
        File::create(&dest_path).map_err(|e| {
            format!("Failed to create file {}: {}", dest_path.display(), e)
        })?
    };

    let mut downloaded: u64 = start_byte;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download stream error: {}", e))?;
        file.write_all(&chunk)
            .map_err(|e| format!("Failed to write to file {}: {}", dest_path.display(), e))?;
        downloaded += chunk.len() as u64;

        if let Some(ref cb) = progress {
            cb(downloaded, total_size);
        }
    }

    file.flush().map_err(|e| {
        format!("Failed to flush file {}: {}", dest_path.display(), e)
    })?;
    drop(file);

    // Compute SHA256 of the downloaded file
    let sha256 = crate::model_manager::compute_sha256(model_file_name).ok();

    Ok(DownloadResult {
        model_file_name: model_file_name.to_string(),
        total_bytes: downloaded,
        sha256,
    })
}

/// Download a model file from Hugging Face.
///
/// Constructs the Hugging Face resolve URL from repo and file name.
pub async fn download_from_huggingface(
    repo: &str,
    file_name: &str,
    progress: Option<ProgressCallback>,
) -> Result<DownloadResult, String> {
    let url = format!("https://huggingface.co/{}/resolve/main/{}", repo, file_name);
    download_model(&url, file_name, progress).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_result_debug() {
        let result = DownloadResult {
            model_file_name: "test.gguf".to_string(),
            total_bytes: 1024,
            sha256: Some("abc123".to_string()),
        };
        assert_eq!(result.model_file_name, "test.gguf");
        assert_eq!(result.total_bytes, 1024);
        assert_eq!(result.sha256, Some("abc123".to_string()));
    }
}
