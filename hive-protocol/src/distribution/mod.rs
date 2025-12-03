//! AI Model Distribution for HIVE
//!
//! This module provides types for distributing AI models across the HIVE network:
//!
//! - **Model Manifest**: Metadata for downloadable models (hash, size, requirements)
//! - **Model Types**: Detection, LLM, Tracking, Vision-Language, etc.
//! - **Hardware Requirements**: VRAM, RAM, architecture constraints
//! - **Update Commands**: C2→node model push with rollback support
//!
//! ## Distribution Flow
//!
//! ```text
//! ┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
//! │   C2 / Bridge   │────▶│  iroh-blobs     │────▶│   Edge Node     │
//! │                 │     │  (content hash) │     │   (download)    │
//! └─────────────────┘     └─────────────────┘     └─────────────────┘
//!         │                                               │
//!    ModelUpdateCommand                             verify + load
//!    (announces new model)                          (advertise capability)
//! ```
//!
//! ## Integration with Capabilities
//!
//! When a node loads a model, it advertises the corresponding capability:
//! - `ModelType::Detector` → `CapabilityType::Sensor` (object detection)
//! - `ModelType::Llm` → `CapabilityType::Compute` (LLM inference)
//! - etc.

mod manifest;
mod types;

pub use manifest::{
    HardwareRequirements, LocalModelState, ModelManifest, ModelStatus, ModelUpdateCommand,
};
pub use types::{ModelFormat, ModelType, Quantization};
