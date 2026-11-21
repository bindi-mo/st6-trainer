//! Phase 9: st6_llm_worker subprocess.
//!
//! Standalone executable that runs llama.cpp (LFM3-1.7B GGUF) as a child process.
//! Communicates with the Rust backend via Named Pipe (Windows) using JSONL framing.

use std::process::exit;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::ServerOptions;

mod model;
mod protocol;

use crate::model::Model;
use crate::protocol::{
    LlmRequest, LlmResponse, RequestEnvelope, ResponseEnvelope, Usage,
};

/// Default pipe name.
const DEFAULT_PIPE_NAME: &str = r"\\.\pipe\st6_llm_worker";

#[tokio::main]
async fn main() {
    let args = Args::parse();

    eprintln!(
        "[st6_llm_worker] Starting worker, pipe={}, model={}",
        args.pipe_name, args.model_path
    );

    if let Err(e) = run_worker(&args).await {
        eprintln!("[st6_llm_worker] Fatal error: {}", e);
        exit(1);
    }
}

struct Args {
    model_path: String,
    ctx_size: u32,
    gpu_layers: i32,
    pipe_name: String,
}

impl Args {
    fn parse() -> Self {
        let mut model_path = String::new();
        let mut ctx_size: u32 = 4096;
        let mut gpu_layers: i32 = 35;
        let mut pipe_name = DEFAULT_PIPE_NAME.to_string();

        let mut argv = std::env::args().skip(1);
        while let Some(arg) = argv.next() {
            match arg.as_str() {
                "--model" => model_path = argv.next().expect("Missing value for --model"),
                "--ctx-size" => {
                    ctx_size = argv
                        .next()
                        .expect("Missing value for --ctx-size")
                        .parse()
                        .expect("Invalid --ctx-size value")
                }
                "--gpu-layers" => {
                    gpu_layers = argv
                        .next()
                        .expect("Missing value for --gpu-layers")
                        .parse()
                        .expect("Invalid --gpu-layers value")
                }
                "--pipe-name" => {
                    pipe_name = argv.next().expect("Missing value for --pipe-name")
                }
                _ => {
                    eprintln!("[st6_llm_worker] Unknown argument: {}", arg);
                }
            }
        }

        Self {
            model_path,
            ctx_size,
            gpu_layers,
            pipe_name,
        }
    }
}

async fn run_worker(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    // Load the model
    let mut model = Model::new();
    model.load(&args.model_path, args.ctx_size, args.gpu_layers)?;
    eprintln!("[st6_llm_worker] Model loaded successfully");

    // Create Named Pipe server loop
    loop {
        eprintln!(
            "[st6_llm_worker] Waiting for pipe connection: {}",
            args.pipe_name
        );

        let pipe = ServerOptions::new()
            .first_pipe_instance(true)
            .create(&args.pipe_name)
            .map_err(|e| format!("Failed to create pipe: {}", e))?;

        // Wait for client to connect
        pipe.connect().await.map_err(|e| format!("Failed to connect pipe: {}", e))?;
        eprintln!("[st6_llm_worker] Client connected");

        // Handle the connection
        if let Err(e) = handle_connection(pipe, &mut model).await {
            eprintln!("[st6_llm_worker] Connection error: {}", e);
        }

        eprintln!(
            "[st6_llm_worker] Client disconnected, waiting for next connection"
        );
    }
}

async fn handle_connection(
    pipe: tokio::net::windows::named_pipe::NamedPipeServer,
    model: &mut Model,
) -> Result<(), Box<dyn std::error::Error>> {
    let (read_half, mut write_half) = tokio::io::split(pipe);
    let mut reader = BufReader::new(read_half);

    loop {
        let mut line = String::new();
        let bytes_read = reader
            .read_line(&mut line)
            .await
            .map_err(|e| format!("Failed to read from pipe: {}", e))?;

        if bytes_read == 0 {
            // Client disconnected
            break;
        }

        let line = line.trim_end_matches(['\n', '\r']);
        if line.is_empty() {
            continue;
        }

        // Parse the request envelope
        let request_env: RequestEnvelope = match serde_json::from_str(line) {
            Ok(env) => env,
            Err(e) => {
                let error_resp = ResponseEnvelope {
                    id: "unknown".to_string(),
                    response: LlmResponse::Error {
                        code: "INVALID_REQUEST".to_string(),
                        message: format!("Failed to parse request: {}", e),
                    },
                };
                let json = serde_json::to_string(&error_resp).unwrap();
                write_half
                    .write_all(json.as_bytes())
                    .await
                    .map_err(|e| format!("Failed to write to pipe: {}", e))?;
                write_half
                    .write_all(b"\n")
                    .await
                    .map_err(|e| format!("Failed to write to pipe: {}", e))?;
                write_half
                    .flush()
                    .await
                    .map_err(|e| format!("Failed to flush pipe: {}", e))?;
                continue;
            }
        };

        let response = handle_request(request_env, model);

        // Check for shutdown before moving response
        let is_shutdown = matches!(response.response, LlmResponse::Ack);

        let response_env = ResponseEnvelope {
            id: response.id,
            response: response.response,
        };

        let json = serde_json::to_string(&response_env).unwrap();
        write_half
            .write_all(json.as_bytes())
            .await
            .map_err(|e| format!("Failed to write to pipe: {}", e))?;
        write_half
            .write_all(b"\n")
            .await
            .map_err(|e| format!("Failed to write to pipe: {}", e))?;
        write_half
            .flush()
            .await
            .map_err(|e| format!("Failed to flush pipe: {}", e))?;

        // If shutdown, break the loop
        if is_shutdown {
            break;
        }
    }

    Ok(())
}

struct RequestResult {
    id: String,
    response: LlmResponse,
}

fn handle_request(env: RequestEnvelope, model: &mut Model) -> RequestResult {
    match env.request {
        LlmRequest::LoadModel {
            model_path,
            ctx_size,
            gpu_layers,
        } => {
            let result = model.load(&model_path, ctx_size, gpu_layers);
            match result {
                Ok(()) => RequestResult {
                    id: env.id,
                    response: LlmResponse::Health {
                        status: "ready".to_string(),
                        model_loaded: true,
                    },
                },
                Err(e) => RequestResult {
                    id: env.id,
                    response: LlmResponse::Error {
                        code: "MODEL_LOAD_FAILED".to_string(),
                        message: e.to_string(),
                    },
                },
            }
        }
        LlmRequest::Infer { prompt, params } => {
            let temperature = params.temperature.unwrap_or(0.7);
            let top_p = params.top_p.unwrap_or(0.9);
            let max_tokens = params.max_tokens.unwrap_or(512);
            let stream = params.stream.unwrap_or(true);

            if stream {
                match model.infer_stream(
                    &prompt,
                    temperature,
                    top_p,
                    max_tokens,
                    |_token| {
                        // Tokens handled by caller
                    },
                ) {
                    Ok(_full_text) => RequestResult {
                        id: env.id.clone(),
                        response: LlmResponse::Done {
                            usage: Usage {
                                prompt_tokens: 0,
                                completion_tokens: 0,
                            },
                        },
                    },
                    Err(e) => RequestResult {
                        id: env.id,
                        response: LlmResponse::Error {
                            code: "INFERENCE_FAILED".to_string(),
                            message: e.to_string(),
                        },
                    },
                }
            } else {
                match model.infer(&prompt, temperature, top_p, max_tokens) {
                    Ok(text) => RequestResult {
                        id: env.id,
                        response: LlmResponse::Result {
                            text,
                            usage: Usage {
                                prompt_tokens: 0,
                                completion_tokens: 0,
                            },
                        },
                    },
                    Err(e) => RequestResult {
                        id: env.id,
                        response: LlmResponse::Error {
                            code: "INFERENCE_FAILED".to_string(),
                            message: e.to_string(),
                        },
                    },
                }
            }
        }
        LlmRequest::HealthCheck => RequestResult {
            id: env.id,
            response: LlmResponse::Health {
                status: "ready".to_string(),
                model_loaded: model.is_loaded(),
            },
        },
        LlmRequest::Shutdown => RequestResult {
            id: env.id,
            response: LlmResponse::Ack,
        },
    }
}
