//! Model manifest and distribution commands

use super::types::{ModelFormat, ModelType, Quantization};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Hardware requirements for running a model
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardwareRequirements {
    /// Minimum VRAM in MB (0 for CPU-only capable)
    pub min_vram_mb: u32,
    /// Recommended VRAM in MB for optimal performance
    pub recommended_vram_mb: u32,
    /// Minimum RAM in MB
    pub min_ram_mb: u32,
    /// Supported execution providers (e.g., "cpu", "cuda", "tensorrt")
    pub execution_providers: Vec<String>,
    /// Target architectures (e.g., "aarch64", "x86_64")
    pub architectures: Vec<String>,
}

impl Default for HardwareRequirements {
    fn default() -> Self {
        Self {
            min_vram_mb: 0,
            recommended_vram_mb: 0,
            min_ram_mb: 2048,
            execution_providers: vec!["cpu".to_string()],
            architectures: vec!["aarch64".to_string(), "x86_64".to_string()],
        }
    }
}

impl HardwareRequirements {
    /// Requirements for a small LLM (3B Q4)
    pub fn small_llm() -> Self {
        Self {
            min_vram_mb: 2048,
            recommended_vram_mb: 4096,
            min_ram_mb: 4096,
            execution_providers: vec!["cuda".to_string(), "cpu".to_string()],
            architectures: vec!["aarch64".to_string(), "x86_64".to_string()],
        }
    }

    /// Requirements for a medium LLM (8B Q4)
    pub fn medium_llm() -> Self {
        Self {
            min_vram_mb: 4096,
            recommended_vram_mb: 8192,
            min_ram_mb: 8192,
            execution_providers: vec!["cuda".to_string(), "cpu".to_string()],
            architectures: vec!["aarch64".to_string(), "x86_64".to_string()],
        }
    }

    /// Requirements for YOLOv8 nano detector
    pub fn yolo_nano() -> Self {
        Self {
            min_vram_mb: 512,
            recommended_vram_mb: 1024,
            min_ram_mb: 1024,
            execution_providers: vec![
                "tensorrt".to_string(),
                "cuda".to_string(),
                "cpu".to_string(),
            ],
            architectures: vec!["aarch64".to_string(), "x86_64".to_string()],
        }
    }

    /// Check if a node meets the requirements
    pub fn can_run_on(&self, available_vram_mb: u32, available_ram_mb: u32, arch: &str) -> bool {
        let vram_ok = available_vram_mb >= self.min_vram_mb || self.min_vram_mb == 0;
        let ram_ok = available_ram_mb >= self.min_ram_mb;
        let arch_ok = self.architectures.iter().any(|a| a == arch);
        vram_ok && ram_ok && arch_ok
    }

    /// Check if a specific execution provider is supported
    pub fn supports_provider(&self, provider: &str) -> bool {
        self.execution_providers.iter().any(|p| p == provider)
    }
}

/// Model manifest containing all metadata for distribution
///
/// This is the primary type for announcing and distributing models across Peat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelManifest {
    /// Unique model identifier (e.g., "ministral-3b-q4km")
    pub model_id: String,

    /// Human-readable name
    pub name: String,

    /// Model type (detector, LLM, etc.)
    pub model_type: ModelType,

    /// Model file format
    pub format: ModelFormat,

    /// Version string (semver or date-based)
    pub version: String,

    /// Quantization level
    pub quantization: Quantization,

    /// Model size in bytes
    pub size_bytes: u64,

    /// SHA-256 hash for verification
    pub sha256: String,

    /// iroh-blobs content hash (for P2P distribution)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob_hash: Option<String>,

    /// Direct download URL (fallback if P2P unavailable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_url: Option<String>,

    /// Hardware requirements
    pub requirements: HardwareRequirements,

    /// Model capabilities/features (e.g., "chat", "vision", "function_calling")
    pub features: Vec<String>,

    /// Number of parameters in billions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params_billions: Option<f32>,

    /// Context length in tokens (for LLMs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u32>,

    /// Supported classes (for detectors)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classes: Option<Vec<String>>,

    /// License identifier (SPDX format)
    pub license: String,

    /// Source/attribution
    pub source: String,

    /// When this manifest was created
    pub created_at: DateTime<Utc>,

    /// Additional metadata
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ModelManifest {
    /// Create a new model manifest
    pub fn new(
        model_id: impl Into<String>,
        name: impl Into<String>,
        model_type: ModelType,
    ) -> Self {
        Self {
            model_id: model_id.into(),
            name: name.into(),
            model_type,
            format: ModelFormat::Gguf,
            version: "1.0.0".to_string(),
            quantization: Quantization::Q4_K_M,
            size_bytes: 0,
            sha256: String::new(),
            blob_hash: None,
            download_url: None,
            requirements: HardwareRequirements::default(),
            features: Vec::new(),
            params_billions: None,
            context_length: None,
            classes: None,
            license: "Apache-2.0".to_string(),
            source: String::new(),
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Create manifest for Ministral 3B
    pub fn ministral_3b(quantization: Quantization) -> Self {
        Self::new("ministral-3b", "Ministral 3B Instruct", ModelType::Llm)
            .with_version("25.12")
            .with_format(ModelFormat::Gguf)
            .with_quantization(quantization)
            .with_params(3.0)
            .with_context_length(256_000)
            .with_requirements(HardwareRequirements::small_llm())
            .with_source("Mistral AI")
            .with_license("Apache-2.0")
            .with_feature("chat")
            .with_feature("function_calling")
            .with_feature("vision")
    }

    /// Create manifest for Ministral 8B
    pub fn ministral_8b(quantization: Quantization) -> Self {
        Self::new("ministral-8b", "Ministral 8B Instruct", ModelType::Llm)
            .with_version("25.12")
            .with_format(ModelFormat::Gguf)
            .with_quantization(quantization)
            .with_params(8.0)
            .with_context_length(256_000)
            .with_requirements(HardwareRequirements::medium_llm())
            .with_source("Mistral AI")
            .with_license("Apache-2.0")
            .with_feature("chat")
            .with_feature("function_calling")
            .with_feature("vision")
    }

    /// Create manifest for YOLOv8n
    pub fn yolov8n() -> Self {
        Self::new("yolov8n", "YOLOv8 Nano", ModelType::Detector)
            .with_version("8.0.0")
            .with_format(ModelFormat::Onnx)
            .with_quantization(Quantization::F16)
            .with_requirements(HardwareRequirements::yolo_nano())
            .with_source("Ultralytics")
            .with_license("AGPL-3.0")
            .with_feature("coco_80")
    }

    // Builder methods

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    pub fn with_format(mut self, format: ModelFormat) -> Self {
        self.format = format;
        self
    }

    pub fn with_quantization(mut self, quantization: Quantization) -> Self {
        self.quantization = quantization;
        self
    }

    pub fn with_size_bytes(mut self, size: u64) -> Self {
        self.size_bytes = size;
        self
    }

    pub fn with_sha256(mut self, hash: impl Into<String>) -> Self {
        self.sha256 = hash.into();
        self
    }

    pub fn with_blob_hash(mut self, hash: impl Into<String>) -> Self {
        self.blob_hash = Some(hash.into());
        self
    }

    pub fn with_download_url(mut self, url: impl Into<String>) -> Self {
        self.download_url = Some(url.into());
        self
    }

    pub fn with_requirements(mut self, requirements: HardwareRequirements) -> Self {
        self.requirements = requirements;
        self
    }

    pub fn with_feature(mut self, feature: impl Into<String>) -> Self {
        self.features.push(feature.into());
        self
    }

    pub fn with_params(mut self, billions: f32) -> Self {
        self.params_billions = Some(billions);
        self
    }

    pub fn with_context_length(mut self, length: u32) -> Self {
        self.context_length = Some(length);
        self
    }

    pub fn with_classes(mut self, classes: Vec<String>) -> Self {
        self.classes = Some(classes);
        self
    }

    pub fn with_license(mut self, license: impl Into<String>) -> Self {
        self.license = license.into();
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Get estimated VRAM usage in MB based on parameters and quantization
    pub fn estimated_vram_mb(&self) -> u32 {
        if let Some(params) = self.params_billions {
            // Rough estimate: params * 2 bytes (FP16) * quantization factor * overhead
            let base_mb = (params * 2.0 * 1024.0) as u32;
            (base_mb as f32 * self.quantization.memory_factor() * 1.2) as u32
        } else {
            self.requirements.recommended_vram_mb
        }
    }

    /// Generate a filename for this model
    pub fn filename(&self) -> String {
        format!(
            "{}-{}-{}.{}",
            self.model_id,
            self.version.replace('.', "_"),
            self.quantization.as_str().to_lowercase(),
            self.format.extension()
        )
    }

    /// Check if this model can run on given hardware
    pub fn can_run_on(&self, vram_mb: u32, ram_mb: u32, arch: &str) -> bool {
        self.requirements.can_run_on(vram_mb, ram_mb, arch)
    }
}

/// Model download/deployment status on a node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelStatus {
    /// Model manifest received, not yet downloaded
    Available,
    /// Model is being downloaded
    Downloading,
    /// Model downloaded and hash verified
    Ready,
    /// Model loaded into memory/GPU
    Loaded,
    /// Download or verification failed
    Failed,
}

/// Local state of a model on a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModelState {
    /// Model ID from manifest
    pub model_id: String,
    /// Current status
    pub status: ModelStatus,
    /// Local file path (if downloaded)
    pub local_path: Option<PathBuf>,
    /// Download progress (0.0 - 1.0)
    pub download_progress: f32,
    /// Last verification timestamp
    pub verified_at: Option<DateTime<Utc>>,
    /// Error message (if status is Failed)
    pub error: Option<String>,
}

impl LocalModelState {
    /// Create new state for an available model
    pub fn available(model_id: impl Into<String>) -> Self {
        Self {
            model_id: model_id.into(),
            status: ModelStatus::Available,
            local_path: None,
            download_progress: 0.0,
            verified_at: None,
            error: None,
        }
    }

    /// Update to downloading state
    pub fn downloading(mut self, progress: f32) -> Self {
        self.status = ModelStatus::Downloading;
        self.download_progress = progress.clamp(0.0, 1.0);
        self
    }

    /// Update to ready state
    pub fn ready(mut self, path: PathBuf) -> Self {
        self.status = ModelStatus::Ready;
        self.local_path = Some(path);
        self.download_progress = 1.0;
        self.verified_at = Some(Utc::now());
        self
    }

    /// Update to loaded state
    pub fn loaded(mut self) -> Self {
        self.status = ModelStatus::Loaded;
        self
    }

    /// Update to failed state
    pub fn failed(mut self, error: impl Into<String>) -> Self {
        self.status = ModelStatus::Failed;
        self.error = Some(error.into());
        self
    }
}

/// Command to push a model update to nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUpdateCommand {
    /// Unique command ID
    pub command_id: String,
    /// Model manifest
    pub manifest: ModelManifest,
    /// Target node IDs (empty = broadcast to all capable nodes)
    pub target_nodes: Vec<String>,
    /// Priority (1-5, 1 = highest)
    pub priority: u8,
    /// Whether to auto-load after download
    pub auto_load: bool,
    /// Model ID to rollback to if update fails
    pub rollback_model_id: Option<String>,
    /// Command timestamp
    pub timestamp: DateTime<Utc>,
}

impl ModelUpdateCommand {
    /// Create a new model update command
    pub fn new(manifest: ModelManifest) -> Self {
        Self {
            command_id: uuid::Uuid::new_v4().to_string(),
            manifest,
            target_nodes: Vec::new(),
            priority: 3,
            auto_load: true,
            rollback_model_id: None,
            timestamp: Utc::now(),
        }
    }

    /// Target specific nodes
    pub fn with_targets(mut self, nodes: Vec<String>) -> Self {
        self.target_nodes = nodes;
        self
    }

    /// Set priority (1 = highest, 5 = lowest)
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority.clamp(1, 5);
        self
    }

    /// Set rollback model
    pub fn with_rollback(mut self, model_id: impl Into<String>) -> Self {
        self.rollback_model_id = Some(model_id.into());
        self
    }

    /// Disable auto-load after download
    pub fn without_auto_load(mut self) -> Self {
        self.auto_load = false;
        self
    }

    /// Check if this command targets a specific node
    pub fn targets_node(&self, node_id: &str) -> bool {
        self.target_nodes.is_empty() || self.target_nodes.iter().any(|n| n == node_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_ministral() {
        let manifest = ModelManifest::ministral_3b(Quantization::Q4_K_M);

        assert_eq!(manifest.model_id, "ministral-3b");
        assert_eq!(manifest.model_type, ModelType::Llm);
        assert_eq!(manifest.format, ModelFormat::Gguf);
        assert_eq!(manifest.quantization, Quantization::Q4_K_M);
        assert_eq!(manifest.context_length, Some(256_000));
        assert!(manifest.features.contains(&"chat".to_string()));
    }

    #[test]
    fn test_manifest_yolo() {
        let manifest = ModelManifest::yolov8n();

        assert_eq!(manifest.model_id, "yolov8n");
        assert_eq!(manifest.model_type, ModelType::Detector);
        assert_eq!(manifest.format, ModelFormat::Onnx);
    }

    #[test]
    fn test_filename_generation() {
        let manifest = ModelManifest::ministral_3b(Quantization::Q4_K_M).with_version("25.12");

        assert_eq!(manifest.filename(), "ministral-3b-25_12-q4_k_m.gguf");
    }

    #[test]
    fn test_hardware_requirements() {
        let reqs = HardwareRequirements::small_llm();

        // Jetson Orin Nano (8GB, ~4GB available)
        assert!(reqs.can_run_on(4096, 8192, "aarch64"));

        // Low-end device
        assert!(!reqs.can_run_on(512, 2048, "aarch64"));

        // Wrong architecture
        assert!(!reqs.can_run_on(4096, 8192, "armv7"));
    }

    #[test]
    fn test_update_command_targeting() {
        let manifest = ModelManifest::ministral_3b(Quantization::Q4_K_M);
        let cmd = ModelUpdateCommand::new(manifest);

        // Empty targets = broadcast
        assert!(cmd.targets_node("any-node"));

        let cmd = cmd.with_targets(vec!["node-1".to_string(), "node-2".to_string()]);
        assert!(cmd.targets_node("node-1"));
        assert!(cmd.targets_node("node-2"));
        assert!(!cmd.targets_node("node-3"));
    }

    #[test]
    fn test_local_model_state_transitions() {
        let state = LocalModelState::available("ministral-3b");
        assert_eq!(state.status, ModelStatus::Available);

        let state = state.downloading(0.5);
        assert_eq!(state.status, ModelStatus::Downloading);
        assert_eq!(state.download_progress, 0.5);

        let state = state.ready(PathBuf::from("/models/ministral-3b.gguf"));
        assert_eq!(state.status, ModelStatus::Ready);
        assert!(state.verified_at.is_some());

        let state = state.loaded();
        assert_eq!(state.status, ModelStatus::Loaded);
    }
}
