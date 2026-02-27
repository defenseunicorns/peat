//! Software Distribution for PEAT - ADR-012 / ADR-026
//!
//! This module provides types for distributing software across the PEAT network:
//!
//! - **Deployment Directives**: C2→node commands for software deployment
//! - **Model Manifest**: Metadata for downloadable models (hash, size, requirements)
//! - **Model Types**: Detection, LLM, Tracking, Vision-Language, etc.
//! - **Hardware Requirements**: VRAM, RAM, architecture constraints
//!
//! ## Distribution Flow
//!
//! ```text
//! ┌─────────────────┐                      ┌─────────────────┐
//! │   C2 / Bridge   │──DeploymentDirective─│   Edge Node     │
//! │                 │─────────────────────▶│                 │
//! │                 │                      │  fetch blob     │
//! │                 │                      │  activate       │
//! │                 │◀─────────────────────│                 │
//! │                 │   DeploymentStatus   │  advertise cap  │
//! └─────────────────┘                      └─────────────────┘
//! ```
//!
//! ## Artifact Types
//!
//! - **ONNX Models**: AI inference via ONNX Runtime
//! - **Containers**: Docker/Podman images
//! - **Native Binaries**: Architecture-specific executables
//! - **Config Packages**: Configuration files/bundles
//! - **WASM Modules**: WebAssembly (future)
//!
//! ## Integration with Capabilities
//!
//! When a node activates an artifact, it advertises the corresponding capability:
//! - ONNX detector → `CapabilityType::Sensor` (object detection)
//! - LLM model → `CapabilityType::Compute` (LLM inference)
//! - etc.

mod directive;
mod manifest;
mod types;

// Deployment directives (ADR-012)
pub use directive::{
    ArtifactSpec, ArtifactType, CapabilityFilter, ContainerRuntime, DeploymentDirective,
    DeploymentOptions, DeploymentPriority, DeploymentScope, DeploymentState, DeploymentStatus,
    PortMapping,
};

// Model-specific types
pub use manifest::{
    HardwareRequirements, LocalModelState, ModelManifest, ModelStatus, ModelUpdateCommand,
};
pub use types::{ModelFormat, ModelType, Quantization};
