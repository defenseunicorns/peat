//! Gossip Protocol State Machine
//!
//! Manages the state of the gossip protocol including peer discovery,
//! heartbeats, and data synchronization.

use super::capabilities::NodeCapabilities;
use super::message::{CrdtType, Message, MessageType, MAX_PACKET_SIZE};
use super::peer::{Peer, PeerTable};
use heapless::Vec;

/// Heartbeat interval in milliseconds
pub const HEARTBEAT_INTERVAL_MS: u64 = 5000;

/// Peer timeout in milliseconds (3 missed heartbeats)
pub const PEER_TIMEOUT_MS: u64 = HEARTBEAT_INTERVAL_MS * 3 + 1000;

/// Gossip protocol state
pub struct GossipState {
    /// This node's ID
    pub node_id: u32,
    /// This node's capabilities
    pub capabilities: NodeCapabilities,
    /// Current sequence number
    seq_num: u32,
    /// Known peers
    pub peers: PeerTable,
    /// Last heartbeat send time
    last_heartbeat: u64,
    /// Pending outbound messages
    outbound: Vec<OutboundMessage, 8>,
}

/// Message queued for sending
#[derive(Debug, Clone)]
pub struct OutboundMessage {
    pub target: MessageTarget,
    pub data: Vec<u8, MAX_PACKET_SIZE>,
}

/// Target for outbound messages
#[derive(Debug, Clone, Copy)]
pub enum MessageTarget {
    /// Send to specific peer
    Unicast { addr: [u8; 4], port: u16 },
    /// Send to multicast group
    Multicast,
    /// Broadcast to all known peers
    AllPeers,
}

impl GossipState {
    /// Create new gossip state
    pub fn new(node_id: u32, capabilities: NodeCapabilities) -> Self {
        Self {
            node_id,
            capabilities,
            seq_num: 0,
            peers: PeerTable::new(),
            last_heartbeat: 0,
            outbound: Vec::new(),
        }
    }

    /// Get next sequence number
    fn next_seq(&mut self) -> u32 {
        self.seq_num = self.seq_num.wrapping_add(1);
        self.seq_num
    }

    /// Process a received message
    pub fn handle_message(
        &mut self,
        msg: &Message,
        from_addr: [u8; 4],
        from_port: u16,
        now: u64,
    ) -> HandleResult {
        match msg.msg_type {
            MessageType::Announce => self.handle_announce(msg, from_addr, from_port, now),
            MessageType::Heartbeat => self.handle_heartbeat(msg, from_addr, from_port, now),
            MessageType::Data => self.handle_data(msg, now),
            MessageType::Query => self.handle_query(msg, from_addr, from_port),
            MessageType::Ack => HandleResult::Ok,
            MessageType::Leave => self.handle_leave(msg),
            // OTA messages are handled directly in wifi_main.rs, not via GossipState
            _ => HandleResult::Ok,
        }
    }

    fn handle_announce(
        &mut self,
        msg: &Message,
        addr: [u8; 4],
        port: u16,
        now: u64,
    ) -> HandleResult {
        // Parse capabilities from payload
        let capabilities = if msg.payload.len() >= 2 {
            NodeCapabilities::decode([msg.payload[0], msg.payload[1]])
        } else {
            NodeCapabilities::empty()
        };

        let mut peer = Peer::new(msg.node_id, addr, port, capabilities);
        peer.last_seen = now;
        peer.last_seq = msg.seq_num;

        let is_new = !self.peers.contains(msg.node_id);
        self.peers.upsert(peer);

        if is_new {
            // Send our own announce back
            self.queue_announce(MessageTarget::Unicast { addr, port });
            HandleResult::NewPeer(msg.node_id)
        } else {
            HandleResult::Ok
        }
    }

    fn handle_heartbeat(
        &mut self,
        msg: &Message,
        addr: [u8; 4],
        port: u16,
        now: u64,
    ) -> HandleResult {
        if let Some(peer) = self.peers.get_mut(msg.node_id) {
            peer.update(msg.seq_num, now);
            HandleResult::Ok
        } else {
            // Unknown peer sent heartbeat, request announce
            let mut peer = Peer::new(msg.node_id, addr, port, NodeCapabilities::empty());
            peer.last_seen = now;
            self.peers.upsert(peer);
            HandleResult::NewPeer(msg.node_id)
        }
    }

    fn handle_data(&mut self, msg: &Message, _now: u64) -> HandleResult {
        if msg.payload.is_empty() {
            return HandleResult::Error(GossipError::InvalidPayload);
        }

        let crdt_type = CrdtType::from_u8(msg.payload[0]);
        let _crdt_data = &msg.payload[1..];

        match crdt_type {
            Some(CrdtType::LwwRegister) => HandleResult::CrdtUpdate {
                crdt_type: CrdtType::LwwRegister,
                from_node: msg.node_id,
            },
            Some(CrdtType::GCounter) => HandleResult::CrdtUpdate {
                crdt_type: CrdtType::GCounter,
                from_node: msg.node_id,
            },
            Some(CrdtType::PnCounter) => HandleResult::CrdtUpdate {
                crdt_type: CrdtType::PnCounter,
                from_node: msg.node_id,
            },
            _ => HandleResult::Error(GossipError::UnknownCrdtType),
        }
    }

    fn handle_query(&mut self, _msg: &Message, _addr: [u8; 4], _port: u16) -> HandleResult {
        // TODO: Implement query handling
        HandleResult::Ok
    }

    fn handle_leave(&mut self, msg: &Message) -> HandleResult {
        self.peers.remove(msg.node_id);
        HandleResult::PeerLeft(msg.node_id)
    }

    /// Periodic tick - call this regularly (e.g., every 100ms)
    pub fn tick(&mut self, now: u64) {
        // Send heartbeat if interval elapsed
        if now.saturating_sub(self.last_heartbeat) >= HEARTBEAT_INTERVAL_MS {
            self.queue_heartbeat();
            self.last_heartbeat = now;
        }

        // Remove stale peers
        self.peers.remove_stale(now, PEER_TIMEOUT_MS);
    }

    /// Queue an announce message
    pub fn queue_announce(&mut self, target: MessageTarget) {
        let seq = self.next_seq();
        let msg = Message::announce(self.node_id, seq, self.capabilities);
        self.queue_message(msg, target);
    }

    /// Queue a heartbeat message
    pub fn queue_heartbeat(&mut self) {
        let seq = self.next_seq();
        let msg = Message::heartbeat(self.node_id, seq);
        self.queue_message(msg, MessageTarget::Multicast);
    }

    /// Queue a data message with CRDT update
    pub fn queue_crdt_update(&mut self, crdt_type: CrdtType, data: &[u8]) {
        let seq = self.next_seq();
        if let Some(msg) = Message::data(self.node_id, seq, crdt_type as u8, data) {
            self.queue_message(msg, MessageTarget::AllPeers);
        }
    }

    fn queue_message(&mut self, msg: Message, target: MessageTarget) {
        let mut buf = Vec::new();
        buf.resize_default(MAX_PACKET_SIZE).ok();
        if let Ok(len) = msg.encode(&mut buf) {
            buf.truncate(len);
            self.outbound
                .push(OutboundMessage { target, data: buf })
                .ok();
        }
    }

    /// Take pending outbound messages
    pub fn take_outbound(&mut self) -> Vec<OutboundMessage, 8> {
        core::mem::take(&mut self.outbound)
    }

    /// Get peer addresses for AllPeers target
    pub fn peer_addresses(&self) -> impl Iterator<Item = ([u8; 4], u16)> + '_ {
        self.peers.iter().map(|p| (p.addr, p.port))
    }
}

/// Result of handling a message
#[derive(Debug)]
pub enum HandleResult {
    Ok,
    NewPeer(u32),
    PeerLeft(u32),
    CrdtUpdate { crdt_type: CrdtType, from_node: u32 },
    Error(GossipError),
}

/// Gossip protocol errors
#[derive(Debug, Clone, Copy)]
pub enum GossipError {
    InvalidPayload,
    UnknownCrdtType,
    DecodeFailed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gossip_state_new() {
        let state = GossipState::new(42, NodeCapabilities::lite());
        assert_eq!(state.node_id, 42);
        assert!(state.peers.is_empty());
    }

    #[test]
    fn test_handle_announce() {
        let mut state = GossipState::new(1, NodeCapabilities::lite());

        let msg = Message::announce(2, 1, NodeCapabilities::lite());
        let result = state.handle_message(&msg, [192, 168, 1, 100], 4872, 1000);

        match result {
            HandleResult::NewPeer(id) => assert_eq!(id, 2),
            _ => panic!("Expected NewPeer"),
        }

        assert!(state.peers.contains(2));
    }

    #[test]
    fn test_heartbeat_updates_peer() {
        let mut state = GossipState::new(1, NodeCapabilities::lite());

        // First, add peer via announce
        let announce = Message::announce(2, 1, NodeCapabilities::lite());
        state.handle_message(&announce, [192, 168, 1, 100], 4872, 1000);

        // Then heartbeat
        let heartbeat = Message::heartbeat(2, 10);
        state.handle_message(&heartbeat, [192, 168, 1, 100], 4872, 5000);

        let peer = state.peers.get(2).unwrap();
        assert_eq!(peer.last_seq, 10);
        assert_eq!(peer.last_seen, 5000);
    }

    #[test]
    fn test_tick_removes_stale() {
        let mut state = GossipState::new(1, NodeCapabilities::lite());

        let msg = Message::announce(2, 1, NodeCapabilities::lite());
        state.handle_message(&msg, [192, 168, 1, 100], 4872, 0);

        // Tick way into the future
        state.tick(PEER_TIMEOUT_MS + 1000);

        assert!(!state.peers.contains(2));
    }
}
