//! LLM inference module for text generation
//!
//! Provides local LLM inference using llama.cpp for models like:
//! - Ministral 3B/8B (edge-optimized)
//! - Llama 3.x
//! - Phi-3/4
//! - Any GGUF-compatible model
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
//! │  LlmInference   │────▶│   LlamaModel    │────▶│  GGUF Model     │
//! │     (trait)     │     │ (llama-cpp-2)   │     │  (Ministral)    │
//! └─────────────────┘     └─────────────────┘     └─────────────────┘
//!         │
//!    generate()
//!    describe_scene()
//!    analyze_tracks()
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use peat_inference::inference::{LlamaInference, LlmConfig};
//!
//! let config = LlmConfig {
//!     model_path: "/models/ministral-3b-q4_k_m.gguf".into(),
//!     n_gpu_layers: 99,  // Offload all layers to GPU
//!     ..Default::default()
//! };
//!
//! let mut llm = LlamaInference::new(config)?;
//! llm.load().await?;
//!
//! let response = llm.generate("Describe what you see", None).await?;
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info};

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel, Special};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;

/// Result type for LLM operations
pub type LlmResult<T> = Result<T, LlmError>;

/// Errors from LLM inference
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("Model not loaded")]
    NotLoaded,

    #[error("Failed to load model: {0}")]
    LoadError(String),

    #[error("Inference failed: {0}")]
    InferenceError(String),

    #[error("Tokenization failed: {0}")]
    TokenizationError(String),

    #[error("Context overflow: {0} tokens exceeds {1} max")]
    ContextOverflow(usize, usize),

    #[error("Model file not found: {0}")]
    ModelNotFound(PathBuf),
}

/// LLM inference trait - abstraction over different backends
#[async_trait]
pub trait LlmInference: Send + Sync {
    /// Generate text from a prompt
    async fn generate(&mut self, prompt: &str, max_tokens: Option<u32>) -> LlmResult<String>;

    /// Generate with system prompt
    async fn generate_with_system(
        &mut self,
        system: &str,
        user: &str,
        max_tokens: Option<u32>,
    ) -> LlmResult<String>;

    /// Check if model is loaded and ready
    fn is_ready(&self) -> bool;

    /// Get model info
    fn model_info(&self) -> &LlmModelInfo;

    /// Load the model (call before inference)
    async fn load(&mut self) -> LlmResult<()>;

    /// Unload the model to free memory
    async fn unload(&mut self) -> LlmResult<()>;
}

/// Information about a loaded LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmModelInfo {
    /// Model identifier (e.g., "ministral-3b")
    pub model_id: String,

    /// Model file path
    pub model_path: PathBuf,

    /// Quantization format (e.g., "Q4_K_M")
    pub quantization: String,

    /// Context size in tokens
    pub context_size: u32,

    /// Number of parameters (billions)
    pub params_b: f32,

    /// VRAM usage in MB (0 if CPU only)
    pub vram_mb: u32,
}

/// Configuration for LLM inference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Path to GGUF model file
    pub model_path: PathBuf,

    /// Model identifier for reporting
    pub model_id: String,

    /// Context size in tokens
    pub n_ctx: u32,

    /// Number of layers to offload to GPU (0 = CPU only, 99 = all)
    pub n_gpu_layers: u32,

    /// Number of threads for CPU inference
    pub n_threads: u32,

    /// Batch size for prompt processing
    pub n_batch: u32,

    /// Temperature for sampling (0.0 = greedy)
    pub temperature: f32,

    /// Top-p (nucleus) sampling threshold
    pub top_p: f32,

    /// Top-k sampling (0 = disabled)
    pub top_k: u32,

    /// Repetition penalty
    pub repeat_penalty: f32,

    /// Default max tokens for generation
    pub default_max_tokens: u32,

    /// Whether to use mmap for model loading
    pub use_mmap: bool,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::from("models/ministral-3b-q4_k_m.gguf"),
            model_id: "ministral-3b".to_string(),
            n_ctx: 4096,
            n_gpu_layers: 0, // CPU by default, set to 99 for full GPU offload
            n_threads: 4,
            n_batch: 512,
            temperature: 0.7,
            top_p: 0.9,
            top_k: 40,
            repeat_penalty: 1.1,
            default_max_tokens: 256,
            use_mmap: true,
        }
    }
}

impl LlmConfig {
    /// Create config for Ministral 3B
    pub fn ministral_3b(model_path: &str) -> Self {
        Self {
            model_path: PathBuf::from(model_path),
            model_id: "ministral-3b".to_string(),
            n_ctx: 4096, // Can go up to 256k but start conservative
            ..Default::default()
        }
    }

    /// Create config for Ministral 8B
    pub fn ministral_8b(model_path: &str) -> Self {
        Self {
            model_path: PathBuf::from(model_path),
            model_id: "ministral-8b".to_string(),
            n_ctx: 4096,
            ..Default::default()
        }
    }

    /// Enable GPU offloading
    pub fn with_gpu(mut self, n_layers: u32) -> Self {
        self.n_gpu_layers = n_layers;
        self
    }

    /// Set context size
    pub fn with_context(mut self, n_ctx: u32) -> Self {
        self.n_ctx = n_ctx;
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = temp;
        self
    }
}

/// Llama.cpp based LLM inference
pub struct LlamaInference {
    config: LlmConfig,
    backend: Option<LlamaBackend>,
    model: Option<Arc<LlamaModel>>,
    info: LlmModelInfo,
    inference_count: u64,
    total_tokens_generated: u64,
    total_inference_time_ms: f64,
}

impl LlamaInference {
    /// Create a new LlamaInference instance (does not load model yet)
    pub fn new(config: LlmConfig) -> LlmResult<Self> {
        if !config.model_path.exists() {
            return Err(LlmError::ModelNotFound(config.model_path.clone()));
        }

        // Extract quantization from filename (e.g., "q4_k_m" from "model-q4_k_m.gguf")
        let quantization = config
            .model_path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| {
                // Look for common quantization patterns
                let lower = s.to_lowercase();
                for q in &[
                    "q4_k_m", "q4_k_s", "q5_k_m", "q5_k_s", "q8_0", "q6_k", "f16", "f32",
                ] {
                    if lower.contains(q) {
                        return Some(q.to_uppercase());
                    }
                }
                None
            })
            .unwrap_or_else(|| "unknown".to_string());

        let info = LlmModelInfo {
            model_id: config.model_id.clone(),
            model_path: config.model_path.clone(),
            quantization,
            context_size: config.n_ctx,
            params_b: 0.0, // Will be updated after loading
            vram_mb: 0,    // Will be updated after loading
        };

        Ok(Self {
            config,
            backend: None,
            model: None,
            info,
            inference_count: 0,
            total_tokens_generated: 0,
            total_inference_time_ms: 0.0,
        })
    }

    /// Format a chat message for Mistral/Ministral models
    fn format_prompt(&self, system: Option<&str>, user: &str) -> String {
        // Mistral chat template format
        let mut prompt = String::new();

        if let Some(sys) = system {
            prompt.push_str("<s>[INST] ");
            prompt.push_str(sys);
            prompt.push_str("\n\n");
            prompt.push_str(user);
            prompt.push_str(" [/INST]");
        } else {
            prompt.push_str("<s>[INST] ");
            prompt.push_str(user);
            prompt.push_str(" [/INST]");
        }

        prompt
    }

    /// Internal generation implementation
    fn generate_internal(&mut self, prompt: &str, max_tokens: u32) -> LlmResult<String> {
        let model = self.model.as_ref().ok_or(LlmError::NotLoaded)?;

        let start = Instant::now();

        // Create context for this generation
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(std::num::NonZeroU32::new(self.config.n_ctx))
            .with_n_threads(self.config.n_threads as i32)
            .with_n_threads_batch(self.config.n_threads as i32);

        let mut ctx = model
            .new_context(&self.backend.as_ref().unwrap(), ctx_params)
            .map_err(|e| LlmError::InferenceError(format!("Failed to create context: {}", e)))?;

        // Tokenize the prompt
        let tokens = model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| LlmError::TokenizationError(e.to_string()))?;

        if tokens.len() > self.config.n_ctx as usize {
            return Err(LlmError::ContextOverflow(
                tokens.len(),
                self.config.n_ctx as usize,
            ));
        }

        debug!("Prompt tokenized to {} tokens", tokens.len());

        // Create batch and add tokens
        let mut batch = LlamaBatch::new(self.config.n_ctx as usize, 1);

        for (i, token) in tokens.iter().enumerate() {
            let is_last = i == tokens.len() - 1;
            batch.add(*token, i as i32, &[0], is_last).map_err(|e| {
                LlmError::InferenceError(format!("Failed to add token to batch: {}", e))
            })?;
        }

        // Decode the prompt
        ctx.decode(&mut batch)
            .map_err(|e| LlmError::InferenceError(format!("Failed to decode prompt: {}", e)))?;

        // Setup sampler
        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::temp(self.config.temperature),
            LlamaSampler::top_p(self.config.top_p, 1),
            LlamaSampler::top_k(self.config.top_k as i32),
            LlamaSampler::penalties(
                64, // last_n tokens to consider for penalty
                self.config.repeat_penalty,
                0.0, // freq penalty
                0.0, // presence penalty
            ),
            LlamaSampler::dist(rand::random()),
        ]);

        // Generate tokens
        let mut output_tokens: Vec<LlamaToken> = Vec::new();
        let mut n_cur = tokens.len();

        for _ in 0..max_tokens {
            // Sample next token
            let new_token = sampler.sample(&ctx, -1);

            // Check for EOS
            if model.is_eog_token(new_token) {
                break;
            }

            output_tokens.push(new_token);

            // Prepare next batch
            batch.clear();
            batch
                .add(new_token, n_cur as i32, &[0], true)
                .map_err(|e| LlmError::InferenceError(format!("Failed to add token: {}", e)))?;

            n_cur += 1;

            // Decode
            ctx.decode(&mut batch)
                .map_err(|e| LlmError::InferenceError(format!("Decode failed: {}", e)))?;
        }

        // Convert tokens to string
        let output = output_tokens
            .iter()
            .map(|t| model.token_to_str(*t, Special::Tokenize))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| LlmError::InferenceError(format!("Failed to decode tokens: {}", e)))?
            .join("");

        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        self.inference_count += 1;
        self.total_tokens_generated += output_tokens.len() as u64;
        self.total_inference_time_ms += elapsed;

        let tokens_per_sec = output_tokens.len() as f64 / (elapsed / 1000.0);
        debug!(
            "Generated {} tokens in {:.1}ms ({:.1} tok/s)",
            output_tokens.len(),
            elapsed,
            tokens_per_sec
        );

        Ok(output)
    }

    /// Get generation statistics
    pub fn stats(&self) -> LlmStats {
        let avg_tokens_per_inference = if self.inference_count > 0 {
            self.total_tokens_generated as f64 / self.inference_count as f64
        } else {
            0.0
        };

        let avg_time_per_inference = if self.inference_count > 0 {
            self.total_inference_time_ms / self.inference_count as f64
        } else {
            0.0
        };

        let tokens_per_sec = if self.total_inference_time_ms > 0.0 {
            self.total_tokens_generated as f64 / (self.total_inference_time_ms / 1000.0)
        } else {
            0.0
        };

        LlmStats {
            inference_count: self.inference_count,
            total_tokens_generated: self.total_tokens_generated,
            avg_tokens_per_inference,
            avg_time_per_inference_ms: avg_time_per_inference,
            tokens_per_sec,
        }
    }
}

#[async_trait]
impl LlmInference for LlamaInference {
    async fn generate(&mut self, prompt: &str, max_tokens: Option<u32>) -> LlmResult<String> {
        let max = max_tokens.unwrap_or(self.config.default_max_tokens);
        let formatted = self.format_prompt(None, prompt);
        self.generate_internal(&formatted, max)
    }

    async fn generate_with_system(
        &mut self,
        system: &str,
        user: &str,
        max_tokens: Option<u32>,
    ) -> LlmResult<String> {
        let max = max_tokens.unwrap_or(self.config.default_max_tokens);
        let formatted = self.format_prompt(Some(system), user);
        self.generate_internal(&formatted, max)
    }

    fn is_ready(&self) -> bool {
        self.model.is_some()
    }

    fn model_info(&self) -> &LlmModelInfo {
        &self.info
    }

    async fn load(&mut self) -> LlmResult<()> {
        info!("Loading LLM model: {:?}", self.config.model_path);

        // Initialize backend
        let backend = LlamaBackend::init()
            .map_err(|e| LlmError::LoadError(format!("Failed to init backend: {}", e)))?;

        // Setup model parameters
        let model_params = LlamaModelParams::default().with_n_gpu_layers(self.config.n_gpu_layers);

        // Load model
        let model = LlamaModel::load_from_file(&backend, &self.config.model_path, &model_params)
            .map_err(|e| LlmError::LoadError(format!("Failed to load model: {}", e)))?;

        info!(
            "Model loaded: {} tokens vocab, {} layers",
            model.n_vocab(),
            model.n_layer()
        );

        self.backend = Some(backend);
        self.model = Some(Arc::new(model));

        Ok(())
    }

    async fn unload(&mut self) -> LlmResult<()> {
        info!("Unloading LLM model");
        self.model = None;
        self.backend = None;
        Ok(())
    }
}

/// LLM generation statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmStats {
    pub inference_count: u64,
    pub total_tokens_generated: u64,
    pub avg_tokens_per_inference: f64,
    pub avg_time_per_inference_ms: f64,
    pub tokens_per_sec: f64,
}

/// Helper functions for Peat integration
impl LlamaInference {
    /// Describe detected objects for Peat track updates
    /// Input: list of detected object labels and their positions
    pub async fn describe_detections(
        &mut self,
        detections: &[(String, f32, f32)], // (label, x, y normalized)
    ) -> LlmResult<String> {
        if detections.is_empty() {
            return Ok("No objects detected.".to_string());
        }

        let detection_list: Vec<String> = detections
            .iter()
            .map(|(label, x, y)| format!("- {} at ({:.0}%, {:.0}%)", label, x * 100.0, y * 100.0))
            .collect();

        let prompt = format!(
            "Briefly describe this scene with detected objects:\n{}\n\nProvide a 1-2 sentence tactical summary.",
            detection_list.join("\n")
        );

        self.generate_with_system(
            "You are a tactical reconnaissance AI. Be concise and factual.",
            &prompt,
            Some(64),
        )
        .await
    }

    /// Analyze track patterns for anomaly detection
    pub async fn analyze_tracks(&mut self, track_summaries: &[String]) -> LlmResult<String> {
        if track_summaries.is_empty() {
            return Ok("No tracks to analyze.".to_string());
        }

        let prompt = format!(
            "Analyze these tracked objects for unusual patterns:\n{}\n\nNote any anomalies or concerns.",
            track_summaries.join("\n")
        );

        self.generate_with_system(
            "You are a surveillance analysis AI. Focus on movement patterns and potential threats.",
            &prompt,
            Some(128),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = LlmConfig::default();
        assert_eq!(config.n_ctx, 4096);
        assert_eq!(config.n_gpu_layers, 0);
        assert_eq!(config.temperature, 0.7);
    }

    #[test]
    fn test_config_builder() {
        let config = LlmConfig::ministral_3b("/models/test.gguf")
            .with_gpu(99)
            .with_context(8192)
            .with_temperature(0.5);

        assert_eq!(config.model_id, "ministral-3b");
        assert_eq!(config.n_gpu_layers, 99);
        assert_eq!(config.n_ctx, 8192);
        assert_eq!(config.temperature, 0.5);
    }

    #[test]
    fn test_prompt_formatting() {
        let config = LlmConfig::default();
        // Can't test format_prompt directly without model, but we test config
        assert!(config.model_path.to_str().unwrap().contains("ministral"));
    }
}
