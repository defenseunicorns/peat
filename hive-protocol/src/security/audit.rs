//! # Audit Logging Module - Security Event Tracking for HIVE Protocol
//!
//! Implements ADR-006 Layer 7: Audit Logging.
//!
//! ## Overview
//!
//! Provides comprehensive logging of all security-relevant events for forensics
//! and compliance with NIST SP 800-53 AU (Audit) controls.
//!
//! ## Event Types
//!
//! - **Authentication**: Device/user authentication attempts
//! - **Authorization**: Permission grants and denials
//! - **DataAccess**: Read/write operations on sensitive data
//! - **CellFormation**: Cell join/leave/formation events
//! - **KeyExchange**: Cryptographic key operations
//! - **SecurityViolation**: Detected security anomalies
//!
//! ## Usage
//!
//! ```ignore
//! use hive_protocol::security::{AuditLogger, FileAuditLogger, AuditEventType};
//!
//! // Create file-based audit logger
//! let logger = FileAuditLogger::new("/var/log/hive/audit.log")?;
//!
//! // Log authentication event
//! logger.log_authentication(
//!     "device:abc123",
//!     true,
//!     Some("challenge-response verified"),
//! );
//!
//! // Log authorization denial
//! logger.log_denial(
//!     "device:abc123",
//!     "SetCellLeader",
//!     "cell:squad-1",
//!     "role=Member, required=Leader",
//! );
//! ```

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::authorization::Permission;
use super::error::SecurityError;

/// Audit event types for categorization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    /// Device or user authentication attempt
    Authentication,
    /// Authorization check (permission grant/denial)
    Authorization,
    /// Data access operation (read)
    DataAccess,
    /// Data modification operation (write)
    DataModification,
    /// Cryptographic key exchange
    KeyExchange,
    /// Cell formation or membership change
    CellFormation,
    /// Leader election or change
    LeaderElection,
    /// Detected security violation
    SecurityViolation,
    /// Session management (create/expire/invalidate)
    SessionManagement,
    /// Encryption operation
    Encryption,
}

impl std::fmt::Display for AuditEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditEventType::Authentication => write!(f, "AUTHENTICATION"),
            AuditEventType::Authorization => write!(f, "AUTHORIZATION"),
            AuditEventType::DataAccess => write!(f, "DATA_ACCESS"),
            AuditEventType::DataModification => write!(f, "DATA_MODIFICATION"),
            AuditEventType::KeyExchange => write!(f, "KEY_EXCHANGE"),
            AuditEventType::CellFormation => write!(f, "CELL_FORMATION"),
            AuditEventType::LeaderElection => write!(f, "LEADER_ELECTION"),
            AuditEventType::SecurityViolation => write!(f, "SECURITY_VIOLATION"),
            AuditEventType::SessionManagement => write!(f, "SESSION_MANAGEMENT"),
            AuditEventType::Encryption => write!(f, "ENCRYPTION"),
        }
    }
}

/// Security violation types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityViolation {
    /// Invalid signature detected
    InvalidSignature,
    /// Replay attack detected
    ReplayAttack,
    /// Unauthorized access attempt
    UnauthorizedAccess,
    /// Certificate validation failed
    CertificateError,
    /// Tampered message detected
    TamperedMessage,
    /// Rate limit exceeded
    RateLimitExceeded,
    /// Unknown device attempted join
    UnknownDevice,
    /// Expired credentials used
    ExpiredCredentials,
    /// Protocol violation
    ProtocolViolation,
}

impl std::fmt::Display for SecurityViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecurityViolation::InvalidSignature => write!(f, "INVALID_SIGNATURE"),
            SecurityViolation::ReplayAttack => write!(f, "REPLAY_ATTACK"),
            SecurityViolation::UnauthorizedAccess => write!(f, "UNAUTHORIZED_ACCESS"),
            SecurityViolation::CertificateError => write!(f, "CERTIFICATE_ERROR"),
            SecurityViolation::TamperedMessage => write!(f, "TAMPERED_MESSAGE"),
            SecurityViolation::RateLimitExceeded => write!(f, "RATE_LIMIT_EXCEEDED"),
            SecurityViolation::UnknownDevice => write!(f, "UNKNOWN_DEVICE"),
            SecurityViolation::ExpiredCredentials => write!(f, "EXPIRED_CREDENTIALS"),
            SecurityViolation::ProtocolViolation => write!(f, "PROTOCOL_VIOLATION"),
        }
    }
}

/// Audit log entry (JSON-serializable)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    /// Unix timestamp (seconds since epoch)
    pub timestamp: u64,
    /// ISO 8601 formatted timestamp for readability
    pub timestamp_iso: String,
    /// Event type category
    pub event_type: AuditEventType,
    /// Entity performing the action (device ID or user ID)
    pub entity_id: String,
    /// Whether the action succeeded
    pub success: bool,
    /// Human-readable description
    pub description: String,
    /// Additional context (key-value pairs)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub context: HashMap<String, String>,
    /// Sequence number for ordering
    pub sequence: u64,
}

impl AuditLogEntry {
    /// Create new audit log entry
    pub fn new(
        event_type: AuditEventType,
        entity_id: impl Into<String>,
        success: bool,
        description: impl Into<String>,
        sequence: u64,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            timestamp: now,
            timestamp_iso: format_timestamp(now),
            event_type,
            entity_id: entity_id.into(),
            success,
            description: description.into(),
            context: HashMap::new(),
            sequence,
        }
    }

    /// Add context key-value pair
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }

    /// Serialize to JSON line
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            format!(
                "{{\"error\":\"serialization failed for seq {}\"}}",
                self.sequence
            )
        })
    }
}

/// Format Unix timestamp as ISO 8601
fn format_timestamp(secs: u64) -> String {
    // Simple ISO 8601 format without external dependencies
    let dt = chrono::DateTime::from_timestamp(secs as i64, 0)
        .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).unwrap());
    dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Audit logger trait for security event tracking
pub trait AuditLogger: Send + Sync {
    /// Log authentication event
    fn log_authentication(&self, entity_id: &str, success: bool, reason: Option<&str>);

    /// Log authorization grant
    fn log_grant(&self, entity_id: &str, permission: Permission, target: &str);

    /// Log authorization denial
    fn log_denial(&self, entity_id: &str, permission: &str, target: &str, reason: &str);

    /// Log data operation
    fn log_operation(&self, entity_id: &str, operation: &str, target: &str, success: bool);

    /// Log security violation
    fn log_violation(&self, entity_id: &str, violation: SecurityViolation, details: &str);

    /// Log cell formation event
    fn log_cell_event(&self, entity_id: &str, cell_id: &str, action: &str, success: bool);

    /// Log key exchange event
    fn log_key_exchange(&self, entity_id: &str, peer_id: &str, success: bool);

    /// Log session event (create, expire, invalidate)
    fn log_session(&self, entity_id: &str, session_id: &str, action: &str, success: bool);

    /// Log encryption event
    fn log_encryption(&self, entity_id: &str, operation: &str, target: &str, success: bool);

    /// Get the number of entries logged
    fn entry_count(&self) -> u64;

    /// Flush any buffered entries
    fn flush(&self) -> Result<(), SecurityError>;
}

/// In-memory audit logger (for testing)
#[derive(Debug)]
pub struct MemoryAuditLogger {
    entries: Arc<Mutex<Vec<AuditLogEntry>>>,
    sequence: Arc<Mutex<u64>>,
}

impl MemoryAuditLogger {
    /// Create new in-memory logger
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(Vec::new())),
            sequence: Arc::new(Mutex::new(0)),
        }
    }

    /// Get all logged entries
    pub fn entries(&self) -> Vec<AuditLogEntry> {
        self.entries.lock().unwrap().clone()
    }

    /// Get entries filtered by event type
    pub fn entries_by_type(&self, event_type: AuditEventType) -> Vec<AuditLogEntry> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.event_type == event_type)
            .cloned()
            .collect()
    }

    /// Clear all entries
    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }

    fn next_sequence(&self) -> u64 {
        let mut seq = self.sequence.lock().unwrap();
        *seq += 1;
        *seq
    }

    fn add_entry(&self, entry: AuditLogEntry) {
        self.entries.lock().unwrap().push(entry);
    }
}

impl Default for MemoryAuditLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl AuditLogger for MemoryAuditLogger {
    fn log_authentication(&self, entity_id: &str, success: bool, reason: Option<&str>) {
        let description = match (success, reason) {
            (true, Some(r)) => format!("Authentication succeeded: {}", r),
            (true, None) => "Authentication succeeded".to_string(),
            (false, Some(r)) => format!("Authentication failed: {}", r),
            (false, None) => "Authentication failed".to_string(),
        };

        let entry = AuditLogEntry::new(
            AuditEventType::Authentication,
            entity_id,
            success,
            description,
            self.next_sequence(),
        );
        self.add_entry(entry);
    }

    fn log_grant(&self, entity_id: &str, permission: Permission, target: &str) {
        let entry = AuditLogEntry::new(
            AuditEventType::Authorization,
            entity_id,
            true,
            format!("Permission granted: {:?}", permission),
            self.next_sequence(),
        )
        .with_context("permission", format!("{:?}", permission))
        .with_context("target", target);
        self.add_entry(entry);
    }

    fn log_denial(&self, entity_id: &str, permission: &str, target: &str, reason: &str) {
        let entry = AuditLogEntry::new(
            AuditEventType::Authorization,
            entity_id,
            false,
            format!("Permission denied: {} - {}", permission, reason),
            self.next_sequence(),
        )
        .with_context("permission", permission)
        .with_context("target", target)
        .with_context("reason", reason);
        self.add_entry(entry);
    }

    fn log_operation(&self, entity_id: &str, operation: &str, target: &str, success: bool) {
        let event_type = if operation.starts_with("read")
            || operation.starts_with("get")
            || operation.starts_with("query")
        {
            AuditEventType::DataAccess
        } else {
            AuditEventType::DataModification
        };

        let entry = AuditLogEntry::new(
            event_type,
            entity_id,
            success,
            format!("{} on {}", operation, target),
            self.next_sequence(),
        )
        .with_context("operation", operation)
        .with_context("target", target);
        self.add_entry(entry);
    }

    fn log_violation(&self, entity_id: &str, violation: SecurityViolation, details: &str) {
        let entry = AuditLogEntry::new(
            AuditEventType::SecurityViolation,
            entity_id,
            false,
            format!("Security violation: {} - {}", violation, details),
            self.next_sequence(),
        )
        .with_context("violation_type", violation.to_string())
        .with_context("details", details);
        self.add_entry(entry);
    }

    fn log_cell_event(&self, entity_id: &str, cell_id: &str, action: &str, success: bool) {
        let entry = AuditLogEntry::new(
            AuditEventType::CellFormation,
            entity_id,
            success,
            format!("Cell {}: {}", action, cell_id),
            self.next_sequence(),
        )
        .with_context("cell_id", cell_id)
        .with_context("action", action);
        self.add_entry(entry);
    }

    fn log_key_exchange(&self, entity_id: &str, peer_id: &str, success: bool) {
        let entry = AuditLogEntry::new(
            AuditEventType::KeyExchange,
            entity_id,
            success,
            format!("Key exchange with peer: {}", peer_id),
            self.next_sequence(),
        )
        .with_context("peer_id", peer_id);
        self.add_entry(entry);
    }

    fn log_session(&self, entity_id: &str, session_id: &str, action: &str, success: bool) {
        let entry = AuditLogEntry::new(
            AuditEventType::SessionManagement,
            entity_id,
            success,
            format!("Session {}: {}", action, session_id),
            self.next_sequence(),
        )
        .with_context("session_id", session_id)
        .with_context("action", action);
        self.add_entry(entry);
    }

    fn log_encryption(&self, entity_id: &str, operation: &str, target: &str, success: bool) {
        let entry = AuditLogEntry::new(
            AuditEventType::Encryption,
            entity_id,
            success,
            format!("Encryption {}: {}", operation, target),
            self.next_sequence(),
        )
        .with_context("operation", operation)
        .with_context("target", target);
        self.add_entry(entry);
    }

    fn entry_count(&self) -> u64 {
        self.entries.lock().unwrap().len() as u64
    }

    fn flush(&self) -> Result<(), SecurityError> {
        Ok(()) // In-memory, nothing to flush
    }
}

/// File-based audit logger (append-only for tamper resistance)
pub struct FileAuditLogger {
    writer: Arc<Mutex<BufWriter<File>>>,
    sequence: Arc<Mutex<u64>>,
    path: String,
}

impl FileAuditLogger {
    /// Create new file-based audit logger
    ///
    /// The file is opened in append mode for tamper resistance.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, SecurityError> {
        let path_str = path.as_ref().to_string_lossy().to_string();

        let file = OpenOptions::new().create(true).append(true).open(&path)?;

        Ok(Self {
            writer: Arc::new(Mutex::new(BufWriter::new(file))),
            sequence: Arc::new(Mutex::new(0)),
            path: path_str,
        })
    }

    /// Get the log file path
    pub fn path(&self) -> &str {
        &self.path
    }

    fn next_sequence(&self) -> u64 {
        let mut seq = self.sequence.lock().unwrap();
        *seq += 1;
        *seq
    }

    fn write_entry(&self, entry: &AuditLogEntry) {
        let json = entry.to_json();
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writeln!(writer, "{}", json);
            // Flush after each entry for durability
            let _ = writer.flush();
        }
    }
}

impl std::fmt::Debug for FileAuditLogger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileAuditLogger")
            .field("path", &self.path)
            .field("sequence", &self.sequence)
            .finish()
    }
}

impl AuditLogger for FileAuditLogger {
    fn log_authentication(&self, entity_id: &str, success: bool, reason: Option<&str>) {
        let description = match (success, reason) {
            (true, Some(r)) => format!("Authentication succeeded: {}", r),
            (true, None) => "Authentication succeeded".to_string(),
            (false, Some(r)) => format!("Authentication failed: {}", r),
            (false, None) => "Authentication failed".to_string(),
        };

        let entry = AuditLogEntry::new(
            AuditEventType::Authentication,
            entity_id,
            success,
            description,
            self.next_sequence(),
        );
        self.write_entry(&entry);
    }

    fn log_grant(&self, entity_id: &str, permission: Permission, target: &str) {
        let entry = AuditLogEntry::new(
            AuditEventType::Authorization,
            entity_id,
            true,
            format!("Permission granted: {:?}", permission),
            self.next_sequence(),
        )
        .with_context("permission", format!("{:?}", permission))
        .with_context("target", target);
        self.write_entry(&entry);
    }

    fn log_denial(&self, entity_id: &str, permission: &str, target: &str, reason: &str) {
        let entry = AuditLogEntry::new(
            AuditEventType::Authorization,
            entity_id,
            false,
            format!("Permission denied: {} - {}", permission, reason),
            self.next_sequence(),
        )
        .with_context("permission", permission)
        .with_context("target", target)
        .with_context("reason", reason);
        self.write_entry(&entry);
    }

    fn log_operation(&self, entity_id: &str, operation: &str, target: &str, success: bool) {
        let event_type = if operation.starts_with("read")
            || operation.starts_with("get")
            || operation.starts_with("query")
        {
            AuditEventType::DataAccess
        } else {
            AuditEventType::DataModification
        };

        let entry = AuditLogEntry::new(
            event_type,
            entity_id,
            success,
            format!("{} on {}", operation, target),
            self.next_sequence(),
        )
        .with_context("operation", operation)
        .with_context("target", target);
        self.write_entry(&entry);
    }

    fn log_violation(&self, entity_id: &str, violation: SecurityViolation, details: &str) {
        let entry = AuditLogEntry::new(
            AuditEventType::SecurityViolation,
            entity_id,
            false,
            format!("Security violation: {} - {}", violation, details),
            self.next_sequence(),
        )
        .with_context("violation_type", violation.to_string())
        .with_context("details", details);
        self.write_entry(&entry);
    }

    fn log_cell_event(&self, entity_id: &str, cell_id: &str, action: &str, success: bool) {
        let entry = AuditLogEntry::new(
            AuditEventType::CellFormation,
            entity_id,
            success,
            format!("Cell {}: {}", action, cell_id),
            self.next_sequence(),
        )
        .with_context("cell_id", cell_id)
        .with_context("action", action);
        self.write_entry(&entry);
    }

    fn log_key_exchange(&self, entity_id: &str, peer_id: &str, success: bool) {
        let entry = AuditLogEntry::new(
            AuditEventType::KeyExchange,
            entity_id,
            success,
            format!("Key exchange with peer: {}", peer_id),
            self.next_sequence(),
        )
        .with_context("peer_id", peer_id);
        self.write_entry(&entry);
    }

    fn log_session(&self, entity_id: &str, session_id: &str, action: &str, success: bool) {
        let entry = AuditLogEntry::new(
            AuditEventType::SessionManagement,
            entity_id,
            success,
            format!("Session {}: {}", action, session_id),
            self.next_sequence(),
        )
        .with_context("session_id", session_id)
        .with_context("action", action);
        self.write_entry(&entry);
    }

    fn log_encryption(&self, entity_id: &str, operation: &str, target: &str, success: bool) {
        let entry = AuditLogEntry::new(
            AuditEventType::Encryption,
            entity_id,
            success,
            format!("Encryption {}: {}", operation, target),
            self.next_sequence(),
        )
        .with_context("operation", operation)
        .with_context("target", target);
        self.write_entry(&entry);
    }

    fn entry_count(&self) -> u64 {
        *self.sequence.lock().unwrap()
    }

    fn flush(&self) -> Result<(), SecurityError> {
        self.writer
            .lock()
            .map_err(|e| SecurityError::Internal(e.to_string()))?
            .flush()?;
        Ok(())
    }
}

/// No-op audit logger (for testing/disabled logging)
#[derive(Debug, Default)]
pub struct NullAuditLogger;

impl NullAuditLogger {
    pub fn new() -> Self {
        Self
    }
}

impl AuditLogger for NullAuditLogger {
    fn log_authentication(&self, _entity_id: &str, _success: bool, _reason: Option<&str>) {}
    fn log_grant(&self, _entity_id: &str, _permission: Permission, _target: &str) {}
    fn log_denial(&self, _entity_id: &str, _permission: &str, _target: &str, _reason: &str) {}
    fn log_operation(&self, _entity_id: &str, _operation: &str, _target: &str, _success: bool) {}
    fn log_violation(&self, _entity_id: &str, _violation: SecurityViolation, _details: &str) {}
    fn log_cell_event(&self, _entity_id: &str, _cell_id: &str, _action: &str, _success: bool) {}
    fn log_key_exchange(&self, _entity_id: &str, _peer_id: &str, _success: bool) {}
    fn log_session(&self, _entity_id: &str, _session_id: &str, _action: &str, _success: bool) {}
    fn log_encryption(&self, _entity_id: &str, _operation: &str, _target: &str, _success: bool) {}
    fn entry_count(&self) -> u64 {
        0
    }
    fn flush(&self) -> Result<(), SecurityError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_entry_creation() {
        let entry = AuditLogEntry::new(
            AuditEventType::Authentication,
            "device:abc123",
            true,
            "Test entry",
            1,
        );

        assert_eq!(entry.entity_id, "device:abc123");
        assert!(entry.success);
        assert_eq!(entry.event_type, AuditEventType::Authentication);
        assert_eq!(entry.sequence, 1);
        assert!(entry.timestamp > 0);
    }

    #[test]
    fn test_audit_entry_with_context() {
        let entry = AuditLogEntry::new(
            AuditEventType::Authorization,
            "device:abc123",
            false,
            "Access denied",
            1,
        )
        .with_context("permission", "SetCellLeader")
        .with_context("cell_id", "cell-1");

        assert_eq!(entry.context.get("permission").unwrap(), "SetCellLeader");
        assert_eq!(entry.context.get("cell_id").unwrap(), "cell-1");
    }

    #[test]
    fn test_audit_entry_json_serialization() {
        let entry = AuditLogEntry::new(
            AuditEventType::Authentication,
            "device:abc123",
            true,
            "Login successful",
            42,
        );

        let json = entry.to_json();
        assert!(json.contains("\"entity_id\":\"device:abc123\""));
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"sequence\":42"));
    }

    #[test]
    fn test_memory_audit_logger() {
        let logger = MemoryAuditLogger::new();

        logger.log_authentication("device:abc", true, Some("verified"));
        logger.log_denial("device:abc", "SetLeader", "cell-1", "not authorized");
        logger.log_operation("device:abc", "store_cell", "cell-1", true);

        assert_eq!(logger.entry_count(), 3);

        let auth_entries = logger.entries_by_type(AuditEventType::Authentication);
        assert_eq!(auth_entries.len(), 1);
        assert!(auth_entries[0].success);

        let authz_entries = logger.entries_by_type(AuditEventType::Authorization);
        assert_eq!(authz_entries.len(), 1);
        assert!(!authz_entries[0].success);
    }

    #[test]
    fn test_memory_logger_all_event_types() {
        let logger = MemoryAuditLogger::new();

        logger.log_authentication("dev1", true, None);
        logger.log_grant("dev1", Permission::JoinCell, "cell-1");
        logger.log_denial("dev1", "SetLeader", "cell-1", "not leader");
        logger.log_operation("dev1", "read_cell", "cell-1", true);
        logger.log_operation("dev1", "store_cell", "cell-1", true);
        logger.log_violation("dev2", SecurityViolation::InvalidSignature, "bad sig");
        logger.log_cell_event("dev1", "cell-1", "join", true);
        logger.log_key_exchange("dev1", "dev2", true);
        logger.log_session("user1", "sess-1", "create", true);
        logger.log_encryption("dev1", "encrypt_for_cell", "cell-1", true);

        assert_eq!(logger.entry_count(), 10);

        // Verify each event type is logged
        assert_eq!(
            logger.entries_by_type(AuditEventType::Authentication).len(),
            1
        );
        assert_eq!(
            logger.entries_by_type(AuditEventType::Authorization).len(),
            2
        );
        assert_eq!(logger.entries_by_type(AuditEventType::DataAccess).len(), 1);
        assert_eq!(
            logger
                .entries_by_type(AuditEventType::DataModification)
                .len(),
            1
        );
        assert_eq!(
            logger
                .entries_by_type(AuditEventType::SecurityViolation)
                .len(),
            1
        );
        assert_eq!(
            logger.entries_by_type(AuditEventType::CellFormation).len(),
            1
        );
        assert_eq!(logger.entries_by_type(AuditEventType::KeyExchange).len(), 1);
        assert_eq!(
            logger
                .entries_by_type(AuditEventType::SessionManagement)
                .len(),
            1
        );
        assert_eq!(logger.entries_by_type(AuditEventType::Encryption).len(), 1);
    }

    #[test]
    fn test_file_audit_logger() {
        let temp_dir = tempfile::tempdir().unwrap();
        let log_path = temp_dir.path().join("audit.log");

        let logger = FileAuditLogger::new(&log_path).unwrap();
        logger.log_authentication("device:test", true, Some("test auth"));
        logger.log_denial("device:test", "TestPerm", "target", "testing");
        logger.flush().unwrap();

        // Read and verify log file
        let contents = std::fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);

        // Parse first line as JSON
        let entry: AuditLogEntry = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(entry.entity_id, "device:test");
        assert!(entry.success);
    }

    #[test]
    fn test_null_audit_logger() {
        let logger = NullAuditLogger::new();

        // All operations should be no-ops
        logger.log_authentication("test", true, None);
        logger.log_denial("test", "perm", "target", "reason");

        assert_eq!(logger.entry_count(), 0);
        assert!(logger.flush().is_ok());
    }

    #[test]
    fn test_security_violation_types() {
        let violations = vec![
            SecurityViolation::InvalidSignature,
            SecurityViolation::ReplayAttack,
            SecurityViolation::UnauthorizedAccess,
            SecurityViolation::CertificateError,
            SecurityViolation::TamperedMessage,
            SecurityViolation::RateLimitExceeded,
            SecurityViolation::UnknownDevice,
            SecurityViolation::ExpiredCredentials,
            SecurityViolation::ProtocolViolation,
        ];

        let logger = MemoryAuditLogger::new();
        for violation in violations {
            logger.log_violation("test", violation, "test details");
        }

        assert_eq!(logger.entry_count(), 9);
    }

    #[test]
    fn test_audit_entry_sequence_ordering() {
        let logger = MemoryAuditLogger::new();

        for i in 0..10 {
            logger.log_authentication(&format!("dev{}", i), true, None);
        }

        let entries = logger.entries();
        for (i, entry) in entries.iter().enumerate() {
            assert_eq!(entry.sequence, (i + 1) as u64);
        }
    }

    #[test]
    fn test_event_type_display() {
        assert_eq!(
            format!("{}", AuditEventType::Authentication),
            "AUTHENTICATION"
        );
        assert_eq!(
            format!("{}", AuditEventType::Authorization),
            "AUTHORIZATION"
        );
        assert_eq!(
            format!("{}", AuditEventType::SecurityViolation),
            "SECURITY_VIOLATION"
        );
    }
}
