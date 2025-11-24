//! Autonomous operation mode for partitioned nodes
//!
//! This module provides autonomous operation capabilities for nodes that are
//! isolated from all higher hierarchy levels (network partition). When a node
//! enters autonomous mode, it continues local operations while buffering
//! telemetry and data for eventual synchronization when connectivity is restored.
//!
//! ## Architecture
//!
//! ```text
//! AutonomousOperationHandler
//! ├── PartitionHandler trait implementation
//! ├── Autonomous state management
//! ├── Telemetry buffering
//! └── Recovery and synchronization
//! ```
//!
//! ## Usage
//!
//! ```rust
//! use hive_mesh::topology::{AutonomousOperationHandler, PartitionDetector};
//! use hive_mesh::beacon::HierarchyLevel;
//! use std::sync::Arc;
//!
//! // Create autonomous operation handler
//! let handler = Arc::new(AutonomousOperationHandler::new());
//!
//! // Create partition detector with handler
//! let detector = PartitionDetector::new(
//!     HierarchyLevel::Platform,
//!     Default::default()
//! ).with_handler(handler.clone());
//!
//! // Query autonomous state
//! assert!(!handler.is_autonomous());
//! ```

use super::partition::{PartitionEvent, PartitionHandler};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tracing::{info, warn};

/// Autonomous operation state
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AutonomousState {
    /// Normal operation with connectivity to higher levels
    #[default]
    Normal,

    /// Autonomous operation mode (partitioned from higher levels)
    Autonomous {
        /// When autonomous mode was entered
        entered_at: Instant,

        /// Number of detection attempts before partition was confirmed
        detection_attempts: u32,
    },
}

/// Handler for autonomous operation during network partitions
///
/// Implements the PartitionHandler trait to respond to partition detection
/// and healing events. When a partition is detected, the handler enters
/// autonomous mode, allowing the node to continue local operations while
/// buffering data for eventual synchronization.
#[derive(Debug)]
pub struct AutonomousOperationHandler {
    state: Arc<RwLock<AutonomousState>>,
}

impl AutonomousOperationHandler {
    /// Create a new autonomous operation handler
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(AutonomousState::default())),
        }
    }

    /// Check if currently in autonomous operation mode
    pub fn is_autonomous(&self) -> bool {
        matches!(
            *self.state.read().unwrap(),
            AutonomousState::Autonomous { .. }
        )
    }

    /// Get current autonomous state
    pub fn get_state(&self) -> AutonomousState {
        self.state.read().unwrap().clone()
    }

    /// Enter autonomous operation mode
    fn enter_autonomous_mode(&self, detection_attempts: u32) {
        let mut state = self.state.write().unwrap();

        if matches!(*state, AutonomousState::Autonomous { .. }) {
            warn!("Already in autonomous mode, ignoring duplicate partition detection");
            return;
        }

        *state = AutonomousState::Autonomous {
            entered_at: Instant::now(),
            detection_attempts,
        };

        info!(
            "Entered autonomous operation mode after {} detection attempts",
            detection_attempts
        );
    }

    /// Exit autonomous operation mode (return to normal operation)
    fn exit_autonomous_mode(&self, autonomous_duration: std::time::Duration) {
        let mut state = self.state.write().unwrap();

        if matches!(*state, AutonomousState::Normal) {
            warn!("Already in normal mode, ignoring duplicate partition healing");
            return;
        }

        *state = AutonomousState::Normal;

        info!(
            "Exited autonomous operation mode after {:?} duration",
            autonomous_duration
        );
    }
}

impl Default for AutonomousOperationHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl PartitionHandler for AutonomousOperationHandler {
    fn on_partition_detected(&self, event: &PartitionEvent) {
        if let PartitionEvent::PartitionDetected {
            attempts,
            detection_duration,
        } = event
        {
            info!(
                "Network partition detected after {} attempts over {:?}",
                attempts, detection_duration
            );

            self.enter_autonomous_mode(*attempts);
        }
    }

    fn on_partition_healed(&self, event: &PartitionEvent) {
        if let PartitionEvent::PartitionHealed {
            visible_beacons,
            partition_duration,
        } = event
        {
            info!(
                "Network partition healed, {} higher-level beacons visible, partition lasted {:?}",
                visible_beacons, partition_duration
            );

            self.exit_autonomous_mode(*partition_duration);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_autonomous_handler_creation() {
        let handler = AutonomousOperationHandler::new();
        assert!(!handler.is_autonomous());
        assert_eq!(handler.get_state(), AutonomousState::Normal);
    }

    #[test]
    fn test_enter_autonomous_mode() {
        let handler = AutonomousOperationHandler::new();

        let event = PartitionEvent::PartitionDetected {
            attempts: 3,
            detection_duration: Duration::from_secs(6),
        };

        handler.on_partition_detected(&event);

        assert!(handler.is_autonomous());

        match handler.get_state() {
            AutonomousState::Autonomous {
                detection_attempts, ..
            } => {
                assert_eq!(detection_attempts, 3);
            }
            _ => panic!("Expected Autonomous state"),
        }
    }

    #[test]
    fn test_exit_autonomous_mode() {
        let handler = AutonomousOperationHandler::new();

        // Enter autonomous mode
        let partition_event = PartitionEvent::PartitionDetected {
            attempts: 3,
            detection_duration: Duration::from_secs(6),
        };
        handler.on_partition_detected(&partition_event);
        assert!(handler.is_autonomous());

        // Exit autonomous mode
        let healed_event = PartitionEvent::PartitionHealed {
            visible_beacons: 2,
            partition_duration: Duration::from_secs(120),
        };
        handler.on_partition_healed(&healed_event);

        assert!(!handler.is_autonomous());
        assert_eq!(handler.get_state(), AutonomousState::Normal);
    }

    #[test]
    fn test_duplicate_partition_detection() {
        let handler = AutonomousOperationHandler::new();

        let event = PartitionEvent::PartitionDetected {
            attempts: 3,
            detection_duration: Duration::from_secs(6),
        };

        // First partition detection
        handler.on_partition_detected(&event);
        assert!(handler.is_autonomous());

        let state1 = handler.get_state();

        // Duplicate partition detection should be ignored
        handler.on_partition_detected(&event);

        let state2 = handler.get_state();
        assert_eq!(state1, state2);
    }

    #[test]
    fn test_duplicate_partition_healing() {
        let handler = AutonomousOperationHandler::new();

        let healed_event = PartitionEvent::PartitionHealed {
            visible_beacons: 2,
            partition_duration: Duration::from_secs(120),
        };

        // Healing when already in normal mode should be ignored
        handler.on_partition_healed(&healed_event);
        assert!(!handler.is_autonomous());
        assert_eq!(handler.get_state(), AutonomousState::Normal);
    }

    #[test]
    fn test_autonomous_state_transition() {
        let handler = AutonomousOperationHandler::new();

        // Normal → Autonomous
        let partition_event = PartitionEvent::PartitionDetected {
            attempts: 5,
            detection_duration: Duration::from_secs(10),
        };
        handler.on_partition_detected(&partition_event);

        assert!(handler.is_autonomous());

        // Autonomous → Normal
        let healed_event = PartitionEvent::PartitionHealed {
            visible_beacons: 1,
            partition_duration: Duration::from_secs(300),
        };
        handler.on_partition_healed(&healed_event);

        assert!(!handler.is_autonomous());
    }
}
