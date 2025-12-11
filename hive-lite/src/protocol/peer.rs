//! Peer Management
//!
//! Tracks known peers in the mesh network.

use super::capabilities::NodeCapabilities;
use heapless::FnvIndexMap;

/// Maximum number of peers to track
const MAX_PEERS: usize = 16;

/// Information about a peer node
#[derive(Debug, Clone)]
pub struct Peer {
    /// Peer's node ID
    pub node_id: u32,
    /// Peer's IP address (as bytes)
    pub addr: [u8; 4],
    /// Peer's port
    pub port: u16,
    /// Peer's capabilities
    pub capabilities: NodeCapabilities,
    /// Last sequence number seen from this peer
    pub last_seq: u32,
    /// Timestamp of last message from peer (in milliseconds since boot)
    pub last_seen: u64,
    /// Number of consecutive missed heartbeats
    pub missed_heartbeats: u8,
}

impl Peer {
    /// Create a new peer
    pub fn new(node_id: u32, addr: [u8; 4], port: u16, capabilities: NodeCapabilities) -> Self {
        Self {
            node_id,
            addr,
            port,
            capabilities,
            last_seq: 0,
            last_seen: 0,
            missed_heartbeats: 0,
        }
    }

    /// Update peer's last seen time and sequence
    pub fn update(&mut self, seq: u32, now: u64) {
        if seq > self.last_seq {
            self.last_seq = seq;
        }
        self.last_seen = now;
        self.missed_heartbeats = 0;
    }

    /// Check if peer is considered stale
    pub fn is_stale(&self, now: u64, timeout_ms: u64) -> bool {
        now.saturating_sub(self.last_seen) > timeout_ms
    }

    /// Format address as string (for logging)
    pub fn addr_str(&self) -> heapless::String<21> {
        let mut s = heapless::String::new();
        use core::fmt::Write;
        write!(
            s,
            "{}.{}.{}.{}:{}",
            self.addr[0], self.addr[1], self.addr[2], self.addr[3], self.port
        )
        .ok();
        s
    }
}

/// Table of known peers
#[derive(Debug)]
pub struct PeerTable {
    peers: FnvIndexMap<u32, Peer, MAX_PEERS>,
}

impl PeerTable {
    /// Create a new empty peer table
    pub fn new() -> Self {
        Self {
            peers: FnvIndexMap::new(),
        }
    }

    /// Add or update a peer
    pub fn upsert(&mut self, peer: Peer) -> bool {
        if let Some(existing) = self.peers.get_mut(&peer.node_id) {
            existing.addr = peer.addr;
            existing.port = peer.port;
            existing.capabilities = peer.capabilities;
            existing.last_seen = peer.last_seen;
            existing.last_seq = peer.last_seq;
            existing.missed_heartbeats = 0;
            true
        } else {
            self.peers.insert(peer.node_id, peer).is_ok()
        }
    }

    /// Get a peer by node ID
    pub fn get(&self, node_id: u32) -> Option<&Peer> {
        self.peers.get(&node_id)
    }

    /// Get a mutable peer by node ID
    pub fn get_mut(&mut self, node_id: u32) -> Option<&mut Peer> {
        self.peers.get_mut(&node_id)
    }

    /// Remove a peer
    pub fn remove(&mut self, node_id: u32) -> Option<Peer> {
        self.peers.remove(&node_id)
    }

    /// Check if we know about a peer
    pub fn contains(&self, node_id: u32) -> bool {
        self.peers.contains_key(&node_id)
    }

    /// Get number of known peers
    pub fn len(&self) -> usize {
        self.peers.len()
    }

    /// Check if peer table is empty
    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }

    /// Iterate over all peers
    pub fn iter(&self) -> impl Iterator<Item = &Peer> {
        self.peers.values()
    }

    /// Remove stale peers (not seen within timeout)
    pub fn remove_stale(&mut self, now: u64, timeout_ms: u64) -> usize {
        let stale_ids: heapless::Vec<u32, MAX_PEERS> = self
            .peers
            .iter()
            .filter(|(_, p)| p.is_stale(now, timeout_ms))
            .map(|(&id, _)| id)
            .collect();

        let count = stale_ids.len();
        for id in stale_ids {
            self.peers.remove(&id);
        }
        count
    }

    /// Get peers that support specific capabilities
    pub fn peers_with_capability(&self, cap: u16) -> impl Iterator<Item = &Peer> {
        self.peers.values().filter(move |p| p.capabilities.has(cap))
    }

    /// Increment missed heartbeat count for all peers
    /// Returns list of peers that exceeded max misses
    pub fn tick_heartbeats(&mut self, max_misses: u8) -> heapless::Vec<u32, MAX_PEERS> {
        let mut expired = heapless::Vec::new();

        for (&node_id, peer) in self.peers.iter_mut() {
            peer.missed_heartbeats = peer.missed_heartbeats.saturating_add(1);
            if peer.missed_heartbeats > max_misses {
                expired.push(node_id).ok();
            }
        }

        // Remove expired peers
        for id in &expired {
            self.peers.remove(id);
        }

        expired
    }
}

impl Default for PeerTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_table_add() {
        let mut table = PeerTable::new();
        let peer = Peer::new(1, [192, 168, 1, 100], 4872, NodeCapabilities::lite());

        assert!(table.upsert(peer));
        assert_eq!(table.len(), 1);
        assert!(table.contains(1));
    }

    #[test]
    fn test_peer_table_update() {
        let mut table = PeerTable::new();
        let peer1 = Peer::new(1, [192, 168, 1, 100], 4872, NodeCapabilities::lite());
        table.upsert(peer1);

        let mut peer2 = Peer::new(1, [192, 168, 1, 200], 4872, NodeCapabilities::full());
        peer2.last_seen = 1000;
        table.upsert(peer2);

        assert_eq!(table.len(), 1);
        let peer = table.get(1).unwrap();
        assert_eq!(peer.addr, [192, 168, 1, 200]);
        assert_eq!(peer.last_seen, 1000);
    }

    #[test]
    fn test_peer_stale_removal() {
        let mut table = PeerTable::new();

        let mut peer1 = Peer::new(1, [192, 168, 1, 100], 4872, NodeCapabilities::lite());
        peer1.last_seen = 0;
        table.upsert(peer1);

        let mut peer2 = Peer::new(2, [192, 168, 1, 101], 4872, NodeCapabilities::lite());
        peer2.last_seen = 9000;
        table.upsert(peer2);

        // Remove peers not seen in last 5000ms
        let removed = table.remove_stale(10000, 5000);
        assert_eq!(removed, 1);
        assert_eq!(table.len(), 1);
        assert!(!table.contains(1));
        assert!(table.contains(2));
    }
}
