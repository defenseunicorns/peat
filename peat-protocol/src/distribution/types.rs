//! Core types for AI model distribution

use serde::{Deserialize, Serialize};

/// Type of AI model
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelType {
    /// Object detection model (YOLO, etc.)
    Detector,
    /// Large language model (Ministral, Llama, Phi, etc.)
    Llm,
    /// Object tracking / re-identification model
    Tracker,
    /// Feature embedding model
    Embedder,
    /// Vision-language model
    Vlm,
    /// Audio transcription model (Whisper, etc.)
    Whisper,
    /// Custom/other model type
    Custom,
}

impl ModelType {
    /// Get a human-readable name
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Detector => "Object Detector",
            Self::Llm => "Language Model",
            Self::Tracker => "Tracker",
            Self::Embedder => "Embedder",
            Self::Vlm => "Vision-Language Model",
            Self::Whisper => "Audio Transcription",
            Self::Custom => "Custom Model",
        }
    }

    /// Get capability type string (for CapabilityInfo compatibility)
    pub fn capability_type(&self) -> &'static str {
        match self {
            Self::Detector => "OBJECT_DETECTION",
            Self::Llm => "LLM_INFERENCE",
            Self::Tracker => "OBJECT_TRACKING",
            Self::Embedder => "FEATURE_EMBEDDING",
            Self::Vlm => "VISION_LANGUAGE",
            Self::Whisper => "AUDIO_TRANSCRIPTION",
            Self::Custom => "CUSTOM",
        }
    }
}

/// Model file format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelFormat {
    /// ONNX format (cross-platform)
    Onnx,
    /// GGUF format (llama.cpp quantized models)
    Gguf,
    /// TensorRT engine (NVIDIA optimized)
    TensorRT,
    /// PyTorch format
    PyTorch,
    /// SafeTensors format
    SafeTensors,
}

impl ModelFormat {
    /// File extension for this format
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Onnx => "onnx",
            Self::Gguf => "gguf",
            Self::TensorRT => "engine",
            Self::PyTorch => "pt",
            Self::SafeTensors => "safetensors",
        }
    }

    /// MIME type for this format
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Onnx => "application/onnx",
            Self::Gguf => "application/octet-stream",
            Self::TensorRT => "application/octet-stream",
            Self::PyTorch => "application/octet-stream",
            Self::SafeTensors => "application/octet-stream",
        }
    }
}

/// Quantization level for model weights
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[allow(non_camel_case_types)] // Keep standard quantization naming
pub enum Quantization {
    /// Full precision (FP32)
    F32,
    /// Half precision (FP16)
    F16,
    /// Brain float 16
    BF16,
    /// 8-bit integer
    INT8,
    /// 4-bit (Q4_0 - legacy)
    Q4_0,
    /// 4-bit K-quant small
    Q4_K_S,
    /// 4-bit K-quant medium (good balance of size/quality)
    Q4_K_M,
    /// 5-bit K-quant small
    Q5_K_S,
    /// 5-bit K-quant medium
    Q5_K_M,
    /// 6-bit K-quant
    Q6_K,
    /// 8-bit (Q8_0)
    Q8_0,
}

impl Quantization {
    /// Get display string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::F32 => "F32",
            Self::F16 => "F16",
            Self::BF16 => "BF16",
            Self::INT8 => "INT8",
            Self::Q4_0 => "Q4_0",
            Self::Q4_K_S => "Q4_K_S",
            Self::Q4_K_M => "Q4_K_M",
            Self::Q5_K_S => "Q5_K_S",
            Self::Q5_K_M => "Q5_K_M",
            Self::Q6_K => "Q6_K",
            Self::Q8_0 => "Q8_0",
        }
    }

    /// Approximate memory multiplier vs FP16 (lower = smaller)
    pub fn memory_factor(&self) -> f32 {
        match self {
            Self::F32 => 2.0,
            Self::F16 | Self::BF16 => 1.0,
            Self::INT8 | Self::Q8_0 => 0.5,
            Self::Q6_K => 0.41,
            Self::Q5_K_S | Self::Q5_K_M => 0.35,
            Self::Q4_0 | Self::Q4_K_S | Self::Q4_K_M => 0.28,
        }
    }

    /// Parse from filename component (e.g., "q4_k_m" -> Q4_K_M)
    pub fn from_filename(s: &str) -> Option<Self> {
        let lower = s.to_lowercase();
        match lower.as_str() {
            "f32" | "fp32" => Some(Self::F32),
            "f16" | "fp16" => Some(Self::F16),
            "bf16" => Some(Self::BF16),
            "int8" => Some(Self::INT8),
            "q4_0" => Some(Self::Q4_0),
            "q4_k_s" => Some(Self::Q4_K_S),
            "q4_k_m" => Some(Self::Q4_K_M),
            "q5_k_s" => Some(Self::Q5_K_S),
            "q5_k_m" => Some(Self::Q5_K_M),
            "q6_k" => Some(Self::Q6_K),
            "q8_0" => Some(Self::Q8_0),
            _ => None,
        }
    }
}

impl std::fmt::Display for Quantization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_type_capability() {
        assert_eq!(ModelType::Detector.capability_type(), "OBJECT_DETECTION");
        assert_eq!(ModelType::Llm.capability_type(), "LLM_INFERENCE");
    }

    #[test]
    fn test_model_format_extension() {
        assert_eq!(ModelFormat::Onnx.extension(), "onnx");
        assert_eq!(ModelFormat::Gguf.extension(), "gguf");
        assert_eq!(ModelFormat::TensorRT.extension(), "engine");
    }

    #[test]
    fn test_quantization_memory_factor() {
        assert!(Quantization::Q4_K_M.memory_factor() < Quantization::F16.memory_factor());
        assert!(Quantization::Q8_0.memory_factor() < Quantization::F16.memory_factor());
        assert!(Quantization::Q4_K_M.memory_factor() < Quantization::Q8_0.memory_factor());
    }

    #[test]
    fn test_quantization_from_filename() {
        assert_eq!(
            Quantization::from_filename("q4_k_m"),
            Some(Quantization::Q4_K_M)
        );
        assert_eq!(
            Quantization::from_filename("Q4_K_M"),
            Some(Quantization::Q4_K_M)
        );
        assert_eq!(Quantization::from_filename("fp16"), Some(Quantization::F16));
        assert_eq!(Quantization::from_filename("unknown"), None);
    }
}
