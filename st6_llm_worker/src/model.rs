//! Phase 9: Model interface for st6_llm_worker.
//!
//! Placeholder model interface. In production, this would bind to llama.cpp
//! (via llama-cpp-sys or similar) to run the LFM3-1.7B GGUF model.

use std::io::Read;
use std::path::Path;

/// Model state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelStatus {
    Unloaded,
    Loaded,
}

/// Model wrapper for llama.cpp inference.
pub struct Model {
    status: ModelStatus,
    model_path: Option<String>,
    ctx_size: u32,
    gpu_layers: i32,
}

impl Model {
    /// Create a new unloaded model.
    pub fn new() -> Self {
        Self {
            status: ModelStatus::Unloaded,
            model_path: None,
            ctx_size: 4096,
            gpu_layers: 35,
        }
    }

    /// Load a GGUF model from the given path.
    pub fn load(
        &mut self,
        model_path: &str,
        ctx_size: u32,
        gpu_layers: i32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let path = Path::new(model_path);
        if !path.exists() {
            return Err(format!("Model file not found: {}", model_path).into());
        }

        // Validate GGUF header (magic bytes)
        let mut file = std::fs::File::open(path)?;
        let mut header = [0u8; 8];
        file.read_exact(&mut header)?;

        let magic = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        if magic != 0x4655_4747 {
            return Err(format!("Invalid GGUF magic: 0x{:08X}", magic).into());
        }

        let version = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        if version < 1 || version > 3 {
            return Err(format!("Unsupported GGUF version: {}", version).into());
        }

        self.status = ModelStatus::Loaded;
        self.model_path = Some(model_path.to_string());
        self.ctx_size = ctx_size;
        self.gpu_layers = gpu_layers;

        Ok(())
    }

    /// Check if the model is loaded.
    pub fn is_loaded(&self) -> bool {
        self.status == ModelStatus::Loaded
    }

    /// Run non-streaming inference.
    pub fn infer(
        &self,
        prompt: &str,
        temperature: f32,
        top_p: f32,
        max_tokens: u32,
    ) -> Result<String, Box<dyn std::error::Error>> {
        if !self.is_loaded() {
            return Err("Model not loaded".into());
        }

        // Placeholder: return a deterministic response based on the prompt
        Ok(format!(
            "[LLM Placeholder Response] Prompt: {} (temp={}, top_p={}, max_tokens={})",
            prompt, temperature, top_p, max_tokens
        ))
    }

    /// Run streaming inference, calling the callback for each token.
    pub fn infer_stream<F>(
        &self,
        prompt: &str,
        temperature: f32,
        top_p: f32,
        max_tokens: u32,
        mut token_callback: F,
    ) -> Result<String, Box<dyn std::error::Error>>
    where
        F: FnMut(&str),
    {
        if !self.is_loaded() {
            return Err("Model not loaded".into());
        }

        // Placeholder: generate a deterministic response token by token
        let response = format!(
            "[LLM Placeholder Response] Prompt: {} (temp={}, top_p={}, max_tokens={})",
            prompt, temperature, top_p, max_tokens
        );

        for (i, ch) in response.chars().enumerate() {
            token_callback(&ch.to_string());
            let _ = i;
        }

        Ok(response)
    }
}

impl Default for Model {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_default_unloaded() {
        let model = Model::default();
        assert!(!model.is_loaded());
    }

    #[test]
    fn test_model_load_nonexistent() {
        let mut model = Model::new();
        let result = model.load("/nonexistent/model.gguf", 4096, 35);
        assert!(result.is_err());
    }

    #[test]
    fn test_model_infer_without_load() {
        let model = Model::new();
        let result = model.infer("Hello", 0.7, 0.9, 512);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Model not loaded");
    }
}
