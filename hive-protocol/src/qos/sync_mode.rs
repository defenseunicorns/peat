//! Sync Mode configuration for delta vs state-based synchronization (ADR-019 Amendment)
//!
//! This module addresses Issue #346: delta-based sync doesn't scale when many documents
//! change frequently. `SyncMode` allows per-collection configuration of how documents
//! sync between peers.
//!
//! # Key Insight
//!
//! ```text
//! Current Behavior (Issue #346):
//! ├─ Squad Leader offline for 5 minutes
//! ├─ 7 soldiers sending beacons every second = 2,100 beacon updates
//! ├─ On reconnection: ALL 2,100 deltas must sync
//! └─ Documents never reach Platoon Leader (broadcast channel lags)
//!
//! With LatestOnly Mode:
//! ├─ Squad Leader offline for 5 minutes
//! ├─ Beacons configured as "LatestOnly" sync mode
//! ├─ On reconnection: Only 7 current positions sync (one per soldier)
//! └─ Sync completes in milliseconds
//! ```
//!
//! # Example
//!
//! ```
//! use hive_protocol::qos::SyncMode;
//!
//! // Position data - only current state matters
//! assert_eq!(SyncMode::default_for_collection("beacons"), SyncMode::LatestOnly);
//!
//! // Audit data - full history required
//! assert_eq!(SyncMode::default_for_collection("commands"), SyncMode::FullHistory);
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::RwLock;

/// Determines how much document history syncs between peers (ADR-019 Amendment)
///
/// This is critical for Issue #346: delta-based sync doesn't scale when many documents
/// change frequently. `LatestOnly` mode syncs only current state, reducing reconnection
/// traffic by up to 300× for high-frequency data like beacons.
///
/// # Implementation
///
/// - **FullHistory**: Uses `generate_sync_message()` - delta-based sync protocol
/// - **LatestOnly**: Uses `doc.save()` - sends full document state, no history
/// - **WindowedHistory**: Uses time-filtered `generate_sync_message()` (Phase 2)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SyncMode {
    /// Sync all deltas - observers see every historical change
    ///
    /// Uses Automerge's `generate_sync_message()` protocol.
    /// Best for: Audit logs, commands, contact reports.
    #[default]
    FullHistory,

    /// Sync only current document state, discard intermediate deltas
    ///
    /// Uses `doc.save()` to send full document state.
    /// Best for: Positions, status updates, health telemetry.
    /// **Reduces reconnection sync by ~300× for high-frequency data.**
    LatestOnly,

    /// Sync deltas within a time window, discard older (Phase 2)
    ///
    /// Uses filtered `generate_sync_message()`.
    /// Best for: Recent track history, last N minutes of updates.
    WindowedHistory {
        /// Time window in seconds - only sync changes within this window
        window_seconds: u64,
    },
}

impl SyncMode {
    /// Returns true if this mode syncs full delta history
    #[inline]
    pub fn is_full_history(&self) -> bool {
        matches!(self, Self::FullHistory)
    }

    /// Returns true if this mode syncs only current state
    #[inline]
    pub fn is_latest_only(&self) -> bool {
        matches!(self, Self::LatestOnly)
    }

    /// Returns true if this mode uses windowed history
    #[inline]
    pub fn is_windowed(&self) -> bool {
        matches!(self, Self::WindowedHistory { .. })
    }

    /// Get the window size in seconds, if applicable
    pub fn window_seconds(&self) -> Option<u64> {
        match self {
            Self::WindowedHistory { window_seconds } => Some(*window_seconds),
            _ => None,
        }
    }

    /// Default sync mode for a collection name
    ///
    /// Returns appropriate defaults based on collection semantics:
    /// - beacons, platforms, tracks → LatestOnly (position data)
    /// - node_states, squad_summaries, platoon_summaries → LatestOnly (hierarchical aggregation)
    /// - contact_reports, commands, audit_logs → FullHistory (critical data)
    /// - track_history → WindowedHistory(300) (5 minutes)
    pub fn default_for_collection(collection: &str) -> Self {
        match collection {
            // Position/state data - only current matters
            "beacons" | "platforms" | "tracks" | "nodes" | "cells" => Self::LatestOnly,

            // Hierarchical aggregation data - only current summaries matter
            // These collections update frequently and history accumulates unbounded without this
            "node_states" | "squad_summaries" | "platoon_summaries" | "company_summaries" => {
                Self::LatestOnly
            }

            // Critical data - full history required
            "contact_reports" | "commands" | "audit_logs" | "alerts" => Self::FullHistory,

            // Track history - recent changes useful
            "track_history" | "capability_history" => Self::WindowedHistory {
                window_seconds: 300,
            },

            // Default to FullHistory for unknown collections
            _ => Self::FullHistory,
        }
    }
}

impl fmt::Display for SyncMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FullHistory => write!(f, "FullHistory"),
            Self::LatestOnly => write!(f, "LatestOnly"),
            Self::WindowedHistory { window_seconds } => {
                write!(f, "WindowedHistory({}s)", window_seconds)
            }
        }
    }
}

/// Registry for per-collection sync mode configuration
///
/// Allows runtime configuration of sync modes per collection,
/// with sensible defaults based on collection names.
///
/// # Example
///
/// ```
/// use hive_protocol::qos::{SyncMode, SyncModeRegistry};
///
/// let registry = SyncModeRegistry::with_defaults();
///
/// // Beacons default to LatestOnly
/// assert_eq!(registry.get("beacons"), SyncMode::LatestOnly);
///
/// // Override for a specific collection
/// registry.set("custom_collection", SyncMode::LatestOnly);
/// ```
#[derive(Debug, Default)]
pub struct SyncModeRegistry {
    /// Per-collection sync mode overrides
    overrides: RwLock<HashMap<String, SyncMode>>,
}

impl SyncModeRegistry {
    /// Create a new empty registry (uses defaults for all collections)
    pub fn new() -> Self {
        Self {
            overrides: RwLock::new(HashMap::new()),
        }
    }

    /// Create a registry with standard defaults pre-configured
    pub fn with_defaults() -> Self {
        let registry = Self::new();
        // Pre-populate with known collections for better performance
        let defaults = [
            ("beacons", SyncMode::LatestOnly),
            ("platforms", SyncMode::LatestOnly),
            ("tracks", SyncMode::LatestOnly),
            ("nodes", SyncMode::LatestOnly),
            ("cells", SyncMode::LatestOnly),
            // Hierarchical aggregation collections - only current summaries matter
            ("node_states", SyncMode::LatestOnly),
            ("squad_summaries", SyncMode::LatestOnly),
            ("platoon_summaries", SyncMode::LatestOnly),
            ("company_summaries", SyncMode::LatestOnly),
            // Critical data - full history required
            ("contact_reports", SyncMode::FullHistory),
            ("commands", SyncMode::FullHistory),
            ("audit_logs", SyncMode::FullHistory),
            ("alerts", SyncMode::FullHistory),
            (
                "track_history",
                SyncMode::WindowedHistory {
                    window_seconds: 300,
                },
            ),
            (
                "capability_history",
                SyncMode::WindowedHistory {
                    window_seconds: 300,
                },
            ),
        ];

        {
            let mut overrides = registry.overrides.write().unwrap();
            for (collection, mode) in defaults {
                overrides.insert(collection.to_string(), mode);
            }
        }

        registry
    }

    /// Get the sync mode for a collection
    ///
    /// Returns the configured override if set, otherwise the default for that collection.
    pub fn get(&self, collection: &str) -> SyncMode {
        self.overrides
            .read()
            .unwrap()
            .get(collection)
            .copied()
            .unwrap_or_else(|| SyncMode::default_for_collection(collection))
    }

    /// Set the sync mode for a collection
    pub fn set(&self, collection: &str, mode: SyncMode) {
        self.overrides
            .write()
            .unwrap()
            .insert(collection.to_string(), mode);
    }

    /// Remove a collection override (will use default)
    pub fn remove(&self, collection: &str) -> Option<SyncMode> {
        self.overrides.write().unwrap().remove(collection)
    }

    /// Get all configured overrides
    pub fn all_overrides(&self) -> HashMap<String, SyncMode> {
        self.overrides.read().unwrap().clone()
    }

    /// Check if a collection is configured for LatestOnly mode
    ///
    /// Convenience method for the sync coordinator.
    #[inline]
    pub fn is_latest_only(&self, collection: &str) -> bool {
        self.get(collection).is_latest_only()
    }

    /// Check if a collection is configured for FullHistory mode
    #[inline]
    pub fn is_full_history(&self, collection: &str) -> bool {
        self.get(collection).is_full_history()
    }
}

impl Clone for SyncModeRegistry {
    fn clone(&self) -> Self {
        Self {
            overrides: RwLock::new(self.overrides.read().unwrap().clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_mode_defaults() {
        // Position data should be LatestOnly
        assert_eq!(
            SyncMode::default_for_collection("beacons"),
            SyncMode::LatestOnly
        );
        assert_eq!(
            SyncMode::default_for_collection("platforms"),
            SyncMode::LatestOnly
        );
        assert_eq!(
            SyncMode::default_for_collection("tracks"),
            SyncMode::LatestOnly
        );

        // Hierarchical aggregation should be LatestOnly (Issue #401 - memory blowout fix)
        assert_eq!(
            SyncMode::default_for_collection("node_states"),
            SyncMode::LatestOnly
        );
        assert_eq!(
            SyncMode::default_for_collection("squad_summaries"),
            SyncMode::LatestOnly
        );
        assert_eq!(
            SyncMode::default_for_collection("platoon_summaries"),
            SyncMode::LatestOnly
        );
        assert_eq!(
            SyncMode::default_for_collection("company_summaries"),
            SyncMode::LatestOnly
        );

        // Critical data should be FullHistory
        assert_eq!(
            SyncMode::default_for_collection("commands"),
            SyncMode::FullHistory
        );
        assert_eq!(
            SyncMode::default_for_collection("contact_reports"),
            SyncMode::FullHistory
        );

        // Track history should be windowed
        assert_eq!(
            SyncMode::default_for_collection("track_history"),
            SyncMode::WindowedHistory {
                window_seconds: 300
            }
        );

        // Unknown should default to FullHistory
        assert_eq!(
            SyncMode::default_for_collection("unknown"),
            SyncMode::FullHistory
        );
    }

    #[test]
    fn test_sync_mode_predicates() {
        assert!(SyncMode::FullHistory.is_full_history());
        assert!(!SyncMode::FullHistory.is_latest_only());
        assert!(!SyncMode::FullHistory.is_windowed());

        assert!(SyncMode::LatestOnly.is_latest_only());
        assert!(!SyncMode::LatestOnly.is_full_history());
        assert!(!SyncMode::LatestOnly.is_windowed());

        let windowed = SyncMode::WindowedHistory { window_seconds: 60 };
        assert!(windowed.is_windowed());
        assert!(!windowed.is_full_history());
        assert!(!windowed.is_latest_only());
        assert_eq!(windowed.window_seconds(), Some(60));
    }

    #[test]
    fn test_sync_mode_display() {
        assert_eq!(SyncMode::FullHistory.to_string(), "FullHistory");
        assert_eq!(SyncMode::LatestOnly.to_string(), "LatestOnly");
        assert_eq!(
            SyncMode::WindowedHistory {
                window_seconds: 300
            }
            .to_string(),
            "WindowedHistory(300s)"
        );
    }

    #[test]
    fn test_sync_mode_registry() {
        let registry = SyncModeRegistry::with_defaults();

        // Check defaults
        assert_eq!(registry.get("beacons"), SyncMode::LatestOnly);
        assert_eq!(registry.get("commands"), SyncMode::FullHistory);

        // Override
        registry.set("beacons", SyncMode::FullHistory);
        assert_eq!(registry.get("beacons"), SyncMode::FullHistory);

        // Remove override
        registry.remove("beacons");
        assert_eq!(registry.get("beacons"), SyncMode::LatestOnly);
    }

    #[test]
    fn test_sync_mode_registry_convenience_methods() {
        let registry = SyncModeRegistry::with_defaults();

        assert!(registry.is_latest_only("beacons"));
        assert!(!registry.is_latest_only("commands"));

        assert!(registry.is_full_history("commands"));
        assert!(!registry.is_full_history("beacons"));
    }

    #[test]
    fn test_sync_mode_serialization() {
        let mode = SyncMode::LatestOnly;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"LatestOnly\"");

        let deserialized: SyncMode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, SyncMode::LatestOnly);

        // Test windowed
        let windowed = SyncMode::WindowedHistory {
            window_seconds: 300,
        };
        let json = serde_json::to_string(&windowed).unwrap();
        let deserialized: SyncMode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, windowed);
    }

    #[test]
    fn test_sync_mode_default() {
        // Default should be FullHistory for backwards compatibility
        assert_eq!(SyncMode::default(), SyncMode::FullHistory);
    }
}
