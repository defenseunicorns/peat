//! Eviction audit logging (ADR-019 Phase 4)
//!
//! Provides audit logging for all storage eviction and lifecycle events,
//! enabling post-mission review and operational debugging.
//!
//! # Features
//!
//! - Thread-safe audit log with configurable retention
//! - Structured entries with timestamp, action, and reason
//! - Query support for filtering by time range or QoS class
//! - Export capability for post-mission analysis

use super::QoSClass;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::RwLock;

/// Actions that can be audited
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuditAction {
    /// Document was evicted from storage
    Evicted,
    /// Document was compressed to save space
    Compressed,
    /// Document was marked as protected (never evict)
    MarkedProtected,
    /// Document protection was removed
    UnmarkedProtected,
    /// Eviction was attempted but failed
    FailedEviction,
    /// Document exceeded TTL and was removed
    TtlExpired,
    /// Document was soft-deleted (marked deleted but not removed)
    SoftDeleted,
    /// Storage cleanup cycle completed
    CleanupCompleted,
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Evicted => write!(f, "EVICTED"),
            Self::Compressed => write!(f, "COMPRESSED"),
            Self::MarkedProtected => write!(f, "PROTECTED"),
            Self::UnmarkedProtected => write!(f, "UNPROTECTED"),
            Self::FailedEviction => write!(f, "EVICT_FAILED"),
            Self::TtlExpired => write!(f, "TTL_EXPIRED"),
            Self::SoftDeleted => write!(f, "SOFT_DELETED"),
            Self::CleanupCompleted => write!(f, "CLEANUP_DONE"),
        }
    }
}

/// A single audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Timestamp when the action occurred
    pub timestamp: DateTime<Utc>,

    /// The action that was taken
    pub action: AuditAction,

    /// Document or item identifier
    pub doc_id: String,

    /// QoS class of the affected document
    pub qos_class: QoSClass,

    /// Size in bytes of the affected document (if known)
    pub size_bytes: Option<usize>,

    /// Human-readable reason for the action
    pub reason: String,

    /// Additional metadata (optional)
    pub metadata: Option<String>,
}

impl AuditEntry {
    /// Create a new audit entry
    pub fn new(
        action: AuditAction,
        doc_id: impl Into<String>,
        qos_class: QoSClass,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            action,
            doc_id: doc_id.into(),
            qos_class,
            size_bytes: None,
            reason: reason.into(),
            metadata: None,
        }
    }

    /// Add size information
    pub fn with_size(mut self, size_bytes: usize) -> Self {
        self.size_bytes = Some(size_bytes);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, metadata: impl Into<String>) -> Self {
        self.metadata = Some(metadata.into());
        self
    }

    /// Create an eviction entry
    pub fn eviction(
        doc_id: impl Into<String>,
        qos_class: QoSClass,
        reason: impl Into<String>,
    ) -> Self {
        Self::new(AuditAction::Evicted, doc_id, qos_class, reason)
    }

    /// Create a compression entry
    pub fn compression(
        doc_id: impl Into<String>,
        qos_class: QoSClass,
        original_size: usize,
        compressed_size: usize,
    ) -> Self {
        let savings = if original_size > 0 {
            ((original_size - compressed_size) as f64 / original_size as f64 * 100.0) as u32
        } else {
            0
        };
        Self::new(
            AuditAction::Compressed,
            doc_id,
            qos_class,
            format!(
                "Compressed {} -> {} bytes ({}% reduction)",
                original_size, compressed_size, savings
            ),
        )
        .with_size(compressed_size)
    }

    /// Create a protection entry
    pub fn protected(
        doc_id: impl Into<String>,
        qos_class: QoSClass,
        reason: impl Into<String>,
    ) -> Self {
        Self::new(AuditAction::MarkedProtected, doc_id, qos_class, reason)
    }

    /// Create a failed eviction entry
    pub fn failed_eviction(
        doc_id: impl Into<String>,
        qos_class: QoSClass,
        error: impl Into<String>,
    ) -> Self {
        Self::new(AuditAction::FailedEviction, doc_id, qos_class, error)
    }

    /// Create a TTL expiration entry
    pub fn ttl_expired(doc_id: impl Into<String>, qos_class: QoSClass, age_seconds: u64) -> Self {
        Self::new(
            AuditAction::TtlExpired,
            doc_id,
            qos_class,
            format!("Document exceeded TTL (age: {} seconds)", age_seconds),
        )
    }

    /// Create a cleanup completion entry
    pub fn cleanup_completed(docs_evicted: usize, bytes_freed: usize, duration_ms: u64) -> Self {
        Self::new(
            AuditAction::CleanupCompleted,
            "system",
            QoSClass::Normal,
            format!(
                "Evicted {} docs, freed {} bytes in {}ms",
                docs_evicted, bytes_freed, duration_ms
            ),
        )
        .with_size(bytes_freed)
    }
}

/// Thread-safe eviction audit log
///
/// Maintains a bounded log of eviction events for post-mission analysis.
#[derive(Debug)]
pub struct EvictionAuditLog {
    entries: RwLock<VecDeque<AuditEntry>>,
    max_entries: usize,
}

impl EvictionAuditLog {
    /// Create a new audit log with specified capacity
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: RwLock::new(VecDeque::with_capacity(max_entries)),
            max_entries,
        }
    }

    /// Create an audit log with default capacity (10,000 entries)
    pub fn default_capacity() -> Self {
        Self::new(10_000)
    }

    /// Record an audit entry
    pub fn record(&self, entry: AuditEntry) {
        let mut entries = self.entries.write().unwrap();
        if entries.len() >= self.max_entries {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    /// Record an eviction event
    pub fn record_eviction(
        &self,
        doc_id: impl Into<String>,
        qos_class: QoSClass,
        size_bytes: usize,
        reason: impl Into<String>,
    ) {
        self.record(AuditEntry::eviction(doc_id, qos_class, reason).with_size(size_bytes));
    }

    /// Record a compression event
    pub fn record_compression(
        &self,
        doc_id: impl Into<String>,
        qos_class: QoSClass,
        original_size: usize,
        compressed_size: usize,
    ) {
        self.record(AuditEntry::compression(
            doc_id,
            qos_class,
            original_size,
            compressed_size,
        ));
    }

    /// Record a protection event
    pub fn record_protection(
        &self,
        doc_id: impl Into<String>,
        qos_class: QoSClass,
        reason: impl Into<String>,
    ) {
        self.record(AuditEntry::protected(doc_id, qos_class, reason));
    }

    /// Record a failed eviction
    pub fn record_failure(
        &self,
        doc_id: impl Into<String>,
        qos_class: QoSClass,
        error: impl Into<String>,
    ) {
        self.record(AuditEntry::failed_eviction(doc_id, qos_class, error));
    }

    /// Get all entries (clone)
    pub fn get_all(&self) -> Vec<AuditEntry> {
        self.entries.read().unwrap().iter().cloned().collect()
    }

    /// Get entries within a time range
    pub fn get_in_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<AuditEntry> {
        self.entries
            .read()
            .unwrap()
            .iter()
            .filter(|e| e.timestamp >= start && e.timestamp <= end)
            .cloned()
            .collect()
    }

    /// Get entries for a specific QoS class
    pub fn get_by_class(&self, class: QoSClass) -> Vec<AuditEntry> {
        self.entries
            .read()
            .unwrap()
            .iter()
            .filter(|e| e.qos_class == class)
            .cloned()
            .collect()
    }

    /// Get entries for a specific action type
    pub fn get_by_action(&self, action: AuditAction) -> Vec<AuditEntry> {
        self.entries
            .read()
            .unwrap()
            .iter()
            .filter(|e| e.action == action)
            .cloned()
            .collect()
    }

    /// Get recent entries (last N)
    pub fn get_recent(&self, count: usize) -> Vec<AuditEntry> {
        let entries = self.entries.read().unwrap();
        entries
            .iter()
            .rev()
            .take(count)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// Get number of entries
    pub fn len(&self) -> usize {
        self.entries.read().unwrap().len()
    }

    /// Check if log is empty
    pub fn is_empty(&self) -> bool {
        self.entries.read().unwrap().is_empty()
    }

    /// Clear all entries
    pub fn clear(&self) {
        self.entries.write().unwrap().clear();
    }

    /// Get summary statistics
    pub fn summary(&self) -> AuditSummary {
        let entries = self.entries.read().unwrap();
        let mut summary = AuditSummary::default();

        for entry in entries.iter() {
            summary.total_entries += 1;

            match entry.action {
                AuditAction::Evicted => {
                    summary.total_evictions += 1;
                    if let Some(size) = entry.size_bytes {
                        summary.bytes_evicted += size;
                    }
                    *summary
                        .evictions_by_class
                        .entry(entry.qos_class)
                        .or_insert(0) += 1;
                }
                AuditAction::Compressed => {
                    summary.total_compressions += 1;
                }
                AuditAction::FailedEviction => {
                    summary.failed_evictions += 1;
                }
                _ => {}
            }
        }

        summary
    }

    /// Export log as JSON string
    pub fn export_json(&self) -> Result<String, serde_json::Error> {
        let entries = self.get_all();
        serde_json::to_string_pretty(&entries)
    }
}

impl Default for EvictionAuditLog {
    fn default() -> Self {
        Self::default_capacity()
    }
}

/// Summary statistics from the audit log
#[derive(Debug, Clone, Default)]
pub struct AuditSummary {
    /// Total number of log entries
    pub total_entries: usize,
    /// Total eviction events
    pub total_evictions: usize,
    /// Total bytes evicted
    pub bytes_evicted: usize,
    /// Total compression events
    pub total_compressions: usize,
    /// Failed eviction attempts
    pub failed_evictions: usize,
    /// Evictions broken down by QoS class
    pub evictions_by_class: std::collections::HashMap<QoSClass, usize>,
}

impl AuditSummary {
    /// Get eviction count for a specific class
    pub fn evictions_for_class(&self, class: QoSClass) -> usize {
        *self.evictions_by_class.get(&class).unwrap_or(&0)
    }

    /// Calculate eviction success rate
    pub fn eviction_success_rate(&self) -> f64 {
        let total_attempts = self.total_evictions + self.failed_evictions;
        if total_attempts == 0 {
            1.0
        } else {
            self.total_evictions as f64 / total_attempts as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_audit_entry_creation() {
        let entry = AuditEntry::eviction("doc-123", QoSClass::Low, "Storage pressure");

        assert_eq!(entry.action, AuditAction::Evicted);
        assert_eq!(entry.doc_id, "doc-123");
        assert_eq!(entry.qos_class, QoSClass::Low);
        assert!(entry.reason.contains("Storage pressure"));
    }

    #[test]
    fn test_audit_entry_with_size() {
        let entry = AuditEntry::eviction("doc-123", QoSClass::Bulk, "TTL expired").with_size(1024);

        assert_eq!(entry.size_bytes, Some(1024));
    }

    #[test]
    fn test_compression_entry() {
        let entry = AuditEntry::compression("doc-456", QoSClass::Normal, 1000, 600);

        assert_eq!(entry.action, AuditAction::Compressed);
        assert!(entry.reason.contains("40%")); // 40% reduction
        assert_eq!(entry.size_bytes, Some(600));
    }

    #[test]
    fn test_ttl_expired_entry() {
        let entry = AuditEntry::ttl_expired("doc-789", QoSClass::Low, 3600);

        assert_eq!(entry.action, AuditAction::TtlExpired);
        assert!(entry.reason.contains("3600"));
    }

    #[test]
    fn test_cleanup_completed_entry() {
        let entry = AuditEntry::cleanup_completed(100, 1_000_000, 250);

        assert_eq!(entry.action, AuditAction::CleanupCompleted);
        assert!(entry.reason.contains("100 docs"));
        assert!(entry.reason.contains("1000000 bytes"));
    }

    #[test]
    fn test_audit_log_basic_operations() {
        let log = EvictionAuditLog::new(100);

        log.record_eviction("doc-1", QoSClass::Bulk, 500, "Storage full");
        log.record_eviction("doc-2", QoSClass::Low, 1000, "TTL expired");

        assert_eq!(log.len(), 2);
        assert!(!log.is_empty());

        let entries = log.get_all();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_audit_log_max_capacity() {
        let log = EvictionAuditLog::new(3);

        log.record_eviction("doc-1", QoSClass::Bulk, 100, "reason1");
        log.record_eviction("doc-2", QoSClass::Bulk, 100, "reason2");
        log.record_eviction("doc-3", QoSClass::Bulk, 100, "reason3");
        log.record_eviction("doc-4", QoSClass::Bulk, 100, "reason4");

        // Should have dropped doc-1
        assert_eq!(log.len(), 3);
        let entries = log.get_all();
        assert_eq!(entries[0].doc_id, "doc-2");
        assert_eq!(entries[2].doc_id, "doc-4");
    }

    #[test]
    fn test_get_by_class() {
        let log = EvictionAuditLog::new(100);

        log.record_eviction("doc-1", QoSClass::Bulk, 100, "reason");
        log.record_eviction("doc-2", QoSClass::Low, 200, "reason");
        log.record_eviction("doc-3", QoSClass::Bulk, 300, "reason");

        let bulk_entries = log.get_by_class(QoSClass::Bulk);
        assert_eq!(bulk_entries.len(), 2);

        let low_entries = log.get_by_class(QoSClass::Low);
        assert_eq!(low_entries.len(), 1);

        let critical_entries = log.get_by_class(QoSClass::Critical);
        assert!(critical_entries.is_empty());
    }

    #[test]
    fn test_get_by_action() {
        let log = EvictionAuditLog::new(100);

        log.record_eviction("doc-1", QoSClass::Bulk, 100, "reason");
        log.record_compression("doc-2", QoSClass::Low, 1000, 500);
        log.record_failure("doc-3", QoSClass::Normal, "IO error");

        let evictions = log.get_by_action(AuditAction::Evicted);
        assert_eq!(evictions.len(), 1);

        let compressions = log.get_by_action(AuditAction::Compressed);
        assert_eq!(compressions.len(), 1);

        let failures = log.get_by_action(AuditAction::FailedEviction);
        assert_eq!(failures.len(), 1);
    }

    #[test]
    fn test_get_recent() {
        let log = EvictionAuditLog::new(100);

        for i in 1..=5 {
            log.record_eviction(format!("doc-{}", i), QoSClass::Bulk, 100, "reason");
        }

        let recent = log.get_recent(3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].doc_id, "doc-3");
        assert_eq!(recent[2].doc_id, "doc-5");
    }

    #[test]
    fn test_get_in_range() {
        let log = EvictionAuditLog::new(100);

        // Record entries
        log.record_eviction("doc-1", QoSClass::Bulk, 100, "reason");

        let now = Utc::now();
        let start = now - Duration::seconds(1);
        let end = now + Duration::seconds(1);

        let in_range = log.get_in_range(start, end);
        assert_eq!(in_range.len(), 1);

        // Outside range
        let old_start = now - Duration::hours(1);
        let old_end = now - Duration::minutes(30);
        let out_of_range = log.get_in_range(old_start, old_end);
        assert!(out_of_range.is_empty());
    }

    #[test]
    fn test_summary() {
        let log = EvictionAuditLog::new(100);

        log.record_eviction("doc-1", QoSClass::Bulk, 500, "reason");
        log.record_eviction("doc-2", QoSClass::Bulk, 300, "reason");
        log.record_eviction("doc-3", QoSClass::Low, 200, "reason");
        log.record_compression("doc-4", QoSClass::Normal, 1000, 500);
        log.record_failure("doc-5", QoSClass::Normal, "IO error");

        let summary = log.summary();

        assert_eq!(summary.total_entries, 5);
        assert_eq!(summary.total_evictions, 3);
        assert_eq!(summary.bytes_evicted, 1000); // 500 + 300 + 200
        assert_eq!(summary.total_compressions, 1);
        assert_eq!(summary.failed_evictions, 1);
        assert_eq!(summary.evictions_for_class(QoSClass::Bulk), 2);
        assert_eq!(summary.evictions_for_class(QoSClass::Low), 1);
        assert_eq!(summary.evictions_for_class(QoSClass::Critical), 0);
    }

    #[test]
    fn test_eviction_success_rate() {
        let summary = AuditSummary {
            total_evictions: 90,
            failed_evictions: 10,
            ..Default::default()
        };

        assert!((summary.eviction_success_rate() - 0.9).abs() < 0.001);

        // Empty case
        let empty_summary = AuditSummary::default();
        assert_eq!(empty_summary.eviction_success_rate(), 1.0);
    }

    #[test]
    fn test_clear() {
        let log = EvictionAuditLog::new(100);

        log.record_eviction("doc-1", QoSClass::Bulk, 100, "reason");
        log.record_eviction("doc-2", QoSClass::Low, 200, "reason");

        assert_eq!(log.len(), 2);

        log.clear();
        assert!(log.is_empty());
    }

    #[test]
    fn test_export_json() {
        let log = EvictionAuditLog::new(100);

        log.record_eviction("doc-1", QoSClass::Bulk, 100, "test reason");

        let json = log.export_json().unwrap();
        assert!(json.contains("doc-1"));
        assert!(json.contains("Bulk"));
        assert!(json.contains("test reason"));
    }

    #[test]
    fn test_action_display() {
        assert_eq!(AuditAction::Evicted.to_string(), "EVICTED");
        assert_eq!(AuditAction::Compressed.to_string(), "COMPRESSED");
        assert_eq!(AuditAction::MarkedProtected.to_string(), "PROTECTED");
        assert_eq!(AuditAction::FailedEviction.to_string(), "EVICT_FAILED");
    }
}
