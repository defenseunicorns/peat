//! Model Registry - Runtime tracking of loaded AI models
//!
//! Implements Issue #107 Phase 2: Runtime Model Registry
//!
//! The ModelRegistry tracks:
//! - Loaded models per platform
//! - Lifecycle events (load, unload, update)
//! - Performance monitoring and degradation detection
//! - Integration with beacon broadcasting
//!
//! ## Usage
//!
//! ```rust,ignore
//! use hive_inference::registry::{ModelRegistry, ModelQuery};
//!
//! // Create registry
//! let mut registry = ModelRegistry::new("platform-01");
//!
//! // Register a model
//! let model_cap = ModelCapability::new(...);
//! registry.register_model(model_cap)?;
//!
//! // Query models
//! let query = ModelQuery::new()
//!     .with_model_type("detector")
//!     .with_min_version("1.2.0")
//!     .with_min_precision(0.9);
//! let matches = registry.query(&query);
//! ```

use crate::messages::{
    CapabilityAdvertisement, ModelCapability, ModelPerformance, OperationalStatus,
    ResourceMetrics,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tracing::{debug, info, warn};

/// Errors that can occur in the model registry
#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Model already registered: {0}")]
    ModelAlreadyExists(String),

    #[error("Invalid model state transition: {from:?} -> {to:?}")]
    InvalidStateTransition {
        from: OperationalStatus,
        to: OperationalStatus,
    },

    #[error("Version conflict: current={current}, requested={requested}")]
    VersionConflict { current: String, requested: String },
}

/// Model lifecycle events for tracking and auditing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelEvent {
    /// Event type
    pub event_type: ModelEventType,
    /// Model identifier
    pub model_id: String,
    /// Model version
    pub model_version: String,
    /// Platform ID where event occurred
    pub platform_id: String,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Optional event details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    /// Previous status (for transitions)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_status: Option<OperationalStatus>,
    /// New status (for transitions)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_status: Option<OperationalStatus>,
}

/// Types of model lifecycle events
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModelEventType {
    /// Model registered in registry
    Registered,
    /// Model loaded into memory
    Loaded,
    /// Model unloaded from memory
    Unloaded,
    /// Model update started
    UpdateStarted,
    /// Model update completed
    UpdateCompleted,
    /// Model update failed
    UpdateFailed,
    /// Model status changed
    StatusChanged,
    /// Performance degradation detected
    DegradationDetected,
    /// Performance recovered
    DegradationCleared,
    /// Model failed
    Failed,
}

/// Performance baseline for degradation detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceBaseline {
    /// Baseline FPS
    pub fps: f64,
    /// Baseline latency in ms
    pub latency_ms: f64,
    /// Threshold for degradation detection (percentage drop)
    pub degradation_threshold: f64,
    /// Number of samples used to establish baseline
    pub sample_count: usize,
    /// When baseline was established
    pub established_at: DateTime<Utc>,
}

impl PerformanceBaseline {
    /// Create a new baseline from performance metrics
    pub fn from_performance(perf: &ModelPerformance) -> Self {
        Self {
            fps: perf.fps,
            latency_ms: perf.latency_ms.unwrap_or(0.0),
            degradation_threshold: 0.2, // 20% degradation threshold
            sample_count: 1,
            established_at: Utc::now(),
        }
    }

    /// Check if current performance is degraded vs baseline
    pub fn is_degraded(&self, current: &ModelPerformance) -> Option<String> {
        // Check FPS degradation
        if self.fps > 0.0 {
            let fps_drop = (self.fps - current.fps) / self.fps;
            if fps_drop > self.degradation_threshold {
                return Some(format!(
                    "FPS dropped {:.1}% (baseline: {:.1}, current: {:.1})",
                    fps_drop * 100.0,
                    self.fps,
                    current.fps
                ));
            }
        }

        // Check latency degradation
        if let Some(current_latency) = current.latency_ms {
            if self.latency_ms > 0.0 {
                let latency_increase = (current_latency - self.latency_ms) / self.latency_ms;
                if latency_increase > self.degradation_threshold {
                    return Some(format!(
                        "Latency increased {:.1}% (baseline: {:.1}ms, current: {:.1}ms)",
                        latency_increase * 100.0,
                        self.latency_ms,
                        current_latency
                    ));
                }
            }
        }

        None
    }
}

/// A registered model entry in the registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredModel {
    /// Model capability information
    pub capability: ModelCapability,
    /// Performance baseline for degradation detection
    pub baseline: Option<PerformanceBaseline>,
    /// When the model was registered
    pub registered_at: DateTime<Utc>,
    /// Recent events for this model
    pub recent_events: Vec<ModelEvent>,
    /// Maximum events to retain
    #[serde(skip)]
    max_events: usize,
}

impl RegisteredModel {
    /// Create a new registered model entry
    pub fn new(capability: ModelCapability) -> Self {
        let baseline = Some(PerformanceBaseline::from_performance(&capability.performance));
        Self {
            capability,
            baseline,
            registered_at: Utc::now(),
            recent_events: Vec::new(),
            max_events: 100,
        }
    }

    /// Add an event to the model history
    pub fn add_event(&mut self, event: ModelEvent) {
        self.recent_events.push(event);
        // Keep only recent events
        if self.recent_events.len() > self.max_events {
            self.recent_events.remove(0);
        }
    }

    /// Update the performance baseline
    pub fn update_baseline(&mut self, perf: &ModelPerformance) {
        self.baseline = Some(PerformanceBaseline::from_performance(perf));
    }

    /// Check for performance degradation
    pub fn check_degradation(&self) -> Option<String> {
        self.baseline
            .as_ref()
            .and_then(|b| b.is_degraded(&self.capability.performance))
    }
}

/// Query criteria for finding models
#[derive(Debug, Clone, Default)]
pub struct ModelQuery {
    /// Filter by model ID
    pub model_id: Option<String>,
    /// Filter by model type
    pub model_type: Option<String>,
    /// Minimum version required
    pub min_version: Option<String>,
    /// Minimum precision required
    pub min_precision: Option<f64>,
    /// Minimum FPS required
    pub min_fps: Option<f64>,
    /// Filter by operational status
    pub status: Option<Vec<OperationalStatus>>,
    /// Only include operational models
    pub operational_only: bool,
    /// Only include non-degraded models
    pub exclude_degraded: bool,
}

impl ModelQuery {
    /// Create a new empty query (matches all)
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by model ID
    pub fn with_model_id(mut self, id: impl Into<String>) -> Self {
        self.model_id = Some(id.into());
        self
    }

    /// Filter by model type
    pub fn with_model_type(mut self, model_type: impl Into<String>) -> Self {
        self.model_type = Some(model_type.into());
        self
    }

    /// Require minimum version
    pub fn with_min_version(mut self, version: impl Into<String>) -> Self {
        self.min_version = Some(version.into());
        self
    }

    /// Require minimum precision
    pub fn with_min_precision(mut self, precision: f64) -> Self {
        self.min_precision = Some(precision);
        self
    }

    /// Require minimum FPS
    pub fn with_min_fps(mut self, fps: f64) -> Self {
        self.min_fps = Some(fps);
        self
    }

    /// Filter by status
    pub fn with_status(mut self, status: Vec<OperationalStatus>) -> Self {
        self.status = Some(status);
        self
    }

    /// Only return operational models
    pub fn operational(mut self) -> Self {
        self.operational_only = true;
        self
    }

    /// Exclude degraded models
    pub fn healthy(mut self) -> Self {
        self.exclude_degraded = true;
        self
    }

    /// Check if a model matches this query
    pub fn matches(&self, model: &ModelCapability) -> bool {
        // Model ID filter
        if let Some(ref id) = self.model_id {
            if &model.model_id != id {
                return false;
            }
        }

        // Model type filter
        if let Some(ref model_type) = self.model_type {
            if &model.model_type != model_type {
                return false;
            }
        }

        // Version filter
        if let Some(ref min_version) = self.min_version {
            if !model.meets_version(min_version) {
                return false;
            }
        }

        // Precision filter
        if let Some(min_precision) = self.min_precision {
            if !model.meets_precision(min_precision) {
                return false;
            }
        }

        // FPS filter
        if let Some(min_fps) = self.min_fps {
            if model.performance.fps < min_fps {
                return false;
            }
        }

        // Status filter
        if let Some(ref statuses) = self.status {
            if !statuses.contains(&model.operational_status) {
                return false;
            }
        }

        // Operational filter
        if self.operational_only && !model.is_operational() {
            return false;
        }

        // Degradation filter
        if self.exclude_degraded && model.degraded {
            return false;
        }

        true
    }
}

/// Runtime model registry for a platform
///
/// Tracks all loaded models, their lifecycle events, and performance.
/// Integrates with beacon broadcasting for capability advertisement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRegistry {
    /// Platform identifier
    pub platform_id: String,
    /// Registered models by model_id
    models: HashMap<String, RegisteredModel>,
    /// All lifecycle events (for audit trail)
    events: Vec<ModelEvent>,
    /// Maximum events to retain
    #[serde(skip)]
    max_events: usize,
    /// Current resource metrics
    pub resources: ResourceMetrics,
}

impl ModelRegistry {
    /// Create a new model registry for a platform
    pub fn new(platform_id: impl Into<String>) -> Self {
        Self {
            platform_id: platform_id.into(),
            models: HashMap::new(),
            events: Vec::new(),
            max_events: 1000,
            resources: ResourceMetrics {
                gpu_utilization: None,
                memory_used_mb: None,
                memory_total_mb: None,
                cpu_utilization: None,
            },
        }
    }

    /// Register a new model
    pub fn register_model(&mut self, capability: ModelCapability) -> Result<(), RegistryError> {
        if self.models.contains_key(&capability.model_id) {
            return Err(RegistryError::ModelAlreadyExists(
                capability.model_id.clone(),
            ));
        }

        let model_id = capability.model_id.clone();
        let model_version = capability.model_version.clone();

        info!(
            platform = %self.platform_id,
            model = %model_id,
            version = %model_version,
            "Registering model"
        );

        let registered = RegisteredModel::new(capability);
        self.models.insert(model_id.clone(), registered);

        self.emit_event(ModelEvent {
            event_type: ModelEventType::Registered,
            model_id,
            model_version,
            platform_id: self.platform_id.clone(),
            timestamp: Utc::now(),
            details: None,
            previous_status: None,
            new_status: Some(OperationalStatus::Loading),
        });

        Ok(())
    }

    /// Mark a model as loaded
    pub fn load_model(&mut self, model_id: &str) -> Result<(), RegistryError> {
        let (previous_status, model_version) = {
            let model = self
                .models
                .get_mut(model_id)
                .ok_or_else(|| RegistryError::ModelNotFound(model_id.to_string()))?;

            let previous_status = model.capability.operational_status;
            model.capability.loaded_at = Some(Utc::now());
            model.capability.operational_status = OperationalStatus::Ready;
            (previous_status, model.capability.model_version.clone())
        };

        info!(
            platform = %self.platform_id,
            model = %model_id,
            "Model loaded"
        );

        self.emit_event(ModelEvent {
            event_type: ModelEventType::Loaded,
            model_id: model_id.to_string(),
            model_version,
            platform_id: self.platform_id.clone(),
            timestamp: Utc::now(),
            details: None,
            previous_status: Some(previous_status),
            new_status: Some(OperationalStatus::Ready),
        });

        Ok(())
    }

    /// Unload a model from memory
    pub fn unload_model(&mut self, model_id: &str) -> Result<(), RegistryError> {
        let (previous_status, model_version) = {
            let model = self
                .models
                .get_mut(model_id)
                .ok_or_else(|| RegistryError::ModelNotFound(model_id.to_string()))?;

            let previous_status = model.capability.operational_status;
            model.capability.operational_status = OperationalStatus::Unloaded;
            (previous_status, model.capability.model_version.clone())
        };

        info!(
            platform = %self.platform_id,
            model = %model_id,
            "Model unloaded"
        );

        self.emit_event(ModelEvent {
            event_type: ModelEventType::Unloaded,
            model_id: model_id.to_string(),
            model_version,
            platform_id: self.platform_id.clone(),
            timestamp: Utc::now(),
            details: None,
            previous_status: Some(previous_status),
            new_status: Some(OperationalStatus::Unloaded),
        });

        Ok(())
    }

    /// Start a model update
    pub fn start_update(
        &mut self,
        model_id: &str,
        new_version: &str,
    ) -> Result<(), RegistryError> {
        let (previous_status, from_version) = {
            let model = self
                .models
                .get_mut(model_id)
                .ok_or_else(|| RegistryError::ModelNotFound(model_id.to_string()))?;

            let previous_status = model.capability.operational_status;
            let from_version = model.capability.model_version.clone();
            model.capability.operational_status = OperationalStatus::Updating;
            (previous_status, from_version)
        };

        info!(
            platform = %self.platform_id,
            model = %model_id,
            from_version = %from_version,
            to_version = %new_version,
            "Starting model update"
        );

        self.emit_event(ModelEvent {
            event_type: ModelEventType::UpdateStarted,
            model_id: model_id.to_string(),
            model_version: new_version.to_string(),
            platform_id: self.platform_id.clone(),
            timestamp: Utc::now(),
            details: Some(format!("Updating from {} to {}", from_version, new_version)),
            previous_status: Some(previous_status),
            new_status: Some(OperationalStatus::Updating),
        });

        Ok(())
    }

    /// Complete a model update
    pub fn complete_update(
        &mut self,
        model_id: &str,
        new_capability: ModelCapability,
    ) -> Result<(), RegistryError> {
        let new_version = new_capability.model_version.clone();
        let perf_for_baseline = new_capability.performance.clone();

        let old_version = {
            let model = self
                .models
                .get_mut(model_id)
                .ok_or_else(|| RegistryError::ModelNotFound(model_id.to_string()))?;

            let old_version = model.capability.model_version.clone();

            // Update capability and reset baseline
            model.capability = new_capability;
            model.capability.operational_status = OperationalStatus::Ready;
            model.capability.loaded_at = Some(Utc::now());
            model.capability.inference_count = Some(0);
            model.update_baseline(&perf_for_baseline);
            old_version
        };

        info!(
            platform = %self.platform_id,
            model = %model_id,
            old_version = %old_version,
            new_version = %new_version,
            "Model update completed"
        );

        self.emit_event(ModelEvent {
            event_type: ModelEventType::UpdateCompleted,
            model_id: model_id.to_string(),
            model_version: new_version,
            platform_id: self.platform_id.clone(),
            timestamp: Utc::now(),
            details: Some(format!("Updated from {}", old_version)),
            previous_status: Some(OperationalStatus::Updating),
            new_status: Some(OperationalStatus::Ready),
        });

        Ok(())
    }

    /// Fail a model update
    pub fn fail_update(&mut self, model_id: &str, reason: &str) -> Result<(), RegistryError> {
        let model_version = {
            let model = self
                .models
                .get_mut(model_id)
                .ok_or_else(|| RegistryError::ModelNotFound(model_id.to_string()))?;

            // Revert to previous ready state
            model.capability.operational_status = OperationalStatus::Ready;
            model.capability.model_version.clone()
        };

        warn!(
            platform = %self.platform_id,
            model = %model_id,
            reason = %reason,
            "Model update failed"
        );

        self.emit_event(ModelEvent {
            event_type: ModelEventType::UpdateFailed,
            model_id: model_id.to_string(),
            model_version,
            platform_id: self.platform_id.clone(),
            timestamp: Utc::now(),
            details: Some(reason.to_string()),
            previous_status: Some(OperationalStatus::Updating),
            new_status: Some(OperationalStatus::Ready),
        });

        Ok(())
    }

    /// Record an inference and update metrics
    pub fn record_inference(
        &mut self,
        model_id: &str,
        latency_ms: f64,
    ) -> Result<(), RegistryError> {
        // First, update the model and check for degradation
        let degradation_event = {
            let model = self
                .models
                .get_mut(model_id)
                .ok_or_else(|| RegistryError::ModelNotFound(model_id.to_string()))?;

            model.capability.record_inference();

            // Update performance metrics with rolling average
            if let Some(current_latency) = model.capability.performance.latency_ms {
                let count = model.capability.inference_count.unwrap_or(1) as f64;
                let new_avg = (current_latency * (count - 1.0) + latency_ms) / count;
                model.capability.performance.latency_ms = Some(new_avg);
            } else {
                model.capability.performance.latency_ms = Some(latency_ms);
            }

            // Check for degradation and prepare event if needed
            if let Some(reason) = model.check_degradation() {
                if !model.capability.degraded {
                    model.capability.mark_degraded(&reason);
                    Some((model.capability.model_version.clone(), reason))
                } else {
                    None
                }
            } else {
                None
            }
        };

        // Emit degradation event outside the borrow
        if let Some((model_version, reason)) = degradation_event {
            self.emit_event(ModelEvent {
                event_type: ModelEventType::DegradationDetected,
                model_id: model_id.to_string(),
                model_version,
                platform_id: self.platform_id.clone(),
                timestamp: Utc::now(),
                details: Some(reason),
                previous_status: Some(OperationalStatus::Active),
                new_status: Some(OperationalStatus::Degraded),
            });
        }

        Ok(())
    }

    /// Update resource metrics
    pub fn update_resources(&mut self, resources: ResourceMetrics) {
        self.resources = resources;
    }

    /// Get a model by ID
    pub fn get_model(&self, model_id: &str) -> Option<&ModelCapability> {
        self.models.get(model_id).map(|m| &m.capability)
    }

    /// Get a mutable model by ID
    pub fn get_model_mut(&mut self, model_id: &str) -> Option<&mut ModelCapability> {
        self.models.get_mut(model_id).map(|m| &mut m.capability)
    }

    /// Get all registered models
    pub fn models(&self) -> impl Iterator<Item = &ModelCapability> {
        self.models.values().map(|m| &m.capability)
    }

    /// Query models matching criteria
    pub fn query(&self, query: &ModelQuery) -> Vec<&ModelCapability> {
        self.models
            .values()
            .map(|m| &m.capability)
            .filter(|m| query.matches(m))
            .collect()
    }

    /// Get model count
    pub fn model_count(&self) -> usize {
        self.models.len()
    }

    /// Get operational model count
    pub fn operational_model_count(&self) -> usize {
        self.models
            .values()
            .filter(|m| m.capability.is_operational())
            .count()
    }

    /// Get recent events
    pub fn recent_events(&self, limit: usize) -> &[ModelEvent] {
        let start = self.events.len().saturating_sub(limit);
        &self.events[start..]
    }

    /// Get events for a specific model
    pub fn model_events(&self, model_id: &str) -> Vec<&ModelEvent> {
        self.events
            .iter()
            .filter(|e| e.model_id == model_id)
            .collect()
    }

    /// Generate a capability advertisement from current state
    pub fn generate_advertisement(&self) -> CapabilityAdvertisement {
        let models: Vec<ModelCapability> = self
            .models
            .values()
            .map(|m| m.capability.clone())
            .collect();

        CapabilityAdvertisement {
            platform_id: self.platform_id.clone(),
            advertised_at: Utc::now(),
            models,
            resources: Some(self.resources.clone()),
        }
    }

    /// Remove a model from the registry
    pub fn remove_model(&mut self, model_id: &str) -> Result<ModelCapability, RegistryError> {
        self.models
            .remove(model_id)
            .map(|m| m.capability)
            .ok_or_else(|| RegistryError::ModelNotFound(model_id.to_string()))
    }

    /// Emit an event
    fn emit_event(&mut self, event: ModelEvent) {
        debug!(
            event_type = ?event.event_type,
            model = %event.model_id,
            "Model event"
        );

        // Add to model's event history
        if let Some(model) = self.models.get_mut(&event.model_id) {
            model.add_event(event.clone());
        }

        // Add to global event history
        self.events.push(event);
        if self.events.len() > self.max_events {
            self.events.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::ResourceRequirements;

    fn create_test_capability() -> ModelCapability {
        ModelCapability::new(
            "object_tracker",
            "1.3.0",
            "sha256:abc123",
            "detector_tracker",
            ModelPerformance::new(0.91, 0.87, 15.0).with_latency(67.0),
        )
        .with_framework("ONNX", "FP16")
        .with_resource_requirements(ResourceRequirements::edge_default())
    }

    #[test]
    fn test_registry_creation() {
        let registry = ModelRegistry::new("platform-01");
        assert_eq!(registry.platform_id, "platform-01");
        assert_eq!(registry.model_count(), 0);
    }

    #[test]
    fn test_register_model() {
        let mut registry = ModelRegistry::new("platform-01");
        let cap = create_test_capability();

        registry.register_model(cap).unwrap();

        assert_eq!(registry.model_count(), 1);
        assert!(registry.get_model("object_tracker").is_some());
    }

    #[test]
    fn test_register_duplicate_fails() {
        let mut registry = ModelRegistry::new("platform-01");
        let cap = create_test_capability();

        registry.register_model(cap.clone()).unwrap();
        let result = registry.register_model(cap);

        assert!(matches!(result, Err(RegistryError::ModelAlreadyExists(_))));
    }

    #[test]
    fn test_load_model() {
        let mut registry = ModelRegistry::new("platform-01");
        let cap = create_test_capability();

        registry.register_model(cap).unwrap();
        registry.load_model("object_tracker").unwrap();

        let model = registry.get_model("object_tracker").unwrap();
        assert_eq!(model.operational_status, OperationalStatus::Ready);
        assert!(model.loaded_at.is_some());
    }

    #[test]
    fn test_unload_model() {
        let mut registry = ModelRegistry::new("platform-01");
        let cap = create_test_capability();

        registry.register_model(cap).unwrap();
        registry.load_model("object_tracker").unwrap();
        registry.unload_model("object_tracker").unwrap();

        let model = registry.get_model("object_tracker").unwrap();
        assert_eq!(model.operational_status, OperationalStatus::Unloaded);
    }

    #[test]
    fn test_model_query() {
        let mut registry = ModelRegistry::new("platform-01");

        // Register multiple models
        let tracker = create_test_capability();
        let mut classifier = ModelCapability::new(
            "classifier",
            "2.0.0",
            "sha256:def456",
            "classifier",
            ModelPerformance::new(0.95, 0.92, 30.0),
        );
        classifier.operational_status = OperationalStatus::Ready;

        registry.register_model(tracker).unwrap();
        registry.load_model("object_tracker").unwrap();
        registry.register_model(classifier).unwrap();
        registry.load_model("classifier").unwrap();

        // Query by type
        let query = ModelQuery::new().with_model_type("detector_tracker");
        let results = registry.query(&query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].model_id, "object_tracker");

        // Query by min version
        let query = ModelQuery::new().with_min_version("2.0.0");
        let results = registry.query(&query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].model_id, "classifier");

        // Query by min precision
        let query = ModelQuery::new().with_min_precision(0.93);
        let results = registry.query(&query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].model_id, "classifier");

        // Query operational only
        let query = ModelQuery::new().operational();
        let results = registry.query(&query);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_record_inference() {
        let mut registry = ModelRegistry::new("platform-01");
        let cap = create_test_capability();

        registry.register_model(cap).unwrap();
        registry.load_model("object_tracker").unwrap();

        registry.record_inference("object_tracker", 70.0).unwrap();
        registry.record_inference("object_tracker", 72.0).unwrap();

        let model = registry.get_model("object_tracker").unwrap();
        assert_eq!(model.inference_count, Some(2));
        assert!(model.last_inference_at.is_some());
    }

    #[test]
    fn test_model_update_lifecycle() {
        let mut registry = ModelRegistry::new("platform-01");
        let cap = create_test_capability();

        registry.register_model(cap).unwrap();
        registry.load_model("object_tracker").unwrap();

        // Start update
        registry.start_update("object_tracker", "1.4.0").unwrap();
        let model = registry.get_model("object_tracker").unwrap();
        assert_eq!(model.operational_status, OperationalStatus::Updating);

        // Complete update
        let new_cap = ModelCapability::new(
            "object_tracker",
            "1.4.0",
            "sha256:new123",
            "detector_tracker",
            ModelPerformance::new(0.93, 0.89, 18.0),
        );
        registry.complete_update("object_tracker", new_cap).unwrap();

        let model = registry.get_model("object_tracker").unwrap();
        assert_eq!(model.model_version, "1.4.0");
        assert_eq!(model.operational_status, OperationalStatus::Ready);
    }

    #[test]
    fn test_event_tracking() {
        let mut registry = ModelRegistry::new("platform-01");
        let cap = create_test_capability();

        registry.register_model(cap).unwrap();
        registry.load_model("object_tracker").unwrap();
        registry.unload_model("object_tracker").unwrap();

        let events = registry.recent_events(10);
        assert_eq!(events.len(), 3); // Registered, Loaded, Unloaded

        let model_events = registry.model_events("object_tracker");
        assert_eq!(model_events.len(), 3);
    }

    #[test]
    fn test_generate_advertisement() {
        let mut registry = ModelRegistry::new("platform-01");
        let cap = create_test_capability();

        registry.register_model(cap).unwrap();
        registry.load_model("object_tracker").unwrap();
        registry.update_resources(ResourceMetrics {
            gpu_utilization: Some(0.65),
            memory_used_mb: Some(2048),
            memory_total_mb: Some(8192),
            cpu_utilization: Some(0.3),
        });

        let advert = registry.generate_advertisement();

        assert_eq!(advert.platform_id, "platform-01");
        assert_eq!(advert.models.len(), 1);
        assert!(advert.resources.is_some());
    }

    #[test]
    fn test_version_comparison() {
        let cap = create_test_capability();

        assert!(cap.meets_version("1.0.0"));
        assert!(cap.meets_version("1.3.0"));
        assert!(!cap.meets_version("1.4.0"));
        assert!(!cap.meets_version("2.0.0"));
    }

    #[test]
    fn test_degradation_detection() {
        let baseline_perf = ModelPerformance::new(0.91, 0.87, 15.0).with_latency(67.0);
        let baseline = PerformanceBaseline::from_performance(&baseline_perf);

        // No degradation
        let current = ModelPerformance::new(0.91, 0.87, 14.0).with_latency(70.0);
        assert!(baseline.is_degraded(&current).is_none());

        // FPS degradation
        let degraded_fps = ModelPerformance::new(0.91, 0.87, 10.0).with_latency(67.0);
        assert!(baseline.is_degraded(&degraded_fps).is_some());

        // Latency degradation
        let degraded_latency = ModelPerformance::new(0.91, 0.87, 15.0).with_latency(100.0);
        assert!(baseline.is_degraded(&degraded_latency).is_some());
    }
}
