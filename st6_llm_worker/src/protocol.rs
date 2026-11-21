//! Phase 9: IPC protocol types for st6_llm_worker.
//!
//! Mirrors the protocol definitions in the main backend crate.

use serde::{Deserialize, Serialize};

/// Request message sent from the Rust backend to the worker subprocess.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum LlmRequest {
    /// Load a model (sent at startup).
    LoadModel {
        model_path: String,
        ctx_size: u32,
        gpu_layers: i32,
    },
    /// Run inference.
    Infer {
        prompt: String,
        params: InferParams,
    },
    /// Health check.
    HealthCheck,
    /// Shutdown the worker.
    Shutdown,
}

/// Inference parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferParams {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub stream: Option<bool>,
}

/// Response message sent from the worker to the Rust backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum LlmResponse {
    /// A single streaming token.
    Token { token: String, index: u64 },
    /// Inference complete (non-streaming).
    Result { text: String, usage: Usage },
    /// Streaming complete.
    Done { usage: Usage },
    /// Health check response.
    Health { status: String, model_loaded: bool },
    /// Shutdown acknowledgment.
    Ack,
    /// Error response.
    Error { code: String, message: String },
}

/// Token usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

/// Envelope wrapping a request with a correlation ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestEnvelope {
    pub id: String,
    pub request: LlmRequest,
}

/// Envelope wrapping a response with a correlation ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseEnvelope {
    pub id: String,
    pub response: LlmResponse,
}

impl Default for InferParams {
    fn default() -> Self {
        Self {
            temperature: Some(0.7),
            top_p: Some(0.9),
            max_tokens: Some(512),
            stream: Some(true),
        }
    }
}
