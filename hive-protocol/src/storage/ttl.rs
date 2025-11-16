//! TTL and Data Lifecycle Management
//!
//! Provides helpers for managing document lifecycle using Ditto's native deletion model:
//! - **Soft-delete pattern**: Avoids husking on high-churn data (beacons, positions)
//! - **EVICT helpers**: Local storage cleanup for edge devices
//! - **Tombstone TTL config**: Configure Ditto's native tombstone reaping
//! - **Offline retention**: Connectivity-aware eviction policies
//!
//! ## Ditto's Deletion Model
//!
//! Ditto provides two deletion mechanisms:
//!
//! 1. **DELETE/EVICT**: Creates tombstones that sync mesh-wide and auto-reap after `TOMBSTONE_TTL_HOURS`
//! 2. **EVICT (local)**: Removes documents from local storage only (no tombstone, may re-sync)
//!
//! ### Tombstone TTL Configuration
//!
//! Set via `ALTER SYSTEM` or environment variables:
//! ```sql
//! ALTER SYSTEM SET TOMBSTONE_TTL_ENABLED = true
//! ALTER SYSTEM SET TOMBSTONE_TTL_HOURS = 168  -- 7 days
//! ```
//!
//! **Critical**: Never set Edge SDK TTL > Server TTL (Edge: 7 days, Cloud: 30 days)
//!
//! ## Architectural Challenges
//!
//! ### Delete-Then-Disconnect Problem
//!
//! When a node deletes a document then goes offline, other nodes that were offline
//! during the delete may resurrect the document when they come back online with stale data.
//!
//! **Solution**: Tombstones persist for `TOMBSTONE_TTL_HOURS`, ensuring deletions propagate
//! even to nodes offline during the delete operation.
//!
//! ### Concurrent Delete-Update (Husking)
//!
//! When one node updates a field while another deletes the document, CRDT merge can
//! create "husked documents" where updated fields exist but all others are null.
//!
//! **Solution**: Use soft-delete pattern for high-churn data (update `_deleted` flag instead
//! of deleting).
//!
//! ## Usage
//!
//! ```ignore
//! use hive_protocol::storage::ttl::{TtlConfig, EvictionStrategy};
//! use std::time::Duration;
//!
//! // Configure for tactical operations
//! let config = TtlConfig::tactical();
//!
//! // Or custom configuration
//! let config = TtlConfig::new()
//!     .with_beacon_ttl(Duration::from_secs(300))  // 5 min soft-delete
//!     .with_eviction(EvictionStrategy::OldestFirst);
//! ```

use std::collections::HashMap;
use std::time::Duration;

/// TTL configuration for data lifecycle management
///
/// Provides collection-specific TTLs and eviction strategies that coordinate
/// with Ditto's native tombstone TTL.
#[derive(Debug, Clone)]
pub struct TtlConfig {
    /// Ditto tombstone TTL (hours)
    ///
    /// **Must be configured via ALTER SYSTEM or environment variable before heavy deletion workload**
    ///
    /// ```sql
    /// ALTER SYSTEM SET TOMBSTONE_TTL_HOURS = 168
    /// ```
    ///
    /// Or via environment variable:
    /// ```bash
    /// export TOMBSTONE_TTL_HOURS=168
    /// ```
    pub tombstone_ttl_hours: u32,

    /// Enable automatic tombstone reaping
    pub tombstone_reaping_enabled: bool,

    /// Days between tombstone reaping scans (default: 1)
    pub days_between_reaping: u32,

    /// Beacon soft-delete TTL (avoids husking)
    ///
    /// Beacons are high-churn data. Use soft-delete pattern to mark as deleted
    /// rather than creating tombstones.
    pub beacon_ttl: Duration,

    /// Node position soft-delete TTL (avoids husking)
    pub position_ttl: Duration,

    /// Capability hard-delete TTL
    ///
    /// Capabilities are low-churn, coordinated updates. Safe to use hard delete (EVICT).
    pub capability_ttl: Duration,

    /// EVICT strategy for edge devices with storage constraints
    pub evict_strategy: EvictionStrategy,

    /// Offline retention policy (optional)
    pub offline_policy: Option<OfflineRetentionPolicy>,
}

/// Eviction strategy when storage limits are reached or node is offline
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub enum EvictionStrategy {
    /// Evict oldest documents first (by last_updated_at)
    OldestFirst,

    /// Evict based on storage pressure threshold
    StoragePressure {
        /// Percentage threshold (0-100) to trigger eviction
        threshold_pct: u8,
    },

    /// Keep only last N documents per collection
    KeepLastN(usize),

    /// No automatic eviction
    None,
}

/// Offline retention policy for connectivity-aware lifecycle
///
/// When a node is offline, it cannot contribute to the mesh anyway.
/// More aggressive eviction saves storage and battery while preserving
/// the ability to re-sync latest state from peers when reconnected.
#[derive(Debug, Clone)]
pub struct OfflineRetentionPolicy {
    /// TTL when node is online (normal retention)
    pub online_ttl: Duration,

    /// TTL when node is offline (aggressive eviction)
    ///
    /// Typically 10x shorter than online_ttl to free resources
    pub offline_ttl: Duration,

    /// Always keep last N items per collection, even when offline
    pub keep_last_n: usize,
}

impl TtlConfig {
    /// Create a new TTL configuration with conservative defaults
    pub fn new() -> Self {
        Self {
            tombstone_ttl_hours: 168, // 7 days (Edge SDK default)
            tombstone_reaping_enabled: true,
            days_between_reaping: 1,
            beacon_ttl: Duration::from_secs(600),      // 10 min
            position_ttl: Duration::from_secs(600),    // 10 min
            capability_ttl: Duration::from_secs(7200), // 2 hours
            evict_strategy: EvictionStrategy::None,
            offline_policy: None,
        }
    }

    /// Tactical operations preset (high-frequency updates, short TTLs)
    ///
    /// Optimized for:
    /// - Rapid beacon discovery (5 min TTL)
    /// - Frequent position updates (10 min TTL)
    /// - Short-lived capabilities (2 hour TTL)
    /// - Offline nodes keep only last 10 items
    pub fn tactical() -> Self {
        Self {
            tombstone_ttl_hours: 168, // 7 days
            tombstone_reaping_enabled: true,
            days_between_reaping: 1,
            beacon_ttl: Duration::from_secs(300), // 5 min soft-delete
            position_ttl: Duration::from_secs(600), // 10 min soft-delete
            capability_ttl: Duration::from_secs(7200), // 2 hours
            evict_strategy: EvictionStrategy::OldestFirst,
            offline_policy: Some(OfflineRetentionPolicy {
                online_ttl: Duration::from_secs(600), // 10 min
                offline_ttl: Duration::from_secs(60), // 1 min when offline
                keep_last_n: 10,
            }),
        }
    }

    /// Long-duration operations preset (ISR, surveillance)
    ///
    /// Optimized for:
    /// - Longer beacon retention (10 min)
    /// - Extended position history (1 hour)
    /// - Long-lived capabilities (48 hours)
    pub fn long_duration() -> Self {
        Self {
            tombstone_ttl_hours: 168, // 7 days
            tombstone_reaping_enabled: true,
            days_between_reaping: 1,
            beacon_ttl: Duration::from_secs(600),    // 10 min
            position_ttl: Duration::from_secs(3600), // 1 hour
            capability_ttl: Duration::from_secs(172800), // 48 hours
            evict_strategy: EvictionStrategy::StoragePressure { threshold_pct: 80 },
            offline_policy: None, // No special offline handling
        }
    }

    /// Offline/storage-constrained node preset
    ///
    /// Optimized for:
    /// - Minimal storage footprint
    /// - Aggressive local eviction (EVICT, not DELETE)
    /// - Short tombstone TTL (3 days instead of 7)
    pub fn offline_node() -> Self {
        Self {
            tombstone_ttl_hours: 72, // 3 days (shorter for storage-constrained edge)
            tombstone_reaping_enabled: true,
            days_between_reaping: 1,
            beacon_ttl: Duration::from_secs(30),   // 30 sec EVICT
            position_ttl: Duration::from_secs(60), // 1 min EVICT
            capability_ttl: Duration::from_secs(300), // 5 min EVICT
            evict_strategy: EvictionStrategy::KeepLastN(10),
            offline_policy: Some(OfflineRetentionPolicy {
                online_ttl: Duration::from_secs(300), // 5 min
                offline_ttl: Duration::from_secs(30), // 30 sec when offline
                keep_last_n: 5,                       // Minimal retention
            }),
        }
    }

    /// Set beacon soft-delete TTL
    pub fn with_beacon_ttl(mut self, ttl: Duration) -> Self {
        self.beacon_ttl = ttl;
        self
    }

    /// Set position soft-delete TTL
    pub fn with_position_ttl(mut self, ttl: Duration) -> Self {
        self.position_ttl = ttl;
        self
    }

    /// Set capability hard-delete TTL
    pub fn with_capability_ttl(mut self, ttl: Duration) -> Self {
        self.capability_ttl = ttl;
        self
    }

    /// Set eviction strategy
    pub fn with_eviction(mut self, strategy: EvictionStrategy) -> Self {
        self.evict_strategy = strategy;
        self
    }

    /// Set offline retention policy
    pub fn with_offline_policy(mut self, policy: OfflineRetentionPolicy) -> Self {
        self.offline_policy = Some(policy);
        self
    }

    /// Set Ditto tombstone TTL (hours)
    ///
    /// **Warning**: This only configures the config struct. You must also
    /// execute the ALTER SYSTEM command or set environment variable.
    pub fn with_tombstone_ttl(mut self, hours: u32) -> Self {
        self.tombstone_ttl_hours = hours;
        self
    }

    /// Get TTL for a specific collection
    ///
    /// Returns the appropriate TTL based on collection name and current configuration.
    pub fn get_collection_ttl(&self, collection: &str) -> Option<Duration> {
        match collection {
            "beacons" => Some(self.beacon_ttl),
            "node_positions" => Some(self.position_ttl),
            "capabilities" => Some(self.capability_ttl),
            "hierarchical_commands" => None, // Manual expiration via TimeoutManager
            "cells" => Some(Duration::from_secs(3600)), // 1 hour
            _ => None,
        }
    }

    /// Generate ALTER SYSTEM commands for Ditto tombstone configuration
    ///
    /// Returns DQL statements to configure Ditto's native tombstone TTL.
    ///
    /// **Usage**: Execute these via `DittoStore::execute()` before heavy deletion workload.
    ///
    /// ```ignore
    /// for statement in config.ditto_alter_system_commands() {
    ///     store.execute(&statement, serde_json::json!({})).await?;
    /// }
    /// ```
    pub fn ditto_alter_system_commands(&self) -> Vec<String> {
        vec![
            format!(
                "ALTER SYSTEM SET TOMBSTONE_TTL_ENABLED = {}",
                self.tombstone_reaping_enabled
            ),
            format!(
                "ALTER SYSTEM SET TOMBSTONE_TTL_HOURS = {}",
                self.tombstone_ttl_hours
            ),
            format!(
                "ALTER SYSTEM SET DAYS_BETWEEN_REAPING = {}",
                self.days_between_reaping
            ),
        ]
    }

    /// Generate environment variable exports for Ditto configuration
    ///
    /// Returns shell commands to set environment variables for scheduling parameters.
    ///
    /// **Note**: Scheduling parameters (REAPER_PREFERRED_HOUR) must be set via env vars
    /// before starting Ditto, not at runtime.
    pub fn ditto_env_vars(&self) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        vars.insert(
            "TOMBSTONE_TTL_ENABLED".to_string(),
            self.tombstone_reaping_enabled.to_string(),
        );
        vars.insert(
            "TOMBSTONE_TTL_HOURS".to_string(),
            self.tombstone_ttl_hours.to_string(),
        );
        vars.insert(
            "DAYS_BETWEEN_REAPING".to_string(),
            self.days_between_reaping.to_string(),
        );
        vars
    }
}

impl Default for TtlConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl OfflineRetentionPolicy {
    /// Create a minimal retention policy for storage-constrained devices
    pub fn minimal() -> Self {
        Self {
            online_ttl: Duration::from_secs(300), // 5 min
            offline_ttl: Duration::from_secs(30), // 30 sec
            keep_last_n: 5,
        }
    }

    /// Create a conservative retention policy
    pub fn conservative() -> Self {
        Self {
            online_ttl: Duration::from_secs(3600), // 1 hour
            offline_ttl: Duration::from_secs(300), // 5 min
            keep_last_n: 20,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tactical_preset() {
        let config = TtlConfig::tactical();

        assert_eq!(config.tombstone_ttl_hours, 168); // 7 days
        assert_eq!(config.beacon_ttl, Duration::from_secs(300)); // 5 min
        assert_eq!(config.position_ttl, Duration::from_secs(600)); // 10 min
        assert!(config.offline_policy.is_some());

        let offline = config.offline_policy.unwrap();
        assert_eq!(offline.keep_last_n, 10);
    }

    #[test]
    fn test_long_duration_preset() {
        let config = TtlConfig::long_duration();

        assert_eq!(config.beacon_ttl, Duration::from_secs(600)); // 10 min
        assert_eq!(config.position_ttl, Duration::from_secs(3600)); // 1 hour
        assert_eq!(config.capability_ttl, Duration::from_secs(172800)); // 48 hours
        assert!(config.offline_policy.is_none());
    }

    #[test]
    fn test_offline_node_preset() {
        let config = TtlConfig::offline_node();

        assert_eq!(config.tombstone_ttl_hours, 72); // 3 days (shorter)
        assert_eq!(config.beacon_ttl, Duration::from_secs(30)); // 30 sec
        assert_eq!(config.position_ttl, Duration::from_secs(60)); // 1 min

        match config.evict_strategy {
            EvictionStrategy::KeepLastN(n) => assert_eq!(n, 10),
            _ => panic!("Expected KeepLastN strategy"),
        }
    }

    #[test]
    fn test_collection_ttl_lookup() {
        let config = TtlConfig::tactical();

        assert_eq!(
            config.get_collection_ttl("beacons"),
            Some(Duration::from_secs(300))
        );
        assert_eq!(
            config.get_collection_ttl("node_positions"),
            Some(Duration::from_secs(600))
        );
        assert_eq!(config.get_collection_ttl("hierarchical_commands"), None);
    }

    #[test]
    fn test_builder_pattern() {
        let config = TtlConfig::new()
            .with_beacon_ttl(Duration::from_secs(120))
            .with_eviction(EvictionStrategy::OldestFirst)
            .with_tombstone_ttl(96); // 4 days

        assert_eq!(config.beacon_ttl, Duration::from_secs(120));
        assert_eq!(config.tombstone_ttl_hours, 96);
        assert!(matches!(
            config.evict_strategy,
            EvictionStrategy::OldestFirst
        ));
    }

    #[test]
    fn test_alter_system_commands() {
        let config = TtlConfig::tactical();
        let commands = config.ditto_alter_system_commands();

        assert_eq!(commands.len(), 3);
        assert!(commands[0].contains("TOMBSTONE_TTL_ENABLED"));
        assert!(commands[1].contains("TOMBSTONE_TTL_HOURS = 168"));
        assert!(commands[2].contains("DAYS_BETWEEN_REAPING = 1"));
    }

    #[test]
    fn test_env_vars() {
        let config = TtlConfig::tactical();
        let env_vars = config.ditto_env_vars();

        assert_eq!(
            env_vars.get("TOMBSTONE_TTL_HOURS"),
            Some(&"168".to_string())
        );
        assert_eq!(
            env_vars.get("TOMBSTONE_TTL_ENABLED"),
            Some(&"true".to_string())
        );
        assert_eq!(env_vars.get("DAYS_BETWEEN_REAPING"), Some(&"1".to_string()));
    }

    #[test]
    fn test_offline_retention_presets() {
        let minimal = OfflineRetentionPolicy::minimal();
        assert_eq!(minimal.offline_ttl, Duration::from_secs(30));
        assert_eq!(minimal.keep_last_n, 5);

        let conservative = OfflineRetentionPolicy::conservative();
        assert_eq!(conservative.online_ttl, Duration::from_secs(3600));
        assert_eq!(conservative.keep_last_n, 20);
    }

    #[test]
    fn test_eviction_strategy_variants() {
        let oldest = EvictionStrategy::OldestFirst;
        let pressure = EvictionStrategy::StoragePressure { threshold_pct: 80 };
        let keep_n = EvictionStrategy::KeepLastN(100);
        let none = EvictionStrategy::None;

        assert!(matches!(oldest, EvictionStrategy::OldestFirst));
        assert!(matches!(
            pressure,
            EvictionStrategy::StoragePressure { threshold_pct: 80 }
        ));
        assert!(matches!(keep_n, EvictionStrategy::KeepLastN(100)));
        assert!(matches!(none, EvictionStrategy::None));
    }

    #[test]
    fn test_tombstone_ttl_edge_constraint() {
        // Edge SDK should never exceed server TTL
        let edge_config = TtlConfig::tactical();
        assert!(edge_config.tombstone_ttl_hours <= 720); // 30 days server default

        // Offline node uses shorter TTL for storage constraints
        let offline_config = TtlConfig::offline_node();
        assert!(offline_config.tombstone_ttl_hours < edge_config.tombstone_ttl_hours);
    }
}
