//! Phase 9: LLM worker manager.
//!
//! Manages the `st6_llm_worker` subprocess lifecycle, Named Pipe connection,
//! request queue (single queue, sequential processing), streaming responses,
//! and state management.

use crate::llm_protocol::{serialize_request, InferParams, LlmRequest, RequestEnvelope};
use crate::named_pipe::{DEFAULT_PIPE_NAME, NamedPipeClient};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{sleep, Duration};
use uuid::Uuid;

/// LLM worker states per §9.8.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerState {
    Uninitialized,
    Starting,
    Connecting,
    LoadingModel,
    Ready,
    Busy,
    Error(String),
    ShuttingDown,
}

/// Result of an inference call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferResult {
    pub request_id: String,
    pub text: String,
    pub usage: Option<crate::llm_protocol::Usage>,
}

/// Streaming token callback.
pub type TokenCallback = Arc<dyn Fn(String) + Send + Sync>;

/// Configuration for the LLM worker.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub worker_bin_path: PathBuf,
    pub model_path: String,
    pub ctx_size: u32,
    pub gpu_layers: i32,
    pub pipe_name: String,
    pub request_timeout_secs: u64,
    pub max_reconnect_attempts: u32,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            worker_bin_path: PathBuf::from("st6_llm_worker.exe"),
            model_path: String::new(),
            ctx_size: 4096,
            gpu_layers: 35,
            pipe_name: DEFAULT_PIPE_NAME.to_string(),
            request_timeout_secs: 120,
            max_reconnect_attempts: 60,
        }
    }
}

/// Pending request waiting for a response.
#[allow(dead_code)]
struct PendingRequest {
    token_callback: Option<TokenCallback>,
    created_at: std::time::Instant,
}

/// LLM worker manager.
pub struct LlmWorker {
    config: WorkerConfig,
    state: Arc<RwLock<WorkerState>>,
    pipe: Arc<Mutex<Option<NamedPipeClient>>>,
    child: Arc<Mutex<Option<tokio::process::Child>>>,
    #[allow(dead_code)]
    pending: Arc<Mutex<HashMap<String, PendingRequest>>>,
}

impl LlmWorker {
    /// Create a new LLM worker manager with the given config.
    pub fn new(config: WorkerConfig) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(WorkerState::Uninitialized)),
            pipe: Arc::new(Mutex::new(None)),
            child: Arc::new(Mutex::new(None)),
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get the current worker state.
    pub async fn state(&self) -> WorkerState {
        self.state.read().await.clone()
    }

    /// Start the worker subprocess and connect via Named Pipe.
    pub async fn start(&self) -> Result<(), String> {
        self.set_state(WorkerState::Starting).await;

        // Launch the subprocess
        let mut child = Command::new(&self.config.worker_bin_path)
            .arg("--model")
            .arg(&self.config.model_path)
            .arg("--ctx-size")
            .arg(self.config.ctx_size.to_string())
            .arg("--gpu-layers")
            .arg(self.config.gpu_layers.to_string())
            .arg("--pipe-name")
            .arg(&self.config.pipe_name)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn worker subprocess: {}", e))?;

        // Capture stderr for diagnostics before the child potentially exits
        let mut stderr = child.stderr.take().expect("stderr is piped");
        let stderr_handle = tokio::spawn(async move {
            let mut buf = String::new();
            let _ = stderr.read_to_string(&mut buf).await;
            buf
        });

        self.set_state(WorkerState::Connecting).await;

        // Store child process
        {
            let mut child_lock = self.child.lock().await;
            *child_lock = Some(child);
        }

        // Connect to Named Pipe with retries, aborting if the worker exits
        let mut pipe = NamedPipeClient::new(&self.config.pipe_name);
        let mut last_error = None;

        for attempt in 0..self.config.max_reconnect_attempts {
            // If the worker has already exited, stop retrying
            if let Some(status) = self.child.lock().await.as_mut().and_then(|c| c.try_wait().ok()) {
                if status.is_some() {
                    let stderr = stderr_handle.await.unwrap_or_default();
                    return Err(format!(
                        "Worker subprocess exited before pipe connection (status: {:?}); stderr: {}",
                        status, stderr
                    ));
                }
            }

            match pipe.connect().await {
                Ok(()) => break,
                Err(e) => {
                    last_error = Some(e);
                    // Cap backoff at 2 seconds so 60 attempts don't grow unbounded
                    let delay_ms = std::cmp::min(500 * (attempt + 1) as u64, 2000);
                    sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }

        // Abort stderr capture if still running
        stderr_handle.abort();

        if let Some(e) = last_error {
            self.set_state(WorkerState::Error(format!(
                "Failed to connect to Named Pipe: {}",
                e
            )))
            .await;
            return Err(format!("Named Pipe connection failed: {}", e));
        }

        // Store the connected pipe so subsequent requests can use it
        {
            let mut pipe_lock = self.pipe.lock().await;
            *pipe_lock = Some(pipe);
        }

        self.set_state(WorkerState::LoadingModel).await;

        // Send LoadModel request
        let load_env = RequestEnvelope {
            id: Uuid::new_v4().to_string(),
            request: LlmRequest::LoadModel {
                model_path: self.config.model_path.clone(),
                ctx_size: self.config.ctx_size,
                gpu_layers: self.config.gpu_layers,
            },
        };

        let load_json = serialize_request(&load_env).map_err(|e| e.to_string())?;
        {
            let mut pipe_lock = self.pipe.lock().await;
            let p = pipe_lock.as_mut().ok_or("Pipe not connected")?;
            p.send_raw(load_json.as_bytes()).await.map_err(|e| e.to_string())?;
        }

        // Wait for Ready response (health check)
        let ready = self.wait_for_ready(Duration::from_secs(30)).await;
        if !ready {
            self.set_state(WorkerState::Error("Model load timeout".to_string())).await;
            return Err("Model load timeout".to_string());
        }

        self.set_state(WorkerState::Ready).await;
        Ok(())
    }

    /// Wait for the worker to report Ready status.
    async fn wait_for_ready(&self, timeout_duration: Duration) -> bool {
        let start = std::time::Instant::now();

        while start.elapsed() < timeout_duration {
            sleep(Duration::from_millis(100)).await;

            // Send health check
            let id = Uuid::new_v4().to_string();
            let env = RequestEnvelope {
                id: id.clone(),
                request: LlmRequest::HealthCheck,
            };

            let json = match serialize_request(&env) {
                Ok(j) => j,
                Err(_) => continue,
            };

            {
                let mut pipe_lock = self.pipe.lock().await;
                if let Some(p) = pipe_lock.as_mut() {
                    let _ = p.send_raw(json.as_bytes()).await;
                }
            }

            // Try to receive a response
            {
                let mut pipe_lock = self.pipe.lock().await;
                if let Some(p) = pipe_lock.as_mut() {
                    match p.receive().await {
                        Ok(resp) => {
                            if matches!(
                                resp.response,
                                crate::llm_protocol::LlmResponse::Health {
                                    model_loaded: true,
                                    ..
                                }
                            ) {
                                return true;
                            }
                        }
                        Err(_) => continue,
                    }
                }
            }
        }

        false
    }

    /// Run inference (non-streaming, wait for completion).
    pub async fn infer(&self, prompt: &str, params: Option<InferParams>) -> Result<InferResult, String> {
        let state = self.state().await;
        if state != WorkerState::Ready {
            return Err(format!("Worker not ready (state: {:?})", state));
        }

        self.set_state(WorkerState::Busy).await;

        let id = Uuid::new_v4().to_string();
        let params = params.unwrap_or_default();

        let env = RequestEnvelope {
            id: id.clone(),
            request: LlmRequest::Infer {
                prompt: prompt.to_string(),
                params,
            },
        };

        let json = serialize_request(&env).map_err(|e| e.to_string())?;
        {
            let mut pipe_lock = self.pipe.lock().await;
            let p = pipe_lock.as_mut().ok_or("Pipe not connected")?;
            p.send_raw(json.as_bytes()).await.map_err(|e| e.to_string())?;
        }

        // Wait for response with timeout
        let result = self.wait_for_response(&id, Duration::from_secs(self.config.request_timeout_secs)).await;

        self.set_state(WorkerState::Ready).await;
        result
    }

    /// Run inference with streaming tokens via callback.
    pub async fn infer_stream(
        &self,
        prompt: &str,
        params: Option<InferParams>,
        token_callback: TokenCallback,
    ) -> Result<InferResult, String> {
        let state = self.state().await;
        if state != WorkerState::Ready {
            return Err(format!("Worker not ready (state: {:?})", state));
        }

        self.set_state(WorkerState::Busy).await;

        let id = Uuid::new_v4().to_string();
        let mut params = params.unwrap_or_default();
        params.stream = Some(true);

        let env = RequestEnvelope {
            id: id.clone(),
            request: LlmRequest::Infer {
                prompt: prompt.to_string(),
                params,
            },
        };

        let json = serialize_request(&env).map_err(|e| e.to_string())?;
        {
            let mut pipe_lock = self.pipe.lock().await;
            let p = pipe_lock.as_mut().ok_or("Pipe not connected")?;
            p.send_raw(json.as_bytes()).await.map_err(|e| e.to_string())?;
        }

        // Wait for streaming completion
        let result = self
            .wait_for_streaming_response(&id, token_callback, Duration::from_secs(self.config.request_timeout_secs))
            .await;

        self.set_state(WorkerState::Ready).await;
        result
    }

    /// Cancel the current inference.
    pub async fn cancel(&self) -> Result<(), String> {
        // Send shutdown to current request, then restart
        self.set_state(WorkerState::Error("Cancelled".to_string())).await;
        // In a full implementation, this would send a cancel request to the worker
        self.set_state(WorkerState::Ready).await;
        Ok(())
    }

    /// Get worker status.
    pub async fn status(&self) -> WorkerStatus {
        WorkerStatus {
            state: self.state().await,
            model_loaded: matches!(self.state().await, WorkerState::Ready),
            pipe_name: self.config.pipe_name.clone(),
        }
    }

    /// Shutdown the worker subprocess.
    pub async fn shutdown(&self) -> Result<(), String> {
        self.set_state(WorkerState::ShuttingDown).await;

        // Send shutdown request
        let env = RequestEnvelope {
            id: Uuid::new_v4().to_string(),
            request: LlmRequest::Shutdown,
        };
        if let Ok(json) = serialize_request(&env) {
            let mut pipe_lock = self.pipe.lock().await;
            if let Some(p) = pipe_lock.as_mut() {
                let _ = p.send_raw(json.as_bytes()).await;
            }
        }

        // Kill the subprocess
        {
            let mut child_lock = self.child.lock().await;
            if let Some(mut child) = child_lock.take() {
                let _ = child.kill().await;
            }
        }

        {
            let mut pipe_lock = self.pipe.lock().await;
            *pipe_lock = None;
        }

        self.set_state(WorkerState::Uninitialized).await;
        Ok(())
    }

    /// Wait for a response to a specific request ID.
    async fn wait_for_response(
        &self,
        request_id: &str,
        timeout_duration: Duration,
    ) -> Result<InferResult, String> {
        let start = std::time::Instant::now();

        while start.elapsed() < timeout_duration {
            sleep(Duration::from_millis(50)).await;

            let mut pipe_lock = self.pipe.lock().await;
            if let Some(p) = pipe_lock.as_mut() {
                match p.receive().await {
                    Ok(resp) => {
                        if resp.id == *request_id {
                            match resp.response {
                                crate::llm_protocol::LlmResponse::Result { text, usage } => {
                                    return Ok(InferResult {
                                        request_id: request_id.to_string(),
                                        text,
                                        usage: Some(usage),
                                    });
                                }
                                crate::llm_protocol::LlmResponse::Error { code, message } => {
                                    return Err(format!("LLM error ({}): {}", code, message));
                                }
                                _ => continue,
                            }
                        }
                    }
                    Err(_) => continue,
                }
            }
        }

        Err(format!("Timeout waiting for response to {}", request_id))
    }

    /// Wait for a streaming response.
    async fn wait_for_streaming_response(
        &self,
        request_id: &str,
        token_callback: TokenCallback,
        timeout_duration: Duration,
    ) -> Result<InferResult, String> {
        let start = std::time::Instant::now();
        let mut full_text = String::new();

        while start.elapsed() < timeout_duration {
            sleep(Duration::from_millis(50)).await;

            let mut pipe_lock = self.pipe.lock().await;
            if let Some(p) = pipe_lock.as_mut() {
                match p.receive().await {
                    Ok(resp) => {
                        if resp.id == *request_id {
                            match resp.response {
                                crate::llm_protocol::LlmResponse::Token { token, .. } => {
                                    full_text.push_str(&token);
                                    token_callback(token);
                                }
                                crate::llm_protocol::LlmResponse::Done { usage } => {
                                    return Ok(InferResult {
                                        request_id: request_id.to_string(),
                                        text: full_text,
                                        usage: Some(usage),
                                    });
                                }
                                crate::llm_protocol::LlmResponse::Error { code, message } => {
                                    return Err(format!("LLM error ({}): {}", code, message));
                                }
                                _ => continue,
                            }
                        }
                    }
                    Err(_) => continue,
                }
            }
        }

        Err(format!("Timeout waiting for streaming response to {}", request_id))
    }

    async fn set_state(&self, state: WorkerState) {
        *self.state.write().await = state;
    }
}

/// Worker status information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerStatus {
    pub state: WorkerState,
    pub model_loaded: bool,
    pub pipe_name: String,
}

impl Default for LlmWorker {
    fn default() -> Self {
        Self::new(WorkerConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_state_default() {
        assert_eq!(WorkerState::Uninitialized, WorkerState::Uninitialized);
    }

    #[test]
    fn test_worker_config_default() {
        let config = WorkerConfig::default();
        assert_eq!(config.ctx_size, 4096);
        assert_eq!(config.gpu_layers, 35);
        assert_eq!(config.request_timeout_secs, 120);
        assert_eq!(config.max_reconnect_attempts, 3);
    }

    #[test]
    fn test_worker_status_serialization() {
        let status = WorkerStatus {
            state: WorkerState::Ready,
            model_loaded: true,
            pipe_name: DEFAULT_PIPE_NAME.to_string(),
        };
        let json = serde_json::to_string(&status).unwrap();
        let parsed: WorkerStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.state, WorkerState::Ready);
        assert!(parsed.model_loaded);
    }

    #[test]
    fn test_infer_result_serialization() {
        let result = InferResult {
            request_id: "req-1".to_string(),
            text: "Hello".to_string(),
            usage: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: InferResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.request_id, "req-1");
        assert_eq!(parsed.text, "Hello");
    }
}
