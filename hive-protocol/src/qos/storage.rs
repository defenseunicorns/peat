//! QoS-aware storage management (ADR-019 Phase 4)
//!
//! Provides QoS-aware storage tracking and eviction candidate selection.
//! This module tracks storage usage and identifies documents for eviction
//! based on priority and retention policies.
//!
//! # Features
//!
//! - Storage tracking with per-QoS-class breakdown
//! - Eviction candidate selection (P5 first, never P1)
//! - Storage pressure monitoring
//! - Compression eligibility tracking

use super::{retention::RetentionPolicies, QoSClass};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::RwLock;

/// Document metadata tracked for eviction decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDocument {
    /// Unique document identifier
    pub doc_id: String,

    /// QoS class of the document
    pub qos_class: QoSClass,

    /// Size in bytes
    pub size_bytes: usize,

    /// Unix timestamp when stored
    pub stored_at: u64,

    /// Unix timestamp of last access
    pub last_accessed: u64,

    /// Whether this document is protected from eviction
    pub protected: bool,

    /// Whether this document has been compressed
    pub compressed: bool,
}

impl StoredDocument {
    /// Create a new stored document record
    pub fn new(doc_id: impl Into<String>, qos_class: QoSClass, size_bytes: usize) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            doc_id: doc_id.into(),
            qos_class,
            size_bytes,
            stored_at: now,
            last_accessed: now,
            protected: false,
            compressed: false,
        }
    }

    /// Create a stored document with a specified age (for testing)
    pub fn with_age(mut self, age_seconds: u64) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.stored_at = now.saturating_sub(age_seconds);
        self.last_accessed = self.stored_at;
        self
    }

    /// Get age in seconds
    pub fn age_seconds(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.stored_at)
    }

    /// Get time since last access in seconds
    pub fn idle_seconds(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.last_accessed)
    }

    /// Mark as accessed
    pub fn touch(&mut self) {
        self.last_accessed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }
}

/// Candidate document for eviction
#[derive(Debug, Clone)]
pub struct EvictionCandidate {
    /// Document identifier
    pub doc_id: String,

    /// QoS class of the document
    pub qos_class: QoSClass,

    /// Age of the document in seconds
    pub age_seconds: u64,

    /// Size in bytes
    pub size_bytes: usize,

    /// Eviction score (higher = evict first)
    pub eviction_score: f64,
}

/// Per-class storage metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClassStorageMetrics {
    /// Number of documents in this class
    pub doc_count: usize,

    /// Total bytes used by this class
    pub total_bytes: usize,

    /// Average age of documents in seconds
    pub avg_age_seconds: u64,

    /// Age of oldest document in seconds
    pub oldest_doc_age: u64,

    /// Number of protected documents
    pub protected_count: usize,

    /// Number of compressed documents
    pub compressed_count: usize,
}

/// Overall storage metrics for monitoring
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageMetrics {
    /// Maximum storage capacity in bytes
    pub max_bytes: usize,

    /// Currently used storage in bytes
    pub used_bytes: usize,

    /// Storage utilization (0.0 - 1.0)
    pub utilization: f32,

    /// Per-class breakdown
    pub by_class: HashMap<QoSClass, ClassStorageMetrics>,

    /// Number of evictions in the last cycle
    pub recent_evictions: usize,

    /// Bytes freed in the last cycle
    pub recent_bytes_freed: usize,
}

impl StorageMetrics {
    /// Get available storage in bytes
    pub fn available_bytes(&self) -> usize {
        self.max_bytes.saturating_sub(self.used_bytes)
    }

    /// Check if storage is under pressure (>80% full)
    pub fn under_pressure(&self) -> bool {
        self.utilization > 0.8
    }

    /// Check if storage is critical (>95% full)
    pub fn is_critical(&self) -> bool {
        self.utilization > 0.95
    }
}

/// QoS-aware storage manager
///
/// Tracks document storage and provides eviction candidate selection
/// based on QoS class, age, and storage pressure.
#[derive(Debug)]
pub struct QoSAwareStorage {
    /// Maximum storage capacity in bytes
    max_storage_bytes: usize,

    /// Current storage usage in bytes (atomic for lock-free reads)
    current_storage_bytes: AtomicUsize,

    /// Retention policies for each QoS class
    retention_policies: RetentionPolicies,

    /// Document tracking
    documents: RwLock<HashMap<String, StoredDocument>>,

    /// Eviction threshold (fraction, default 0.9)
    eviction_threshold: f32,
}

impl QoSAwareStorage {
    /// Create a new QoS-aware storage manager
    ///
    /// # Arguments
    /// * `max_storage_bytes` - Maximum storage capacity
    pub fn new(max_storage_bytes: usize) -> Self {
        Self {
            max_storage_bytes,
            current_storage_bytes: AtomicUsize::new(0),
            retention_policies: RetentionPolicies::default_tactical(),
            documents: RwLock::new(HashMap::new()),
            eviction_threshold: 0.9,
        }
    }

    /// Create with custom retention policies
    pub fn with_retention_policies(mut self, policies: RetentionPolicies) -> Self {
        self.retention_policies = policies;
        self
    }

    /// Set eviction threshold (fraction of storage that triggers eviction)
    pub fn with_eviction_threshold(mut self, threshold: f32) -> Self {
        self.eviction_threshold = threshold.clamp(0.5, 0.99);
        self
    }

    /// Register a document in storage
    pub fn register_document(&self, doc: StoredDocument) {
        let mut docs = self.documents.write().unwrap();

        // If replacing existing doc, adjust storage
        if let Some(existing) = docs.get(&doc.doc_id) {
            let old_size = existing.size_bytes;
            self.current_storage_bytes
                .fetch_sub(old_size, Ordering::Relaxed);
        }

        let size = doc.size_bytes;
        docs.insert(doc.doc_id.clone(), doc);
        self.current_storage_bytes
            .fetch_add(size, Ordering::Relaxed);
    }

    /// Remove a document from tracking
    pub fn unregister_document(&self, doc_id: &str) -> Option<StoredDocument> {
        let mut docs = self.documents.write().unwrap();
        if let Some(doc) = docs.remove(doc_id) {
            self.current_storage_bytes
                .fetch_sub(doc.size_bytes, Ordering::Relaxed);
            Some(doc)
        } else {
            None
        }
    }

    /// Mark a document as accessed (updates last_accessed timestamp)
    pub fn touch_document(&self, doc_id: &str) {
        let mut docs = self.documents.write().unwrap();
        if let Some(doc) = docs.get_mut(doc_id) {
            doc.touch();
        }
    }

    /// Mark a document as protected (never evict)
    pub fn mark_protected(&self, doc_id: &str) -> bool {
        let mut docs = self.documents.write().unwrap();
        if let Some(doc) = docs.get_mut(doc_id) {
            doc.protected = true;
            true
        } else {
            false
        }
    }

    /// Remove protection from a document
    pub fn unmark_protected(&self, doc_id: &str) -> bool {
        let mut docs = self.documents.write().unwrap();
        if let Some(doc) = docs.get_mut(doc_id) {
            doc.protected = false;
            true
        } else {
            false
        }
    }

    /// Update document size after compression
    pub fn update_compressed(&self, doc_id: &str, new_size: usize) -> Option<usize> {
        let mut docs = self.documents.write().unwrap();
        if let Some(doc) = docs.get_mut(doc_id) {
            let old_size = doc.size_bytes;
            let diff = old_size.saturating_sub(new_size);

            doc.size_bytes = new_size;
            doc.compressed = true;
            self.current_storage_bytes
                .fetch_sub(diff, Ordering::Relaxed);

            Some(diff)
        } else {
            None
        }
    }

    /// Get current storage pressure (0.0 - 1.0)
    pub fn storage_pressure(&self) -> f32 {
        let used = self.current_storage_bytes.load(Ordering::Relaxed);
        if self.max_storage_bytes == 0 {
            0.0
        } else {
            used as f32 / self.max_storage_bytes as f32
        }
    }

    /// Check if eviction should be triggered
    pub fn should_evict(&self) -> bool {
        self.storage_pressure() >= self.eviction_threshold
    }

    /// Get eviction candidates in priority order (P5 first, never P1)
    ///
    /// Returns documents sorted by eviction score (highest first).
    /// Critical (P1) documents are never included.
    pub fn get_eviction_candidates(&self, bytes_needed: usize) -> Vec<EvictionCandidate> {
        let pressure = self.storage_pressure();
        let docs = self.documents.read().unwrap();

        let mut candidates: Vec<EvictionCandidate> = docs
            .values()
            .filter(|doc| {
                // Never evict Critical (P1) documents
                if doc.qos_class == QoSClass::Critical {
                    return false;
                }
                // Never evict protected documents
                if doc.protected {
                    return false;
                }
                // Check retention policy
                let policy = self.retention_policies.get(doc.qos_class);
                policy.should_evict(doc.age_seconds(), pressure)
            })
            .map(|doc| {
                let score = self.calculate_eviction_score(doc, pressure);
                EvictionCandidate {
                    doc_id: doc.doc_id.clone(),
                    qos_class: doc.qos_class,
                    age_seconds: doc.age_seconds(),
                    size_bytes: doc.size_bytes,
                    eviction_score: score,
                }
            })
            .collect();

        // Sort by eviction score (highest first)
        candidates.sort_by(|a, b| {
            b.eviction_score
                .partial_cmp(&a.eviction_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Select enough candidates to free required bytes
        let mut total_bytes = 0;
        let mut selected = Vec::new();

        for candidate in candidates {
            selected.push(candidate.clone());
            total_bytes += candidate.size_bytes;
            if total_bytes >= bytes_needed {
                break;
            }
        }

        selected
    }

    /// Calculate eviction score for a document
    ///
    /// Higher score = more likely to be evicted.
    /// Factors:
    /// - QoS class (P5 highest score, P2 lowest)
    /// - Age (older = higher score)
    /// - Idle time (longer idle = higher score)
    fn calculate_eviction_score(&self, doc: &StoredDocument, pressure: f32) -> f64 {
        let policy = self.retention_policies.get(doc.qos_class);

        // Base score from QoS class (inverse of eviction priority)
        // P5=1, P4=2, P3=3, P2=4, P1=5 → scores 5, 4, 3, 2, 1
        let class_score = (6 - policy.eviction_priority) as f64;

        // Age factor (normalized to max retention)
        let age_factor = if policy.max_retain_seconds == u64::MAX {
            0.0 // Critical data, no age penalty
        } else {
            (doc.age_seconds() as f64 / policy.max_retain_seconds as f64).min(1.0)
        };

        // Idle factor (longer idle = higher score)
        let idle_factor = (doc.idle_seconds() as f64 / 3600.0).min(1.0); // Max 1 hour

        // Size factor (larger docs = slightly higher score to free more space)
        let size_factor = (doc.size_bytes as f64 / 1_000_000.0).min(1.0); // Max 1MB

        // Pressure factor (higher pressure = more aggressive eviction)
        let pressure_factor = pressure as f64;

        // Combine factors
        class_score * 10.0  // Class is primary factor
            + age_factor * 5.0
            + idle_factor * 3.0
            + size_factor * 1.0
            + pressure_factor * 2.0
    }

    /// Get documents eligible for compression
    pub fn get_compression_candidates(&self) -> Vec<String> {
        let pressure = self.storage_pressure();
        let docs = self.documents.read().unwrap();

        docs.values()
            .filter(|doc| {
                // Only compress non-critical, non-protected, uncompressed docs
                doc.qos_class != QoSClass::Critical
                    && !doc.protected
                    && !doc.compressed
                    && self
                        .retention_policies
                        .get(doc.qos_class)
                        .should_compress(pressure)
            })
            .map(|doc| doc.doc_id.clone())
            .collect()
    }

    /// Get storage metrics
    pub fn metrics(&self) -> StorageMetrics {
        let docs = self.documents.read().unwrap();
        let used = self.current_storage_bytes.load(Ordering::Relaxed);

        let mut by_class: HashMap<QoSClass, ClassStorageMetrics> = HashMap::new();

        for doc in docs.values() {
            let entry = by_class.entry(doc.qos_class).or_default();
            entry.doc_count += 1;
            entry.total_bytes += doc.size_bytes;
            entry.oldest_doc_age = entry.oldest_doc_age.max(doc.age_seconds());
            if doc.protected {
                entry.protected_count += 1;
            }
            if doc.compressed {
                entry.compressed_count += 1;
            }
        }

        // Calculate average ages
        for (class, metrics) in by_class.iter_mut() {
            if metrics.doc_count > 0 {
                let total_age: u64 = docs
                    .values()
                    .filter(|d| d.qos_class == *class)
                    .map(|d| d.age_seconds())
                    .sum();
                metrics.avg_age_seconds = total_age / metrics.doc_count as u64;
            }
        }

        StorageMetrics {
            max_bytes: self.max_storage_bytes,
            used_bytes: used,
            utilization: self.storage_pressure(),
            by_class,
            recent_evictions: 0,
            recent_bytes_freed: 0,
        }
    }

    /// Get count of tracked documents
    pub fn document_count(&self) -> usize {
        self.documents.read().unwrap().len()
    }

    /// Check if a document exists
    pub fn contains(&self, doc_id: &str) -> bool {
        self.documents.read().unwrap().contains_key(doc_id)
    }

    /// Get document info (read-only)
    pub fn get_document(&self, doc_id: &str) -> Option<StoredDocument> {
        self.documents.read().unwrap().get(doc_id).cloned()
    }

    /// Get max storage capacity
    pub fn max_storage_bytes(&self) -> usize {
        self.max_storage_bytes
    }

    /// Get current storage usage
    pub fn current_storage_bytes(&self) -> usize {
        self.current_storage_bytes.load(Ordering::Relaxed)
    }

    /// Get available storage
    pub fn available_bytes(&self) -> usize {
        self.max_storage_bytes
            .saturating_sub(self.current_storage_bytes.load(Ordering::Relaxed))
    }
}

impl Default for QoSAwareStorage {
    fn default() -> Self {
        // Default to 1GB storage
        Self::new(1024 * 1024 * 1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stored_document_creation() {
        let doc = StoredDocument::new("doc-123", QoSClass::Normal, 1024);

        assert_eq!(doc.doc_id, "doc-123");
        assert_eq!(doc.qos_class, QoSClass::Normal);
        assert_eq!(doc.size_bytes, 1024);
        assert!(!doc.protected);
        assert!(!doc.compressed);
        assert!(doc.stored_at > 0);
    }

    #[test]
    fn test_storage_registration() {
        let storage = QoSAwareStorage::new(1_000_000); // 1MB

        let doc1 = StoredDocument::new("doc-1", QoSClass::Normal, 10_000);
        let doc2 = StoredDocument::new("doc-2", QoSClass::Low, 20_000);

        storage.register_document(doc1);
        storage.register_document(doc2);

        assert_eq!(storage.document_count(), 2);
        assert_eq!(storage.current_storage_bytes(), 30_000);
        assert!(storage.contains("doc-1"));
        assert!(storage.contains("doc-2"));
    }

    #[test]
    fn test_storage_unregistration() {
        let storage = QoSAwareStorage::new(1_000_000);

        let doc = StoredDocument::new("doc-1", QoSClass::Normal, 10_000);
        storage.register_document(doc);

        assert_eq!(storage.current_storage_bytes(), 10_000);

        let removed = storage.unregister_document("doc-1");
        assert!(removed.is_some());
        assert_eq!(storage.current_storage_bytes(), 0);
        assert!(!storage.contains("doc-1"));
    }

    #[test]
    fn test_storage_pressure() {
        let storage = QoSAwareStorage::new(100_000); // 100KB

        // Empty storage
        assert_eq!(storage.storage_pressure(), 0.0);

        // 50% full
        let doc = StoredDocument::new("doc-1", QoSClass::Normal, 50_000);
        storage.register_document(doc);
        assert!((storage.storage_pressure() - 0.5).abs() < 0.01);

        // 90% full
        let doc2 = StoredDocument::new("doc-2", QoSClass::Normal, 40_000);
        storage.register_document(doc2);
        assert!((storage.storage_pressure() - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_should_evict_threshold() {
        let storage = QoSAwareStorage::new(100_000).with_eviction_threshold(0.8);

        // Below threshold
        let doc1 = StoredDocument::new("doc-1", QoSClass::Bulk, 70_000);
        storage.register_document(doc1);
        assert!(!storage.should_evict());

        // Above threshold
        let doc2 = StoredDocument::new("doc-2", QoSClass::Bulk, 15_000);
        storage.register_document(doc2);
        assert!(storage.should_evict());
    }

    #[test]
    fn test_eviction_candidates_exclude_critical() {
        let storage = QoSAwareStorage::new(100_000).with_eviction_threshold(0.5);

        // Fill storage with mix of classes - documents old enough to be evicted
        // Using ages > max_retain so they're evicted regardless of pressure
        // Bulk max_retain: 300s, Low max_retain: 3600s
        storage.register_document(
            StoredDocument::new("critical-1", QoSClass::Critical, 10_000).with_age(1000),
        );
        storage
            .register_document(StoredDocument::new("bulk-1", QoSClass::Bulk, 10_000).with_age(400)); // > 300s max
        storage
            .register_document(StoredDocument::new("low-1", QoSClass::Low, 10_000).with_age(4000)); // > 3600s max

        // Get candidates to free 20KB
        let candidates = storage.get_eviction_candidates(20_000);

        // Critical should never be included
        assert!(!candidates.iter().any(|c| c.qos_class == QoSClass::Critical));

        // Should include Bulk first (highest eviction priority)
        assert!(candidates.iter().any(|c| c.doc_id == "bulk-1"));
    }

    #[test]
    fn test_eviction_candidates_exclude_protected() {
        let storage = QoSAwareStorage::new(100_000).with_eviction_threshold(0.5);

        // Documents old enough to be evicted (Bulk max_retain: 300s)
        storage
            .register_document(StoredDocument::new("doc-1", QoSClass::Bulk, 10_000).with_age(400));
        storage
            .register_document(StoredDocument::new("doc-2", QoSClass::Bulk, 10_000).with_age(400));

        // Protect one document
        storage.mark_protected("doc-1");

        let candidates = storage.get_eviction_candidates(20_000);

        // Protected doc should not be included
        assert!(!candidates.iter().any(|c| c.doc_id == "doc-1"));
        assert!(candidates.iter().any(|c| c.doc_id == "doc-2"));
    }

    #[test]
    fn test_eviction_priority_order() {
        let storage = QoSAwareStorage::new(100_000).with_eviction_threshold(0.3);

        // Add documents of different classes - ages exceed max_retain to ensure eviction
        // High max: 7 days, Normal max: 24h, Low max: 1h, Bulk max: 5min
        storage.register_document(
            StoredDocument::new("high-1", QoSClass::High, 5_000).with_age(700000),
        ); // > 7 days
        storage.register_document(
            StoredDocument::new("normal-1", QoSClass::Normal, 5_000).with_age(100000),
        ); // > 24h
        storage
            .register_document(StoredDocument::new("low-1", QoSClass::Low, 5_000).with_age(4000)); // > 1h
        storage
            .register_document(StoredDocument::new("bulk-1", QoSClass::Bulk, 5_000).with_age(400)); // > 5min

        let candidates = storage.get_eviction_candidates(20_000);

        // Should have candidates now
        assert!(!candidates.is_empty(), "Expected eviction candidates");

        // Bulk should have highest eviction score
        if candidates.len() >= 2 {
            assert!(candidates[0].eviction_score >= candidates[1].eviction_score);
        }

        // First candidate should be Bulk (P5)
        assert_eq!(candidates[0].qos_class, QoSClass::Bulk);
    }

    #[test]
    fn test_compression_candidates() {
        let storage = QoSAwareStorage::new(100_000).with_eviction_threshold(0.5);

        // Fill to 80% to trigger compression eligibility
        storage.register_document(StoredDocument::new(
            "critical-1",
            QoSClass::Critical,
            10_000,
        ));
        storage.register_document(StoredDocument::new("bulk-1", QoSClass::Bulk, 70_000));

        let candidates = storage.get_compression_candidates();

        // Critical should not be eligible for compression
        assert!(!candidates.contains(&"critical-1".to_string()));
        // Bulk should be eligible
        assert!(candidates.contains(&"bulk-1".to_string()));
    }

    #[test]
    fn test_update_compressed() {
        let storage = QoSAwareStorage::new(100_000);

        storage.register_document(StoredDocument::new("doc-1", QoSClass::Normal, 10_000));
        assert_eq!(storage.current_storage_bytes(), 10_000);

        // Compress to 6KB (40% reduction)
        let saved = storage.update_compressed("doc-1", 6_000);
        assert_eq!(saved, Some(4_000));
        assert_eq!(storage.current_storage_bytes(), 6_000);

        let doc = storage.get_document("doc-1").unwrap();
        assert!(doc.compressed);
        assert_eq!(doc.size_bytes, 6_000);
    }

    #[test]
    fn test_document_replacement() {
        let storage = QoSAwareStorage::new(100_000);

        storage.register_document(StoredDocument::new("doc-1", QoSClass::Normal, 10_000));
        assert_eq!(storage.current_storage_bytes(), 10_000);

        // Replace with larger document
        storage.register_document(StoredDocument::new("doc-1", QoSClass::Normal, 15_000));
        assert_eq!(storage.current_storage_bytes(), 15_000);
        assert_eq!(storage.document_count(), 1);
    }

    #[test]
    fn test_storage_metrics() {
        let storage = QoSAwareStorage::new(100_000);

        storage.register_document(StoredDocument::new("bulk-1", QoSClass::Bulk, 10_000));
        storage.register_document(StoredDocument::new("bulk-2", QoSClass::Bulk, 15_000));
        storage.register_document(StoredDocument::new("normal-1", QoSClass::Normal, 20_000));
        storage.mark_protected("normal-1");

        let metrics = storage.metrics();

        assert_eq!(metrics.max_bytes, 100_000);
        assert_eq!(metrics.used_bytes, 45_000);
        assert!((metrics.utilization - 0.45).abs() < 0.01);

        let bulk_metrics = metrics.by_class.get(&QoSClass::Bulk).unwrap();
        assert_eq!(bulk_metrics.doc_count, 2);
        assert_eq!(bulk_metrics.total_bytes, 25_000);

        let normal_metrics = metrics.by_class.get(&QoSClass::Normal).unwrap();
        assert_eq!(normal_metrics.protected_count, 1);
    }

    #[test]
    fn test_touch_document() {
        let storage = QoSAwareStorage::new(100_000);

        let doc = StoredDocument::new("doc-1", QoSClass::Normal, 10_000);
        let original_last_accessed = doc.last_accessed;
        storage.register_document(doc);

        // Wait a tiny bit and touch
        std::thread::sleep(std::time::Duration::from_millis(10));
        storage.touch_document("doc-1");

        let updated_doc = storage.get_document("doc-1").unwrap();
        assert!(updated_doc.last_accessed >= original_last_accessed);
    }

    #[test]
    fn test_available_bytes() {
        let storage = QoSAwareStorage::new(100_000);

        assert_eq!(storage.available_bytes(), 100_000);

        storage.register_document(StoredDocument::new("doc-1", QoSClass::Normal, 30_000));
        assert_eq!(storage.available_bytes(), 70_000);
    }

    #[test]
    fn test_storage_metrics_helper_methods() {
        let metrics = StorageMetrics {
            max_bytes: 100_000,
            used_bytes: 85_000,
            utilization: 0.85,
            by_class: HashMap::new(),
            recent_evictions: 0,
            recent_bytes_freed: 0,
        };

        assert_eq!(metrics.available_bytes(), 15_000);
        assert!(metrics.under_pressure());
        assert!(!metrics.is_critical());

        let critical_metrics = StorageMetrics {
            utilization: 0.97,
            ..metrics
        };
        assert!(critical_metrics.is_critical());
    }
}
