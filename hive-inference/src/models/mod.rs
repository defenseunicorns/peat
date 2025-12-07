//! Model distribution and management for HIVE
//!
//! This module re-exports types from `hive_protocol::distribution` for AI model
//! distribution across the HIVE network.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use hive_inference::models::{ModelManifest, ModelType, Quantization};
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
//! // Broadcast manifest to HIVE network
//! hive.broadcast_model_manifest(&manifest).await?;
//! ```

// Re-export from hive-protocol
pub use hive_protocol::distribution::{
    HardwareRequirements, LocalModelState, ModelFormat, ModelManifest, ModelStatus, ModelType,
    ModelUpdateCommand, Quantization,
};
