//! Phase 9: Named Pipe client for Windows IPC.
//!
//! Async Named Pipe client using `tokio::net::windows::named_pipe` for
//! bidirectional streaming communication with the `st6_llm_worker` subprocess.

use crate::llm_protocol::{deserialize_response, serialize_response, ResponseEnvelope};
use std::io;
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};
use tokio::net::windows::named_pipe::{
    ClientOptions as TokioClientOptions, NamedPipeClient as TokioNamedPipeClient,
};

/// Default pipe name for the LLM worker.
pub const DEFAULT_PIPE_NAME: &str = r"\\.\pipe\st6_llm_worker";

/// Error type for Named Pipe operations.
#[derive(Debug, thiserror::Error)]
pub enum NamedPipeError {
    #[error("Failed to connect to named pipe {0}: {1}")]
    Connect(String, #[source] io::Error),
    #[error("Failed to write to named pipe: {0}")]
    Write(#[source] io::Error),
    #[error("Failed to read from named pipe: {0}")]
    Read(#[source] io::Error),
    #[error("Failed to serialize/deserialize message: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Pipe connection closed")]
    ConnectionClosed,
}

/// Named Pipe client for sending/receiving JSONL messages.
pub struct NamedPipeClient {
    pipe: Option<WriteHalf<TokioNamedPipeClient>>,
    reader: Option<BufReader<ReadHalf<TokioNamedPipeClient>>>,
    pipe_name: String,
}

impl NamedPipeClient {
    /// Create a new Named Pipe client targeting the given pipe name.
    pub fn new(pipe_name: &str) -> Self {
        Self {
            pipe: None,
            reader: None,
            pipe_name: pipe_name.to_string(),
        }
    }

    /// Connect to the Named Pipe server.
    pub async fn connect(&mut self) -> Result<(), NamedPipeError> {
        let pipe = TokioClientOptions::new()
            .read(true)
            .write(true)
            .open(&self.pipe_name)
            .map_err(|e| NamedPipeError::Connect(self.pipe_name.clone(), e))?;

        // Split into read/write halves for concurrent use
        let (read_half, write_half) = tokio::io::split(pipe);

        self.reader = Some(BufReader::new(read_half));
        self.pipe = Some(write_half);

        Ok(())
    }

    /// Send raw bytes over the pipe (followed by newline).
    pub async fn send_raw(&mut self, bytes: &[u8]) -> Result<(), NamedPipeError> {
        let write_half = self
            .pipe
            .as_mut()
            .ok_or(NamedPipeError::ConnectionClosed)?;

        write_half
            .write_all(bytes)
            .await
            .map_err(NamedPipeError::Write)?;
        write_half
            .write_all(b"\n")
            .await
            .map_err(NamedPipeError::Write)?;
        write_half.flush().await.map_err(NamedPipeError::Write)?;

        Ok(())
    }

    /// Send a response envelope over the pipe.
    pub async fn send(&mut self, envelope: &ResponseEnvelope) -> Result<(), NamedPipeError> {
        let line = serialize_response(envelope)?;
        self.send_raw(line.as_bytes()).await
    }

    /// Receive a single JSONL message from the pipe.
    pub async fn receive(&mut self) -> Result<ResponseEnvelope, NamedPipeError> {
        let reader = self
            .reader
            .as_mut()
            .ok_or(NamedPipeError::ConnectionClosed)?;

        let mut line = String::new();
        reader
            .read_line(&mut line)
            .await
            .map_err(NamedPipeError::Read)?;

        if line.is_empty() {
            return Err(NamedPipeError::ConnectionClosed);
        }

        let line = line.trim_end_matches(['\n', '\r']);
        deserialize_response(line).map_err(NamedPipeError::Serde)
    }

    /// Check if the pipe is connected.
    pub fn is_connected(&self) -> bool {
        self.pipe.is_some() && self.reader.is_some()
    }

    /// Disconnect from the pipe.
    pub fn disconnect(&mut self) {
        self.pipe = None;
        self.reader = None;
    }
}

impl Default for NamedPipeClient {
    fn default() -> Self {
        Self::new(DEFAULT_PIPE_NAME)
    }
}

impl AsRef<Path> for NamedPipeClient {
    fn as_ref(&self) -> &Path {
        Path::new(&self.pipe_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_pipe_name() {
        let client = NamedPipeClient::default();
        assert_eq!(client.as_ref(), Path::new(DEFAULT_PIPE_NAME));
    }

    #[test]
    fn test_custom_pipe_name() {
        let client = NamedPipeClient::new(r"\\.\pipe\custom_pipe");
        assert_eq!(client.as_ref(), Path::new(r"\\.\pipe\custom_pipe"));
    }

    #[test]
    fn test_is_connected_before_connect() {
        let client = NamedPipeClient::default();
        assert!(!client.is_connected());
    }
}
