//! Data type classification for QoS (ADR-019)
//!
//! Maps Peat data types to their default QoS classes, enabling
//! automatic priority assignment based on data semantics.

use super::{QoSClass, QoSPolicy};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Peat Protocol data types for QoS classification
///
/// Represents the semantic categories of data flowing through the mesh.
/// Each type has a default QoS class and policy.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataType {
    // ========================================================================
    // P1: Critical - Mission-critical, immediate sync
    // ========================================================================
    /// Enemy/target contact report requiring immediate commander awareness
    ContactReport,
    /// Emergency alert (medical, safety, system failure)
    EmergencyAlert,
    /// Abort/halt command requiring immediate execution
    AbortCommand,
    /// Rules of engagement update (safety-critical)
    RoeUpdate,

    // ========================================================================
    // P2: High - Important, sync within seconds
    // ========================================================================
    /// Target imagery for analysis
    TargetImage,
    /// Audio intercept for intelligence
    AudioIntercept,
    /// Mission retasking directive
    MissionRetasking,
    /// Formation change command
    FormationChange,

    // ========================================================================
    // P3: Normal - Standard operational data
    // ========================================================================
    /// Node health status (battery, sensors, comms)
    HealthStatus,
    /// Capability advertisement change
    CapabilityChange,
    /// Cell formation update
    FormationUpdate,
    /// Task assignment
    TaskAssignment,

    // ========================================================================
    // P4: Low - Routine telemetry
    // ========================================================================
    /// Periodic position update
    PositionUpdate,
    /// Heartbeat/keepalive
    Heartbeat,
    /// Sensor telemetry (non-critical)
    SensorTelemetry,
    /// Environment data (weather, terrain)
    EnvironmentData,

    // ========================================================================
    // P5: Bulk - Archival/historical data
    // ========================================================================
    /// AI model update distribution
    ModelUpdate,
    /// Debug/diagnostic logs
    DebugLog,
    /// Historical track data
    HistoricalTrack,
    /// Training data for on-device ML
    TrainingData,

    // ========================================================================
    // Dynamic/Custom
    // ========================================================================
    /// Custom data type with explicit QoS class
    Custom {
        /// User-defined type name
        name: String,
        /// Explicitly assigned QoS class
        qos_class: QoSClass,
    },
}

impl DataType {
    /// Get the default QoS class for this data type
    pub fn default_class(&self) -> QoSClass {
        match self {
            // P1: Critical
            Self::ContactReport | Self::EmergencyAlert | Self::AbortCommand | Self::RoeUpdate => {
                QoSClass::Critical
            }

            // P2: High
            Self::TargetImage
            | Self::AudioIntercept
            | Self::MissionRetasking
            | Self::FormationChange => QoSClass::High,

            // P3: Normal
            Self::HealthStatus
            | Self::CapabilityChange
            | Self::FormationUpdate
            | Self::TaskAssignment => QoSClass::Normal,

            // P4: Low
            Self::PositionUpdate
            | Self::Heartbeat
            | Self::SensorTelemetry
            | Self::EnvironmentData => QoSClass::Low,

            // P5: Bulk
            Self::ModelUpdate | Self::DebugLog | Self::HistoricalTrack | Self::TrainingData => {
                QoSClass::Bulk
            }

            // Custom uses its explicit class
            Self::Custom { qos_class, .. } => *qos_class,
        }
    }

    /// Get the default QoS policy for this data type
    pub fn default_policy(&self) -> QoSPolicy {
        match self {
            // P1: Critical - strict latency, no TTL, non-preemptable
            Self::ContactReport => QoSPolicy {
                base_class: QoSClass::Critical,
                max_latency_ms: Some(500),
                max_size_bytes: Some(32 * 1024), // 32KB
                ttl_seconds: None,               // Never expire
                retention_priority: 5,
                preemptable: false,
            },
            Self::EmergencyAlert => QoSPolicy {
                base_class: QoSClass::Critical,
                max_latency_ms: Some(500),
                max_size_bytes: Some(8 * 1024), // 8KB
                ttl_seconds: None,
                retention_priority: 5,
                preemptable: false,
            },
            Self::AbortCommand => QoSPolicy {
                base_class: QoSClass::Critical,
                max_latency_ms: Some(500),
                max_size_bytes: Some(1024), // 1KB - commands are small
                ttl_seconds: None,
                retention_priority: 5,
                preemptable: false,
            },
            Self::RoeUpdate => QoSPolicy {
                base_class: QoSClass::Critical,
                max_latency_ms: Some(500),
                max_size_bytes: Some(64 * 1024), // 64KB for ROE docs
                ttl_seconds: None,
                retention_priority: 5,
                preemptable: false,
            },

            // P2: High - moderate latency, longer TTL
            Self::TargetImage => QoSPolicy {
                base_class: QoSClass::High,
                max_latency_ms: Some(5_000),
                max_size_bytes: Some(10 * 1024 * 1024), // 10MB
                ttl_seconds: Some(3600),                // 1 hour
                retention_priority: 4,
                preemptable: true,
            },
            Self::AudioIntercept => QoSPolicy {
                base_class: QoSClass::High,
                max_latency_ms: Some(5_000),
                max_size_bytes: Some(5 * 1024 * 1024), // 5MB
                ttl_seconds: Some(3600),
                retention_priority: 4,
                preemptable: true,
            },
            Self::MissionRetasking => QoSPolicy {
                base_class: QoSClass::High,
                max_latency_ms: Some(2_000), // 2s - commands need faster delivery
                max_size_bytes: Some(64 * 1024),
                ttl_seconds: Some(7200), // 2 hours
                retention_priority: 4,
                preemptable: false, // Commands should complete
            },
            Self::FormationChange => QoSPolicy {
                base_class: QoSClass::High,
                max_latency_ms: Some(5_000),
                max_size_bytes: Some(16 * 1024),
                ttl_seconds: Some(3600),
                retention_priority: 4,
                preemptable: true,
            },

            // P3: Normal - relaxed latency
            Self::HealthStatus => QoSPolicy {
                base_class: QoSClass::Normal,
                max_latency_ms: Some(60_000), // 1 minute
                max_size_bytes: Some(8 * 1024),
                ttl_seconds: Some(86400), // 24 hours
                retention_priority: 3,
                preemptable: true,
            },
            Self::CapabilityChange => QoSPolicy {
                base_class: QoSClass::Normal,
                max_latency_ms: Some(60_000),
                max_size_bytes: Some(16 * 1024),
                ttl_seconds: Some(86400),
                retention_priority: 3,
                preemptable: true,
            },
            Self::FormationUpdate => QoSPolicy {
                base_class: QoSClass::Normal,
                max_latency_ms: Some(60_000),
                max_size_bytes: Some(32 * 1024),
                ttl_seconds: Some(43200), // 12 hours
                retention_priority: 3,
                preemptable: true,
            },
            Self::TaskAssignment => QoSPolicy {
                base_class: QoSClass::Normal,
                max_latency_ms: Some(30_000), // 30s - tasks need moderate priority
                max_size_bytes: Some(16 * 1024),
                ttl_seconds: Some(86400),
                retention_priority: 3,
                preemptable: true,
            },

            // P4: Low - can be delayed
            Self::PositionUpdate => QoSPolicy {
                base_class: QoSClass::Low,
                max_latency_ms: Some(300_000), // 5 minutes
                max_size_bytes: Some(1024),    // Position data is small
                ttl_seconds: Some(86400),      // 24 hours
                retention_priority: 2,
                preemptable: true,
            },
            Self::Heartbeat => QoSPolicy {
                base_class: QoSClass::Low,
                max_latency_ms: Some(300_000),
                max_size_bytes: Some(256), // Heartbeats are tiny
                ttl_seconds: Some(3600),   // 1 hour
                retention_priority: 1,     // Evict first
                preemptable: true,
            },
            Self::SensorTelemetry => QoSPolicy {
                base_class: QoSClass::Low,
                max_latency_ms: Some(300_000),
                max_size_bytes: Some(64 * 1024),
                ttl_seconds: Some(43200), // 12 hours
                retention_priority: 2,
                preemptable: true,
            },
            Self::EnvironmentData => QoSPolicy {
                base_class: QoSClass::Low,
                max_latency_ms: Some(600_000), // 10 minutes
                max_size_bytes: Some(128 * 1024),
                ttl_seconds: Some(86400),
                retention_priority: 2,
                preemptable: true,
            },

            // P5: Bulk - background transfer
            Self::ModelUpdate => QoSPolicy {
                base_class: QoSClass::Bulk,
                max_latency_ms: None, // No latency requirement
                max_size_bytes: Some(500 * 1024 * 1024), // 500MB for ML models
                ttl_seconds: Some(604800), // 1 week
                retention_priority: 2, // Keep models longer
                preemptable: true,
            },
            Self::DebugLog => QoSPolicy {
                base_class: QoSClass::Bulk,
                max_latency_ms: None,
                max_size_bytes: Some(10 * 1024 * 1024), // 10MB
                ttl_seconds: Some(259200),              // 3 days
                retention_priority: 1,                  // Evict first
                preemptable: true,
            },
            Self::HistoricalTrack => QoSPolicy {
                base_class: QoSClass::Bulk,
                max_latency_ms: None,
                max_size_bytes: Some(100 * 1024 * 1024), // 100MB
                ttl_seconds: Some(604800),               // 1 week
                retention_priority: 2,
                preemptable: true,
            },
            Self::TrainingData => QoSPolicy {
                base_class: QoSClass::Bulk,
                max_latency_ms: None,
                max_size_bytes: Some(1024 * 1024 * 1024), // 1GB
                ttl_seconds: Some(2592000),               // 30 days
                retention_priority: 2,
                preemptable: true,
            },

            // Custom types get default policy for their class
            Self::Custom { qos_class, .. } => match qos_class {
                QoSClass::Critical => QoSPolicy::critical(),
                QoSClass::High => QoSPolicy::high(),
                QoSClass::Normal => QoSPolicy::default(),
                QoSClass::Low => QoSPolicy::low(),
                QoSClass::Bulk => QoSPolicy::bulk(),
            },
        }
    }

    /// Check if this data type is mission-critical
    pub fn is_critical(&self) -> bool {
        self.default_class() == QoSClass::Critical
    }

    /// Check if this data type can be preempted
    pub fn is_preemptable(&self) -> bool {
        self.default_policy().preemptable
    }

    /// Get all predefined data types (excluding Custom)
    pub fn all_predefined() -> &'static [DataType] {
        &[
            // P1
            DataType::ContactReport,
            DataType::EmergencyAlert,
            DataType::AbortCommand,
            DataType::RoeUpdate,
            // P2
            DataType::TargetImage,
            DataType::AudioIntercept,
            DataType::MissionRetasking,
            DataType::FormationChange,
            // P3
            DataType::HealthStatus,
            DataType::CapabilityChange,
            DataType::FormationUpdate,
            DataType::TaskAssignment,
            // P4
            DataType::PositionUpdate,
            DataType::Heartbeat,
            DataType::SensorTelemetry,
            DataType::EnvironmentData,
            // P5
            DataType::ModelUpdate,
            DataType::DebugLog,
            DataType::HistoricalTrack,
            DataType::TrainingData,
        ]
    }
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContactReport => write!(f, "ContactReport"),
            Self::EmergencyAlert => write!(f, "EmergencyAlert"),
            Self::AbortCommand => write!(f, "AbortCommand"),
            Self::RoeUpdate => write!(f, "RoeUpdate"),
            Self::TargetImage => write!(f, "TargetImage"),
            Self::AudioIntercept => write!(f, "AudioIntercept"),
            Self::MissionRetasking => write!(f, "MissionRetasking"),
            Self::FormationChange => write!(f, "FormationChange"),
            Self::HealthStatus => write!(f, "HealthStatus"),
            Self::CapabilityChange => write!(f, "CapabilityChange"),
            Self::FormationUpdate => write!(f, "FormationUpdate"),
            Self::TaskAssignment => write!(f, "TaskAssignment"),
            Self::PositionUpdate => write!(f, "PositionUpdate"),
            Self::Heartbeat => write!(f, "Heartbeat"),
            Self::SensorTelemetry => write!(f, "SensorTelemetry"),
            Self::EnvironmentData => write!(f, "EnvironmentData"),
            Self::ModelUpdate => write!(f, "ModelUpdate"),
            Self::DebugLog => write!(f, "DebugLog"),
            Self::HistoricalTrack => write!(f, "HistoricalTrack"),
            Self::TrainingData => write!(f, "TrainingData"),
            Self::Custom { name, .. } => write!(f, "Custom({})", name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_type_default_class() {
        // P1 Critical
        assert_eq!(DataType::ContactReport.default_class(), QoSClass::Critical);
        assert_eq!(DataType::EmergencyAlert.default_class(), QoSClass::Critical);
        assert_eq!(DataType::AbortCommand.default_class(), QoSClass::Critical);

        // P2 High
        assert_eq!(DataType::TargetImage.default_class(), QoSClass::High);
        assert_eq!(DataType::MissionRetasking.default_class(), QoSClass::High);

        // P3 Normal
        assert_eq!(DataType::HealthStatus.default_class(), QoSClass::Normal);
        assert_eq!(DataType::CapabilityChange.default_class(), QoSClass::Normal);

        // P4 Low
        assert_eq!(DataType::PositionUpdate.default_class(), QoSClass::Low);
        assert_eq!(DataType::Heartbeat.default_class(), QoSClass::Low);

        // P5 Bulk
        assert_eq!(DataType::ModelUpdate.default_class(), QoSClass::Bulk);
        assert_eq!(DataType::DebugLog.default_class(), QoSClass::Bulk);
    }

    #[test]
    fn test_data_type_default_policy() {
        let policy = DataType::ContactReport.default_policy();
        assert_eq!(policy.base_class, QoSClass::Critical);
        assert_eq!(policy.max_latency_ms, Some(500));
        assert!(!policy.preemptable);

        let policy = DataType::PositionUpdate.default_policy();
        assert_eq!(policy.base_class, QoSClass::Low);
        assert_eq!(policy.max_latency_ms, Some(300_000));
        assert!(policy.preemptable);
    }

    #[test]
    fn test_data_type_is_critical() {
        assert!(DataType::ContactReport.is_critical());
        assert!(DataType::EmergencyAlert.is_critical());
        assert!(!DataType::HealthStatus.is_critical());
        assert!(!DataType::DebugLog.is_critical());
    }

    #[test]
    fn test_data_type_is_preemptable() {
        assert!(!DataType::ContactReport.is_preemptable());
        assert!(!DataType::AbortCommand.is_preemptable());
        assert!(!DataType::MissionRetasking.is_preemptable()); // Commands complete
        assert!(DataType::TargetImage.is_preemptable());
        assert!(DataType::HealthStatus.is_preemptable());
    }

    #[test]
    fn test_custom_data_type() {
        let custom = DataType::Custom {
            name: "MyType".to_string(),
            qos_class: QoSClass::High,
        };
        assert_eq!(custom.default_class(), QoSClass::High);
        assert_eq!(custom.to_string(), "Custom(MyType)");
    }

    #[test]
    fn test_all_predefined_data_types() {
        let all = DataType::all_predefined();
        assert_eq!(all.len(), 20);

        // Verify all types have valid policies
        for dt in all {
            let policy = dt.default_policy();
            assert!(policy.retention_priority >= 1 && policy.retention_priority <= 5);
        }
    }

    #[test]
    fn test_data_type_serialization() {
        let dt = DataType::ContactReport;
        let json = serde_json::to_string(&dt).unwrap();
        assert_eq!(json, "\"ContactReport\"");

        let deserialized: DataType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, DataType::ContactReport);
    }

    #[test]
    fn test_model_update_large_size() {
        // ML models can be large
        let policy = DataType::ModelUpdate.default_policy();
        assert_eq!(policy.max_size_bytes, Some(500 * 1024 * 1024)); // 500MB
    }
}
