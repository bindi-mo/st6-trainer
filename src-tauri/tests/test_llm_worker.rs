//! Phase 15.2: Unit tests for LLM worker (Phase 9).
//! Covers IPC protocol serialization, Named Pipe client, worker states,
//! and InferResult/WorkerStatus serialization.

use serial_test::serial;

#[test]
#[serial]
fn test_llm_protocol_serialize_deserialize_infer() {
    use st6_trainer_lib::llm_protocol::{
        deserialize_request, deserialize_response, serialize_request, serialize_response,
        InferParams, LlmRequest, LlmResponse, RequestEnvelope, ResponseEnvelope, Usage,
    };

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
    assert!(!json.contains('\n'));
    assert!(!json.contains('\r'));
}

#[test]
#[serial]
fn test_llm_protocol_serialize_deserialize_token() {
    use st6_trainer_lib::llm_protocol::{
        deserialize_response, serialize_response, LlmResponse, ResponseEnvelope, Usage,
    };

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
#[serial]
fn test_llm_protocol_serialize_deserialize_error() {
    use st6_trainer_lib::llm_protocol::{
        deserialize_response, serialize_response, LlmResponse, ResponseEnvelope,
    };

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
#[serial]
fn test_llm_protocol_health_and_shutdown() {
    use st6_trainer_lib::llm_protocol::{
        deserialize_request, serialize_request, LlmRequest, RequestEnvelope,
    };

    let health = RequestEnvelope {
        id: "hc-1".to_string(),
        request: LlmRequest::HealthCheck,
    };
    let json = serialize_request(&health).unwrap();
    let parsed: RequestEnvelope = deserialize_request(&json).unwrap();
    assert_eq!(parsed.id, "hc-1");
    assert!(matches!(parsed.request, LlmRequest::HealthCheck));

    let shutdown = RequestEnvelope {
        id: "sh-1".to_string(),
        request: LlmRequest::Shutdown,
    };
    let json = serialize_request(&shutdown).unwrap();
    let parsed: RequestEnvelope = deserialize_request(&json).unwrap();
    assert!(matches!(parsed.request, LlmRequest::Shutdown));
}

#[test]
#[serial]
fn test_llm_worker_state_default_and_transitions() {
    use st6_trainer_lib::llm_worker::WorkerState;

    assert_eq!(WorkerState::Uninitialized, WorkerState::Uninitialized);
    assert_ne!(WorkerState::Uninitialized, WorkerState::Ready);
    assert_ne!(WorkerState::Ready, WorkerState::Busy);
    assert_ne!(WorkerState::Ready, WorkerState::Error("err".to_string()));
}

#[test]
#[serial]
fn test_llm_worker_config_default() {
    use st6_trainer_lib::llm_worker::WorkerConfig;
    use std::path::PathBuf;

    let config = WorkerConfig::default();
    assert_eq!(config.ctx_size, 4096);
    assert_eq!(config.gpu_layers, 35);
    assert_eq!(config.request_timeout_secs, 120);
    assert_eq!(config.max_reconnect_attempts, 3);
    assert_eq!(config.worker_bin_path, PathBuf::from("st6_llm_worker.exe"));
    assert!(config.model_path.is_empty());
}

#[test]
#[serial]
fn test_llm_worker_status_serialization() {
    use st6_trainer_lib::llm_worker::{WorkerStatus, WorkerState};
    use st6_trainer_lib::named_pipe::DEFAULT_PIPE_NAME;

    let status = WorkerStatus {
        state: WorkerState::Ready,
        model_loaded: true,
        pipe_name: DEFAULT_PIPE_NAME.to_string(),
    };
    let json = serde_json::to_string(&status).unwrap();
    let parsed: WorkerStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.state, WorkerState::Ready);
    assert!(parsed.model_loaded);
    assert_eq!(parsed.pipe_name, DEFAULT_PIPE_NAME);
}

#[test]
#[serial]
fn test_llm_infer_result_serialization() {
    use st6_trainer_lib::llm_worker::InferResult;
    use st6_trainer_lib::llm_protocol::Usage;

    let result = InferResult {
        request_id: "req-1".to_string(),
        text: "Hello".to_string(),
        usage: Some(Usage {
            prompt_tokens: 10,
            completion_tokens: 20,
        }),
    };
    let json = serde_json::to_string(&result).unwrap();
    let parsed: InferResult = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.request_id, "req-1");
    assert_eq!(parsed.text, "Hello");
    let usage = parsed.usage.expect("usage should be present");
    assert_eq!(usage.prompt_tokens, 10);
    assert_eq!(usage.completion_tokens, 20);
}

#[test]
#[serial]
fn test_named_pipe_client_default_and_custom() {
    use st6_trainer_lib::named_pipe::{DEFAULT_PIPE_NAME, NamedPipeClient};
    use std::path::Path;

    let client = NamedPipeClient::default();
    assert_eq!(client.as_ref(), Path::new(DEFAULT_PIPE_NAME));
    assert!(!client.is_connected());

    let custom = NamedPipeClient::new(r"\\.\pipe\custom_pipe");
    assert_eq!(custom.as_ref(), Path::new(r"\\.\pipe\custom_pipe"));
    assert!(!custom.is_connected());
}

#[test]
#[serial]
fn test_llm_protocol_infer_params_default() {
    use st6_trainer_lib::llm_protocol::InferParams;

    let params = InferParams::default();
    assert_eq!(params.temperature, Some(0.7));
    assert_eq!(params.top_p, Some(0.9));
    assert_eq!(params.max_tokens, Some(512));
    assert_eq!(params.stream, Some(true));
}
