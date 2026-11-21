//! Phase 9: LLM IPC protocol definition.
//!
//! JSONL (JSON Lines) framing over Named Pipe between the Rust backend and the
//! `st6_llm_worker` subprocess. All messages are single-line JSON objects.

use serde::{Deserialize, Serialize};

/// Protocol version header sent with the first message after pipe connection.
pub const PROTOCOL_VERSION: &str = "st6-llm-ipc/1.0";

/// Request message sent from the Rust backend to the worker subprocess.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InferParams {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub stream: Option<bool>,
}

/// Response message sent from the worker to the Rust backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// Serialize a request envelope to a JSONL line.
pub fn serialize_request(env: &RequestEnvelope) -> Result<String, serde_json::Error> {
    serde_json::to_string(env)
}

/// Deserialize a JSONL line into a request envelope.
pub fn deserialize_request(line: &str) -> Result<RequestEnvelope, serde_json::Error> {
    serde_json::from_str(line)
}

/// Serialize a response envelope to a JSONL line.
pub fn serialize_response(env: &ResponseEnvelope) -> Result<String, serde_json::Error> {
    serde_json::to_string(env)
}

/// Deserialize a JSONL line into a response envelope.
pub fn deserialize_response(line: &str) -> Result<ResponseEnvelope, serde_json::Error> {
    serde_json::from_str(line)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_deserialize_infer_request() {
        let env = RequestEnvelope {
            id: "req-123".to_string(),
            request: LlmRequest::Infer {
                prompt: "Hello, world!".to_string(),
                params: InferParams::default(),
            },
        };
        let json = serialize_request(&env).unwrap();
        let parsed: RequestEnvelope = deserialize_request(&json).unwrap();
        assert_eq!(parsed.id, "req-123");
        assert_eq!(parsed.request, env.request);
    }

    #[test]
    fn test_serialize_deserialize_token_response() {
        let env = ResponseEnvelope {
            id: "req-123".to_string(),
            response: LlmResponse::Token {
                token: "Hello".to_string(),
                index: 0,
            },
        };
        let json = serialize_response(&env).unwrap();
        let parsed: ResponseEnvelope = deserialize_response(&json).unwrap();
        assert_eq!(parsed.id, "req-123");
        assert_eq!(parsed.response, env.response);
    }

    #[test]
    fn test_serialize_deserialize_error_response() {
        let env = ResponseEnvelope {
            id: "req-123".to_string(),
            response: LlmResponse::Error {
                code: "MODEL_NOT_LOADED".to_string(),
                message: "Model file not found".to_string(),
            },
        };
        let json = serialize_response(&env).unwrap();
        let parsed: ResponseEnvelope = deserialize_response(&json).unwrap();
        assert_eq!(parsed.id, "req-123");
        assert_eq!(parsed.response, env.response);
    }

    #[test]
    fn test_jsonl_line_is_single_line() {
        let env = RequestEnvelope {
            id: "req-1".to_string(),
            request: LlmRequest::HealthCheck,
        };
        let json = serialize_request(&env).unwrap();
        assert!(!json.contains('\n'));
        assert!(!json.contains('\r'));
    }
}
