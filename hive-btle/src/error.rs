//! Error types for HIVE-BTLE
//!
//! Provides a comprehensive error enum for all BLE operations including
//! adapter initialization, discovery, GATT operations, and connectivity.

#[cfg(not(feature = "std"))]
use core::fmt;

/// Result type alias for BLE operations
pub type Result<T> = core::result::Result<T, BleError>;

/// Errors that can occur during BLE operations
#[derive(Debug)]
#[cfg_attr(feature = "std", derive(thiserror::Error))]
pub enum BleError {
    /// Bluetooth adapter not available on this device
    #[cfg_attr(feature = "std", error("Bluetooth adapter not available"))]
    AdapterNotAvailable,

    /// Bluetooth is powered off
    #[cfg_attr(feature = "std", error("Bluetooth is powered off"))]
    NotPowered,

    /// Bluetooth permissions not granted
    #[cfg_attr(feature = "std", error("Bluetooth permissions not granted: {0}"))]
    PermissionDenied(String),

    /// Feature not supported by this adapter or platform
    #[cfg_attr(feature = "std", error("Feature not supported: {0}"))]
    NotSupported(String),

    /// Connection to peer failed
    #[cfg_attr(feature = "std", error("Connection failed: {0}"))]
    ConnectionFailed(String),

    /// Connection was lost
    #[cfg_attr(feature = "std", error("Connection lost: {0}"))]
    ConnectionLost(String),

    /// Discovery operation failed
    #[cfg_attr(feature = "std", error("Discovery failed: {0}"))]
    DiscoveryFailed(String),

    /// GATT operation failed
    #[cfg_attr(feature = "std", error("GATT error: {0}"))]
    GattError(String),

    /// Characteristic not found
    #[cfg_attr(feature = "std", error("Characteristic not found: {0}"))]
    CharacteristicNotFound(String),

    /// Service not found
    #[cfg_attr(feature = "std", error("Service not found: {0}"))]
    ServiceNotFound(String),

    /// MTU negotiation failed
    #[cfg_attr(
        feature = "std",
        error("MTU negotiation failed: requested {requested}, got {actual}")
    )]
    MtuNegotiationFailed {
        /// Requested MTU size
        requested: u16,
        /// Actual negotiated MTU
        actual: u16,
    },

    /// Security/pairing error
    #[cfg_attr(feature = "std", error("Security error: {0}"))]
    SecurityError(String),

    /// Pairing failed
    #[cfg_attr(feature = "std", error("Pairing failed: {0}"))]
    PairingFailed(String),

    /// Sync operation failed
    #[cfg_attr(feature = "std", error("Sync error: {0}"))]
    SyncError(String),

    /// Platform-specific error
    #[cfg_attr(feature = "std", error("Platform error: {0}"))]
    PlatformError(String),

    /// Operation timed out
    #[cfg_attr(feature = "std", error("Operation timed out"))]
    Timeout,

    /// Invalid configuration
    #[cfg_attr(feature = "std", error("Invalid configuration: {0}"))]
    InvalidConfig(String),

    /// Invalid state for this operation
    #[cfg_attr(feature = "std", error("Invalid state: {0}"))]
    InvalidState(String),

    /// Resource exhausted (e.g., max connections reached)
    #[cfg_attr(feature = "std", error("Resource exhausted: {0}"))]
    ResourceExhausted(String),

    /// I/O error
    #[cfg_attr(feature = "std", error("I/O error: {0}"))]
    Io(String),
}

// Manual Display implementation for no_std
#[cfg(not(feature = "std"))]
impl fmt::Display for BleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BleError::AdapterNotAvailable => write!(f, "Bluetooth adapter not available"),
            BleError::NotPowered => write!(f, "Bluetooth is powered off"),
            BleError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            BleError::NotSupported(msg) => write!(f, "Not supported: {}", msg),
            BleError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            BleError::ConnectionLost(msg) => write!(f, "Connection lost: {}", msg),
            BleError::DiscoveryFailed(msg) => write!(f, "Discovery failed: {}", msg),
            BleError::GattError(msg) => write!(f, "GATT error: {}", msg),
            BleError::CharacteristicNotFound(uuid) => {
                write!(f, "Characteristic not found: {}", uuid)
            }
            BleError::ServiceNotFound(uuid) => write!(f, "Service not found: {}", uuid),
            BleError::MtuNegotiationFailed { requested, actual } => {
                write!(
                    f,
                    "MTU negotiation failed: requested {}, got {}",
                    requested, actual
                )
            }
            BleError::SecurityError(msg) => write!(f, "Security error: {}", msg),
            BleError::PairingFailed(msg) => write!(f, "Pairing failed: {}", msg),
            BleError::SyncError(msg) => write!(f, "Sync error: {}", msg),
            BleError::PlatformError(msg) => write!(f, "Platform error: {}", msg),
            BleError::Timeout => write!(f, "Operation timed out"),
            BleError::InvalidConfig(msg) => write!(f, "Invalid config: {}", msg),
            BleError::InvalidState(msg) => write!(f, "Invalid state: {}", msg),
            BleError::ResourceExhausted(msg) => write!(f, "Resource exhausted: {}", msg),
            BleError::Io(msg) => write!(f, "I/O error: {}", msg),
        }
    }
}

#[cfg(feature = "std")]
impl From<std::io::Error> for BleError {
    fn from(err: std::io::Error) -> Self {
        BleError::Io(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = BleError::ConnectionFailed("peer unreachable".to_string());
        assert!(err.to_string().contains("Connection failed"));
        assert!(err.to_string().contains("peer unreachable"));
    }

    #[test]
    fn test_mtu_error() {
        let err = BleError::MtuNegotiationFailed {
            requested: 512,
            actual: 23,
        };
        let msg = err.to_string();
        assert!(msg.contains("512"));
        assert!(msg.contains("23"));
    }
}
