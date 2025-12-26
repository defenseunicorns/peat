//! Gossip protocol strategies for mesh synchronization
//!
//! This module provides configurable gossip strategies that determine how
//! documents are propagated through the mesh. The key insight is that BLE
//! mesh sync does NOT require full n² connectivity - epidemic gossip
//! protocols achieve eventual consistency with O(log N) rounds.
//!
//! ## Gossip Protocol Fundamentals
//!
//! - **Push gossip**: Nodes proactively push updates to random peers
//! - **Pull gossip**: Nodes periodically request updates from peers
//! - **Push-pull**: Combines both for faster convergence
//!
//! For HIVE BLE mesh, we use push gossip with configurable fanout.
//!
//! ## Convergence Guarantees
//!
//! With fanout=2 and N nodes:
//! - Expected rounds to reach all nodes: O(log N)
//! - 10 nodes: ~4 rounds
//! - 20 nodes: ~5 rounds
//! - 50 nodes: ~6 rounds
//!
//! ## Usage
//!
//! ```rust
//! use hive_btle::gossip::{GossipStrategy, RandomFanout};
//! use hive_btle::peer::HivePeer;
//!
//! // Create a strategy with fanout of 2
//! let strategy = RandomFanout::new(2);
//!
//! // Select peers to gossip to
//! let peers: Vec<HivePeer> = vec![]; // your connected peers
//! let selected = strategy.select_peers(&peers);
//! ```

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::document::MergeResult;
use crate::peer::HivePeer;

/// Trait for gossip peer selection strategies
///
/// Implementations determine which subset of connected peers receive
/// each gossip message. The goal is efficient epidemic spread while
/// minimizing bandwidth and battery usage.
pub trait GossipStrategy: Send + Sync {
    /// Select peers to send a gossip message to
    ///
    /// Given the list of connected peers, return those that should
    /// receive the next gossip message. The selection should balance:
    /// - Convergence speed (more peers = faster)
    /// - Resource usage (fewer peers = less battery/bandwidth)
    fn select_peers<'a>(&self, peers: &'a [HivePeer]) -> Vec<&'a HivePeer>;

    /// Determine if an update should be forwarded after a merge
    ///
    /// Returns `true` if the merge result indicates new information
    /// that should be propagated to other peers.
    fn should_forward(&self, result: &MergeResult) -> bool {
        // Default: forward if counter or emergency state changed
        result.counter_changed || result.emergency_changed
    }

    /// Get the name of this strategy (for logging/debugging)
    fn name(&self) -> &'static str;
}

/// Random fanout gossip strategy
///
/// Selects a random subset of peers for each gossip round.
/// This is the classic epidemic gossip approach.
///
/// ## Fanout Selection
///
/// - **fanout=1**: Minimal, slow convergence, lowest overhead
/// - **fanout=2**: Standard, O(log N) convergence, good balance
/// - **fanout=3+**: Fast convergence, higher overhead
///
/// For most HIVE deployments, fanout=2 is recommended.
#[derive(Debug, Clone)]
pub struct RandomFanout {
    /// Number of peers to select per round
    fanout: usize,
    /// Random seed for deterministic testing (None = use system random)
    #[cfg(feature = "std")]
    seed: Option<u64>,
}

impl RandomFanout {
    /// Create a new random fanout strategy
    ///
    /// # Arguments
    /// * `fanout` - Number of peers to select per gossip round
    pub fn new(fanout: usize) -> Self {
        Self {
            fanout: fanout.max(1), // At least 1
            #[cfg(feature = "std")]
            seed: None,
        }
    }

    /// Create with a fixed seed for deterministic testing
    #[cfg(feature = "std")]
    pub fn with_seed(fanout: usize, seed: u64) -> Self {
        Self {
            fanout: fanout.max(1),
            seed: Some(seed),
        }
    }

    /// Get a pseudo-random number
    #[cfg(feature = "std")]
    fn random_index(&self, max: usize, iteration: usize) -> usize {
        use std::time::SystemTime;

        let seed = self.seed.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(12345)
        });

        // Simple LCG for lightweight randomness
        let mixed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(iteration as u64);
        (mixed as usize) % max
    }
}

impl Default for RandomFanout {
    fn default() -> Self {
        Self::new(2) // Default fanout of 2
    }
}

impl GossipStrategy for RandomFanout {
    fn select_peers<'a>(&self, peers: &'a [HivePeer]) -> Vec<&'a HivePeer> {
        if peers.is_empty() {
            return Vec::new();
        }

        // If we have fewer peers than fanout, return all
        if peers.len() <= self.fanout {
            return peers.iter().collect();
        }

        // Select random subset
        #[cfg(feature = "std")]
        {
            let mut selected = Vec::with_capacity(self.fanout);
            let mut used = std::collections::HashSet::new();

            for i in 0..self.fanout * 3 {
                // Try up to 3x fanout to find unique peers
                if selected.len() >= self.fanout {
                    break;
                }

                let idx = self.random_index(peers.len(), i);
                if !used.contains(&idx) {
                    used.insert(idx);
                    selected.push(&peers[idx]);
                }
            }

            selected
        }

        #[cfg(not(feature = "std"))]
        {
            // No_std fallback: just take first N peers
            peers.iter().take(self.fanout).collect()
        }
    }

    fn name(&self) -> &'static str {
        "random_fanout"
    }
}

/// Broadcast-all strategy
///
/// Sends to all connected peers. Use only for:
/// - Very small meshes (< 5 nodes)
/// - Emergency situations requiring immediate propagation
/// - Testing/debugging
///
/// **Warning**: This is O(N) per round - not suitable for large meshes.
#[derive(Debug, Clone, Default)]
pub struct BroadcastAll;

impl BroadcastAll {
    /// Create a new broadcast-all strategy
    pub fn new() -> Self {
        Self
    }
}

impl GossipStrategy for BroadcastAll {
    fn select_peers<'a>(&self, peers: &'a [HivePeer]) -> Vec<&'a HivePeer> {
        peers.iter().collect()
    }

    fn name(&self) -> &'static str {
        "broadcast_all"
    }
}

/// Signal-strength based selection
///
/// Prefers peers with stronger signal (better reliability).
/// Falls back to random selection for peers with similar signal.
#[derive(Debug, Clone)]
pub struct SignalBasedFanout {
    /// Number of peers to select
    fanout: usize,
    /// Minimum RSSI difference to prefer one peer over another
    rssi_threshold: i8,
}

impl SignalBasedFanout {
    /// Create a new signal-based strategy
    ///
    /// # Arguments
    /// * `fanout` - Number of peers to select
    /// * `rssi_threshold` - RSSI difference (dB) to consider significant
    pub fn new(fanout: usize, rssi_threshold: i8) -> Self {
        Self {
            fanout: fanout.max(1),
            rssi_threshold,
        }
    }
}

impl Default for SignalBasedFanout {
    fn default() -> Self {
        Self::new(2, 10) // Default: 2 peers, 10dB threshold
    }
}

impl GossipStrategy for SignalBasedFanout {
    fn select_peers<'a>(&self, peers: &'a [HivePeer]) -> Vec<&'a HivePeer> {
        if peers.is_empty() {
            return Vec::new();
        }

        if peers.len() <= self.fanout {
            return peers.iter().collect();
        }

        // Sort by signal strength (higher RSSI = better)
        let mut sorted: Vec<_> = peers.iter().collect();
        sorted.sort_by(|a, b| b.rssi.cmp(&a.rssi));

        // Take the best ones, but add some randomness for diversity
        let mut selected: Vec<&HivePeer> = Vec::with_capacity(self.fanout);

        // Always include the strongest peer
        if let Some(best) = sorted.first() {
            selected.push(best);
        }

        // For remaining slots, prefer strong signals but allow some diversity
        for peer in sorted.iter().skip(1) {
            if selected.len() >= self.fanout {
                break;
            }

            // Check if this peer is significantly weaker than the last selected
            let last_rssi = selected.last().map(|p| p.rssi).unwrap_or(-100);
            let this_rssi = peer.rssi;

            // Include if within threshold or we need more peers
            if this_rssi >= last_rssi - self.rssi_threshold || selected.len() < self.fanout / 2 + 1
            {
                selected.push(peer);
            }
        }

        // Fill remaining slots if needed
        for peer in sorted.iter() {
            if selected.len() >= self.fanout {
                break;
            }
            // Check by node_id to avoid requiring PartialEq on HivePeer
            let already_selected = selected.iter().any(|p| p.node_id == peer.node_id);
            if !already_selected {
                selected.push(peer);
            }
        }

        selected
    }

    fn name(&self) -> &'static str {
        "signal_based"
    }
}

/// Emergency broadcast strategy
///
/// For emergency events, use maximum fanout to ensure rapid propagation.
/// Automatically switches between normal and emergency modes.
#[derive(Debug)]
pub struct EmergencyAware {
    /// Normal operation strategy
    normal_fanout: usize,
    /// Emergency fanout (usually all peers)
    emergency_fanout: usize,
    /// Whether we're in emergency mode
    #[cfg(feature = "std")]
    emergency_mode: std::sync::atomic::AtomicBool,
}

impl Clone for EmergencyAware {
    fn clone(&self) -> Self {
        Self {
            normal_fanout: self.normal_fanout,
            emergency_fanout: self.emergency_fanout,
            #[cfg(feature = "std")]
            emergency_mode: std::sync::atomic::AtomicBool::new(self.is_emergency()),
        }
    }
}

impl EmergencyAware {
    /// Create a new emergency-aware strategy
    pub fn new(normal_fanout: usize) -> Self {
        Self {
            normal_fanout: normal_fanout.max(1),
            emergency_fanout: usize::MAX, // All peers during emergency
            #[cfg(feature = "std")]
            emergency_mode: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Set emergency mode
    #[cfg(feature = "std")]
    pub fn set_emergency(&self, active: bool) {
        self.emergency_mode
            .store(active, std::sync::atomic::Ordering::SeqCst);
    }

    /// Check if in emergency mode
    #[cfg(feature = "std")]
    pub fn is_emergency(&self) -> bool {
        self.emergency_mode
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    fn effective_fanout(&self) -> usize {
        #[cfg(feature = "std")]
        {
            if self.is_emergency() {
                self.emergency_fanout
            } else {
                self.normal_fanout
            }
        }
        #[cfg(not(feature = "std"))]
        {
            self.normal_fanout
        }
    }
}

impl Default for EmergencyAware {
    fn default() -> Self {
        Self::new(2)
    }
}

impl GossipStrategy for EmergencyAware {
    fn select_peers<'a>(&self, peers: &'a [HivePeer]) -> Vec<&'a HivePeer> {
        let fanout = self.effective_fanout();

        if peers.len() <= fanout {
            return peers.iter().collect();
        }

        // During emergency: all peers
        // Normal: use random fanout behavior
        peers.iter().take(fanout).collect()
    }

    fn should_forward(&self, result: &MergeResult) -> bool {
        // Always forward during emergency mode
        #[cfg(feature = "std")]
        if self.is_emergency() {
            return true;
        }

        // Switch to emergency mode if we received an emergency
        #[cfg(feature = "std")]
        if result.is_emergency() || result.emergency_changed {
            self.set_emergency(true);
        }

        result.counter_changed || result.emergency_changed
    }

    fn name(&self) -> &'static str {
        "emergency_aware"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeId;

    fn make_peer(id: u32, rssi: i8) -> HivePeer {
        HivePeer {
            node_id: NodeId::new(id),
            identifier: format!("device-{}", id),
            mesh_id: Some("TEST".to_string()),
            name: Some(format!("HIVE-{:08X}", id)),
            rssi,
            is_connected: true,
            last_seen_ms: 0,
        }
    }

    #[test]
    fn test_random_fanout_basic() {
        let strategy = RandomFanout::new(2);

        // Empty peers
        let peers: Vec<HivePeer> = vec![];
        assert!(strategy.select_peers(&peers).is_empty());

        // Fewer peers than fanout
        let peers = vec![make_peer(1, -50)];
        let selected = strategy.select_peers(&peers);
        assert_eq!(selected.len(), 1);

        // More peers than fanout
        let peers = vec![
            make_peer(1, -50),
            make_peer(2, -60),
            make_peer(3, -70),
            make_peer(4, -80),
        ];
        let selected = strategy.select_peers(&peers);
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn test_broadcast_all() {
        let strategy = BroadcastAll::new();

        let peers = vec![make_peer(1, -50), make_peer(2, -60), make_peer(3, -70)];

        let selected = strategy.select_peers(&peers);
        assert_eq!(selected.len(), 3);
    }

    #[test]
    fn test_signal_based() {
        let strategy = SignalBasedFanout::new(2, 10);

        let peers = vec![
            make_peer(1, -80), // Weak
            make_peer(2, -50), // Strong
            make_peer(3, -90), // Very weak
            make_peer(4, -55), // Strong-ish
        ];

        let selected = strategy.select_peers(&peers);
        assert_eq!(selected.len(), 2);

        // Should prefer stronger signals
        let node_ids: Vec<_> = selected.iter().map(|p| p.node_id.as_u32()).collect();
        assert!(node_ids.contains(&2)); // Strongest should be included
    }

    #[test]
    fn test_emergency_aware() {
        let strategy = EmergencyAware::new(2);

        let peers = vec![
            make_peer(1, -50),
            make_peer(2, -60),
            make_peer(3, -70),
            make_peer(4, -80),
        ];

        // Normal mode: limited fanout
        assert!(!strategy.is_emergency());
        let selected = strategy.select_peers(&peers);
        assert_eq!(selected.len(), 2);

        // Emergency mode: all peers
        strategy.set_emergency(true);
        assert!(strategy.is_emergency());
        let selected = strategy.select_peers(&peers);
        assert_eq!(selected.len(), 4);
    }

    #[test]
    fn test_should_forward() {
        let strategy = RandomFanout::default();

        // Should forward if counter changed
        let result = MergeResult {
            source_node: NodeId::new(1),
            event: None,
            counter_changed: true,
            emergency_changed: false,
            total_count: 10,
        };
        assert!(strategy.should_forward(&result));

        // Should forward if emergency changed
        let result = MergeResult {
            source_node: NodeId::new(1),
            event: None,
            counter_changed: false,
            emergency_changed: true,
            total_count: 10,
        };
        assert!(strategy.should_forward(&result));

        // Should NOT forward if nothing changed
        let result = MergeResult {
            source_node: NodeId::new(1),
            event: None,
            counter_changed: false,
            emergency_changed: false,
            total_count: 10,
        };
        assert!(!strategy.should_forward(&result));
    }
}
