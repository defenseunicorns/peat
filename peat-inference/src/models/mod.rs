//! Model distribution and management for PEAT
//!
//! This module re-exports types from `peat_protocol::distribution` for AI model
//! distribution across the PEAT network.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use peat_inference::models::{ModelManifest, ModelType, Quantization};
//!
//! // Announce a new model available for download
//! let manifest = ModelManifest::new(
//!     "ministral-3b",
//!     "Ministral 3B Instruct",
//!     ModelType::Llm,
//! )
//! .with_version("25.12")
//! .with_quantization(Quantization::Q4_K_M)
//! .with_blob_hash("bafkreihdwdcef...")
//! .with_size_bytes(2_000_000_000);
//!
//! // Broadcast manifest to PEAT network
//! peat.broadcast_model_manifest(&manifest).await?;
//! ```

// Re-export from peat-protocol
pub use peat_protocol::distribution::{
    HardwareRequirements, LocalModelState, ModelFormat, ModelManifest, ModelStatus, ModelType,
    ModelUpdateCommand, Quantization,
};
