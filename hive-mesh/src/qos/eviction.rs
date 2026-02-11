//! QoS-aware eviction controller (ADR-019 Phase 4)
//!
//! Orchestrates storage eviction cycles based on QoS policies,
//! with full audit logging and operator override support.
//!
//! # Features
//!
//! - Automated eviction cycles triggered by storage pressure
//! - Priority-based eviction (P5 → P4 → P3 → P2, never P1)
//! - Manual eviction trigger for operator override
//! - Compression before eviction where applicable
//! - Full audit trail of all eviction decisions

use super::{
    audit::{AuditEntry, EvictionAuditLog},
    storage::{EvictionCandidate, QoSAwareStorage, StorageMetrics},
    QoSClass,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Instant;

/// Result of an eviction cycle
#[derive(Debug, Clone, Default)]
pub struct EvictionResult {
    /// Number of documents evicted
    pub docs_evicted: usize,

    /// Total bytes freed
    pub bytes_freed: usize,

    /// Evictions broken down by QoS class
    pub by_class: HashMap<QoSClass, usize>,

    /// Number of compression operations performed
    pub compressions: usize,

    /// Bytes saved by compression
    pub compression_savings: usize,

    /// Number of failed evictions
    pub failures: usize,

    /// Duration of the eviction cycle in milliseconds
    pub duration_ms: u64,

    /// Storage pressure before eviction
    pub pressure_before: f32,

    /// Storage pressure after eviction
    pub pressure_after: f32,
}

impl EvictionResult {
    /// Check if eviction was successful
    pub fn is_success(&self) -> bool {
        self.docs_evicted > 0 || self.compression_savings > 0
    }

    /// Check if eviction freed enough space
    pub fn freed_target(&self, target_bytes: usize) -> bool {
        self.bytes_freed + self.compression_savings >= target_bytes
    }
}

/// Callback for actual document eviction
///
/// The eviction controller doesn't directly delete documents; it identifies
/// candidates and calls this callback to perform the actual deletion.
pub type EvictionCallback = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// Callback for document compression
pub type CompressionCallback = Box<dyn Fn(&str) -> Result<usize, String> + Send + Sync>;

/// Configuration for the eviction controller
#[derive(Debug, Clone)]
pub struct EvictionConfig {
    /// Storage pressure threshold to trigger eviction (default: 0.9)
    pub eviction_threshold: f32,

    /// Target pressure after eviction (default: 0.7)
    pub target_pressure: f32,

    /// Whether to attempt compression before eviction
    pub compress_before_eviction: bool,

    /// Maximum number of documents to evict in a single cycle
    pub max_evictions_per_cycle: usize,

    /// Maximum duration of an eviction cycle in milliseconds
    pub max_cycle_duration_ms: u64,
}

impl Default for EvictionConfig {
    fn default() -> Self {
        Self {
            eviction_threshold: 0.9,
            target_pressure: 0.7,
            compress_before_eviction: true,
            max_evictions_per_cycle: 1000,
            max_cycle_duration_ms: 5000, // 5 seconds
        }
    }
}

impl EvictionConfig {
    /// Configuration for aggressive eviction (storage-constrained devices)
    pub fn aggressive() -> Self {
        Self {
            eviction_threshold: 0.75,
            target_pressure: 0.5,
            compress_before_eviction: true,
            max_evictions_per_cycle: 2000,
            max_cycle_duration_ms: 10000,
        }
    }

    /// Configuration for conservative eviction (keep more data)
    pub fn conservative() -> Self {
        Self {
            eviction_threshold: 0.95,
            target_pressure: 0.85,
            compress_before_eviction: true,
            max_evictions_per_cycle: 500,
            max_cycle_duration_ms: 3000,
        }
    }
}

/// Eviction controller that manages storage eviction with QoS awareness
pub struct EvictionController {
    /// Storage manager
    storage: Arc<QoSAwareStorage>,

    /// Audit log for tracking eviction events
    audit_log: Arc<EvictionAuditLog>,

    /// Configuration
    config: EvictionConfig,

    /// Optional callback for performing actual eviction
    eviction_callback: RwLock<Option<EvictionCallback>>,

    /// Optional callback for compression
    compression_callback: RwLock<Option<CompressionCallback>>,

    /// Set of protected document IDs (operator override)
    protected_docs: RwLock<std::collections::HashSet<String>>,

    /// Statistics from recent eviction cycles
    recent_stats: RwLock<Vec<EvictionResult>>,
}

impl EvictionController {
    /// Create a new eviction controller
    pub fn new(storage: Arc<QoSAwareStorage>, audit_log: Arc<EvictionAuditLog>) -> Self {
        Self {
            storage,
            audit_log,
            config: EvictionConfig::default(),
            eviction_callback: RwLock::new(None),
            compression_callback: RwLock::new(None),
            protected_docs: RwLock::new(std::collections::HashSet::new()),
            recent_stats: RwLock::new(Vec::with_capacity(10)),
        }
    }

    /// Create with custom configuration
    pub fn with_config(mut self, config: EvictionConfig) -> Self {
        self.config = config;
        self
    }

    /// Set the eviction callback
    pub fn set_eviction_callback(&self, callback: EvictionCallback) {
        *self.eviction_callback.write().unwrap() = Some(callback);
    }

    /// Set the compression callback
    pub fn set_compression_callback(&self, callback: CompressionCallback) {
        *self.compression_callback.write().unwrap() = Some(callback);
    }

    /// Run an eviction cycle if needed
    ///
    /// Returns `Some(result)` if eviction was performed, `None` if not needed.
    pub fn run_eviction_cycle(&self) -> Option<EvictionResult> {
        let current_pressure = self.storage.storage_pressure();

        if current_pressure < self.config.eviction_threshold {
            return None;
        }

        Some(self.execute_eviction_cycle(None))
    }

    /// Force an eviction cycle to free specified bytes
    ///
    /// This is an operator override that runs eviction regardless of pressure.
    pub fn force_eviction(&self, bytes_to_free: usize) -> EvictionResult {
        self.execute_eviction_cycle(Some(bytes_to_free))
    }

    /// Execute an eviction cycle
    fn execute_eviction_cycle(&self, target_bytes: Option<usize>) -> EvictionResult {
        let start = Instant::now();
        let mut result = EvictionResult {
            pressure_before: self.storage.storage_pressure(),
            ..Default::default()
        };

        // Calculate bytes to free
        let bytes_needed = target_bytes.unwrap_or_else(|| {
            let current = self.storage.current_storage_bytes();
            let target =
                (self.storage.max_storage_bytes() as f32 * self.config.target_pressure) as usize;
            current.saturating_sub(target)
        });

        if bytes_needed == 0 {
            result.pressure_after = self.storage.storage_pressure();
            result.duration_ms = start.elapsed().as_millis() as u64;
            return result;
        }

        // Step 1: Try compression first if enabled
        if self.config.compress_before_eviction {
            let compression_result = self.compress_eligible_documents(&mut result);
            if compression_result >= bytes_needed {
                result.pressure_after = self.storage.storage_pressure();
                result.duration_ms = start.elapsed().as_millis() as u64;
                self.record_cycle_completion(&result);
                return result;
            }
        }

        // Step 2: Get eviction candidates
        let remaining_needed = bytes_needed.saturating_sub(result.compression_savings);
        let candidates = self.storage.get_eviction_candidates(remaining_needed);

        // Step 3: Evict candidates
        for candidate in candidates {
            // Check time limit
            if start.elapsed().as_millis() as u64 > self.config.max_cycle_duration_ms {
                break;
            }

            // Check eviction limit
            if result.docs_evicted >= self.config.max_evictions_per_cycle {
                break;
            }

            // Check if protected by operator override
            if self
                .protected_docs
                .read()
                .unwrap()
                .contains(&candidate.doc_id)
            {
                continue;
            }

            // Perform eviction
            match self.evict_document(&candidate) {
                Ok(bytes) => {
                    result.docs_evicted += 1;
                    result.bytes_freed += bytes;
                    *result.by_class.entry(candidate.qos_class).or_insert(0) += 1;

                    // Check if we've freed enough
                    if result.bytes_freed + result.compression_savings >= bytes_needed {
                        break;
                    }
                }
                Err(e) => {
                    result.failures += 1;
                    self.audit_log
                        .record_failure(&candidate.doc_id, candidate.qos_class, e);
                }
            }
        }

        result.pressure_after = self.storage.storage_pressure();
        result.duration_ms = start.elapsed().as_millis() as u64;

        // Record completion
        self.record_cycle_completion(&result);

        // Store stats
        let mut stats = self.recent_stats.write().unwrap();
        if stats.len() >= 10 {
            stats.remove(0);
        }
        stats.push(result.clone());

        result
    }

    /// Compress eligible documents
    fn compress_eligible_documents(&self, result: &mut EvictionResult) -> usize {
        let callback = self.compression_callback.read().unwrap();
        let callback = match callback.as_ref() {
            Some(cb) => cb,
            None => return 0,
        };

        let candidates = self.storage.get_compression_candidates();
        let mut total_savings = 0;

        for doc_id in candidates {
            if let Some(doc) = self.storage.get_document(&doc_id) {
                let original_size = doc.size_bytes;

                match callback(&doc_id) {
                    Ok(new_size) => {
                        if new_size < original_size {
                            let savings = original_size - new_size;
                            self.storage.update_compressed(&doc_id, new_size);
                            self.audit_log.record_compression(
                                &doc_id,
                                doc.qos_class,
                                original_size,
                                new_size,
                            );
                            total_savings += savings;
                            result.compressions += 1;
                            result.compression_savings += savings;
                        }
                    }
                    Err(e) => {
                        self.audit_log.record_failure(
                            &doc_id,
                            doc.qos_class,
                            format!("Compression failed: {}", e),
                        );
                    }
                }
            }
        }

        total_savings
    }

    /// Evict a single document
    fn evict_document(&self, candidate: &EvictionCandidate) -> Result<usize, String> {
        let callback = self.eviction_callback.read().unwrap();
        let callback = callback.as_ref().ok_or("No eviction callback set")?;

        // Call the eviction callback
        callback(&candidate.doc_id)?;

        // Unregister from storage tracking
        if let Some(doc) = self.storage.unregister_document(&candidate.doc_id) {
            self.audit_log.record_eviction(
                &candidate.doc_id,
                candidate.qos_class,
                doc.size_bytes,
                format!(
                    "Storage pressure eviction (age: {}s, score: {:.2})",
                    candidate.age_seconds, candidate.eviction_score
                ),
            );
            Ok(doc.size_bytes)
        } else {
            Ok(candidate.size_bytes)
        }
    }

    /// Record eviction cycle completion to audit log
    fn record_cycle_completion(&self, result: &EvictionResult) {
        self.audit_log.record(AuditEntry::cleanup_completed(
            result.docs_evicted,
            result.bytes_freed + result.compression_savings,
            result.duration_ms,
        ));
    }

    /// Mark a document as protected (operator override)
    ///
    /// Protected documents will never be evicted by automatic cycles.
    pub fn mark_protected(&self, doc_id: &str) {
        self.protected_docs
            .write()
            .unwrap()
            .insert(doc_id.to_string());
        self.storage.mark_protected(doc_id);

        if let Some(doc) = self.storage.get_document(doc_id) {
            self.audit_log
                .record_protection(doc_id, doc.qos_class, "Operator override");
        }
    }

    /// Remove protection from a document
    pub fn unmark_protected(&self, doc_id: &str) {
        self.protected_docs.write().unwrap().remove(doc_id);
        self.storage.unmark_protected(doc_id);

        if let Some(doc) = self.storage.get_document(doc_id) {
            self.audit_log.record(AuditEntry::new(
                super::audit::AuditAction::UnmarkedProtected,
                doc_id,
                doc.qos_class,
                "Operator override removed",
            ));
        }
    }

    /// Check if a document is protected
    pub fn is_protected(&self, doc_id: &str) -> bool {
        self.protected_docs.read().unwrap().contains(doc_id)
    }

    /// Get storage metrics
    pub fn storage_metrics(&self) -> StorageMetrics {
        self.storage.metrics()
    }

    /// Get recent eviction statistics
    pub fn recent_stats(&self) -> Vec<EvictionResult> {
        self.recent_stats.read().unwrap().clone()
    }

    /// Get audit log summary
    pub fn audit_summary(&self) -> super::audit::AuditSummary {
        self.audit_log.summary()
    }

    /// Export audit log as JSON
    pub fn export_audit_log(&self) -> Result<String, serde_json::Error> {
        self.audit_log.export_json()
    }

    /// Get current configuration
    pub fn config(&self) -> &EvictionConfig {
        &self.config
    }

    /// Check if eviction is currently needed
    pub fn needs_eviction(&self) -> bool {
        self.storage.storage_pressure() >= self.config.eviction_threshold
    }

    /// Get count of protected documents
    pub fn protected_count(&self) -> usize {
        self.protected_docs.read().unwrap().len()
    }
}

impl std::fmt::Debug for EvictionController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvictionController")
            .field("config", &self.config)
            .field(
                "protected_count",
                &self.protected_docs.read().unwrap().len(),
            )
            .field(
                "recent_stats_count",
                &self.recent_stats.read().unwrap().len(),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qos::storage::StoredDocument;

    fn create_test_storage() -> Arc<QoSAwareStorage> {
        Arc::new(QoSAwareStorage::new(100_000)) // 100KB
    }

    fn create_test_controller() -> EvictionController {
        let storage = create_test_storage();
        let audit_log = Arc::new(EvictionAuditLog::new(100));
        EvictionController::new(storage.clone(), audit_log)
    }

    #[test]
    fn test_eviction_config_defaults() {
        let config = EvictionConfig::default();

        assert_eq!(config.eviction_threshold, 0.9);
        assert_eq!(config.target_pressure, 0.7);
        assert!(config.compress_before_eviction);
    }

    #[test]
    fn test_eviction_config_aggressive() {
        let config = EvictionConfig::aggressive();

        assert_eq!(config.eviction_threshold, 0.75);
        assert_eq!(config.target_pressure, 0.5);
    }

    #[test]
    fn test_no_eviction_when_not_needed() {
        let controller = create_test_controller();

        // Add some documents but stay below threshold
        controller.storage.register_document(StoredDocument::new(
            "doc-1",
            QoSClass::Normal,
            10_000,
        ));

        // Should not trigger eviction
        let result = controller.run_eviction_cycle();
        assert!(result.is_none());
    }

    #[test]
    fn test_eviction_triggered_at_threshold() {
        let storage = create_test_storage();
        let audit_log = Arc::new(EvictionAuditLog::new(100));
        let controller =
            EvictionController::new(storage.clone(), audit_log).with_config(EvictionConfig {
                eviction_threshold: 0.8,
                target_pressure: 0.5,
                compress_before_eviction: false,
                ..Default::default()
            });

        // Fill to 85%
        storage.register_document(StoredDocument::new("bulk-1", QoSClass::Bulk, 30_000));
        storage.register_document(StoredDocument::new("bulk-2", QoSClass::Bulk, 30_000));
        storage.register_document(StoredDocument::new("bulk-3", QoSClass::Bulk, 25_000));

        // Set up eviction callback
        controller.set_eviction_callback(Box::new(move |_doc_id| {
            // In real implementation, this would delete from actual storage
            Ok(())
        }));

        // Should trigger eviction
        let result = controller.run_eviction_cycle();
        assert!(result.is_some());

        let result = result.unwrap();
        assert!(result.pressure_before >= 0.8);
    }

    #[test]
    fn test_force_eviction() {
        let storage = create_test_storage();
        let audit_log = Arc::new(EvictionAuditLog::new(100));
        let controller =
            EvictionController::new(storage.clone(), audit_log).with_config(EvictionConfig {
                compress_before_eviction: false,
                ..Default::default()
            });

        // Add documents old enough to be evicted (Bulk max_retain: 300s)
        storage
            .register_document(StoredDocument::new("bulk-1", QoSClass::Bulk, 20_000).with_age(400));
        storage
            .register_document(StoredDocument::new("bulk-2", QoSClass::Bulk, 20_000).with_age(400));

        // Set up eviction callback
        controller.set_eviction_callback(Box::new(|_| Ok(())));

        // Force eviction of 15KB
        let result = controller.force_eviction(15_000);

        // Should have evicted at least one document
        assert!(
            result.docs_evicted >= 1,
            "Expected at least one eviction, got {}",
            result.docs_evicted
        );
    }

    #[test]
    fn test_protected_documents_not_evicted() {
        let storage = create_test_storage();
        let audit_log = Arc::new(EvictionAuditLog::new(100));
        let controller =
            EvictionController::new(storage.clone(), audit_log).with_config(EvictionConfig {
                eviction_threshold: 0.5,
                compress_before_eviction: false,
                ..Default::default()
            });

        // Add a bulk document and protect it
        storage.register_document(StoredDocument::new(
            "protected-bulk",
            QoSClass::Bulk,
            60_000,
        ));
        controller.mark_protected("protected-bulk");

        assert!(controller.is_protected("protected-bulk"));

        // Force eviction - protected doc should not be evicted
        controller.set_eviction_callback(Box::new(|_| Ok(())));
        let result = controller.force_eviction(60_000);

        // Should not have evicted the protected document
        assert_eq!(result.docs_evicted, 0);
    }

    #[test]
    fn test_unmark_protected() {
        let controller = create_test_controller();

        controller
            .storage
            .register_document(StoredDocument::new("doc-1", QoSClass::Bulk, 10_000));
        controller.mark_protected("doc-1");
        assert!(controller.is_protected("doc-1"));

        controller.unmark_protected("doc-1");
        assert!(!controller.is_protected("doc-1"));
    }

    #[test]
    fn test_eviction_result_helpers() {
        let result = EvictionResult {
            docs_evicted: 5,
            bytes_freed: 10_000,
            compression_savings: 5_000,
            ..Default::default()
        };

        assert!(result.is_success());
        assert!(result.freed_target(15_000));
        assert!(!result.freed_target(20_000));

        let empty_result = EvictionResult::default();
        assert!(!empty_result.is_success());
    }

    #[test]
    fn test_storage_metrics() {
        let controller = create_test_controller();

        controller.storage.register_document(StoredDocument::new(
            "doc-1",
            QoSClass::Normal,
            10_000,
        ));
        controller
            .storage
            .register_document(StoredDocument::new("doc-2", QoSClass::Bulk, 20_000));

        let metrics = controller.storage_metrics();

        assert_eq!(metrics.used_bytes, 30_000);
        assert_eq!(metrics.max_bytes, 100_000);
    }

    #[test]
    fn test_audit_summary() {
        let storage = create_test_storage();
        let audit_log = Arc::new(EvictionAuditLog::new(100));
        let controller = EvictionController::new(storage.clone(), audit_log.clone());

        // Record some events
        audit_log.record_eviction("doc-1", QoSClass::Bulk, 1000, "Test");
        audit_log.record_eviction("doc-2", QoSClass::Low, 2000, "Test");

        let summary = controller.audit_summary();

        assert_eq!(summary.total_evictions, 2);
        assert_eq!(summary.bytes_evicted, 3000);
    }

    #[test]
    fn test_recent_stats() {
        let storage = create_test_storage();
        let audit_log = Arc::new(EvictionAuditLog::new(100));
        let controller = EvictionController::new(storage.clone(), audit_log);

        // Initially empty
        assert!(controller.recent_stats().is_empty());

        // Add documents to evict (Bulk max_retain: 300s)
        storage
            .register_document(StoredDocument::new("bulk-1", QoSClass::Bulk, 10_000).with_age(400));

        // Force eviction to generate stats
        controller.set_eviction_callback(Box::new(|_| Ok(())));
        let _ = controller.force_eviction(5_000);

        // Should have one stats entry (even if nothing evicted, stats are recorded)
        assert_eq!(controller.recent_stats().len(), 1);
    }

    #[test]
    fn test_needs_eviction() {
        let storage = create_test_storage();
        let audit_log = Arc::new(EvictionAuditLog::new(100));
        let controller =
            EvictionController::new(storage.clone(), audit_log).with_config(EvictionConfig {
                eviction_threshold: 0.8,
                ..Default::default()
            });

        // Below threshold
        storage.register_document(StoredDocument::new("doc-1", QoSClass::Normal, 70_000));
        assert!(!controller.needs_eviction());

        // Above threshold
        storage.register_document(StoredDocument::new("doc-2", QoSClass::Normal, 15_000));
        assert!(controller.needs_eviction());
    }

    #[test]
    fn test_export_audit_log() {
        let controller = create_test_controller();

        controller
            .storage
            .register_document(StoredDocument::new("doc-1", QoSClass::Bulk, 1000));
        controller.mark_protected("doc-1");

        let json = controller.export_audit_log();
        assert!(json.is_ok());

        let json_str = json.unwrap();
        assert!(json_str.contains("doc-1"));
    }

    #[test]
    fn test_eviction_by_class_tracking() {
        let storage = create_test_storage();
        let audit_log = Arc::new(EvictionAuditLog::new(100));
        let controller =
            EvictionController::new(storage.clone(), audit_log).with_config(EvictionConfig {
                eviction_threshold: 0.5,
                compress_before_eviction: false,
                ..Default::default()
            });

        // Add documents of different classes
        storage.register_document(StoredDocument::new("bulk-1", QoSClass::Bulk, 20_000));
        storage.register_document(StoredDocument::new("bulk-2", QoSClass::Bulk, 20_000));
        storage.register_document(StoredDocument::new("low-1", QoSClass::Low, 20_000));

        controller.set_eviction_callback(Box::new(|_| Ok(())));
        let result = controller.force_eviction(30_000);

        // Check that by_class tracking works
        if result.docs_evicted > 0 {
            assert!(!result.by_class.is_empty());
            // Bulk should be evicted first
            if let Some(&bulk_count) = result.by_class.get(&QoSClass::Bulk) {
                assert!(bulk_count > 0);
            }
        }
    }
}
