//! HIVE GATT Sync Protocol
//!
//! Defines the protocol for CRDT sync operations over BLE GATT.
//!
//! ## Protocol Overview
//!
//! The sync protocol uses a request-response pattern:
//!
//! 1. **Initiator** (central) writes to Sync Data characteristic
//! 2. **Responder** (peripheral) sends indication with response
//! 3. **Initiator** acknowledges indication
//!
//! ## Message Flow
//!
//! ```text
//! Central (Initiator)              Peripheral (Responder)
//!        |                                   |
//!        |  [Write] Sync Request (Vector)    |
//!        |---------------------------------->|
//!        |                                   |
//!        |  [Indicate] Sync Response (Docs)  |
//!        |<----------------------------------|
//!        |                                   |
//!        |  [Write] ACK                      |
//!        |---------------------------------->|
//!        |                                   |
//!        |  [Write] End Sync                 |
//!        |---------------------------------->|
//! ```
//!
//! ## Fragmentation
//!
//! Large documents are fragmented across multiple GATT writes.
//! The `SyncDataHeader` contains fragment count and index.

#[cfg(not(feature = "std"))]
use alloc::{collections::VecDeque, vec, vec::Vec};
#[cfg(feature = "std")]
use std::collections::VecDeque;

use super::characteristics::{SyncDataHeader, SyncDataOp};

/// Maximum payload size for a single GATT write (MTU - 3 - header)
pub const fn max_payload_size(mtu: u16) -> usize {
    (mtu as usize).saturating_sub(3 + SyncDataHeader::SIZE)
}

/// Default maximum payload assuming 23-byte MTU
pub const DEFAULT_MAX_PAYLOAD: usize = 15; // 23 - 3 - 5

/// Sync message types for the protocol state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMessageType {
    /// Sync vector (list of document versions we have)
    SyncVector,
    /// Document data
    Document,
    /// Acknowledgement
    Ack,
    /// End of sync session
    EndSync,
    /// Error occurred
    Error,
}

/// A sync message ready to be sent
#[derive(Debug, Clone)]
pub struct SyncMessage {
    /// Message type
    pub msg_type: SyncMessageType,
    /// Sequence number
    pub seq: u16,
    /// Total fragments for this message
    pub total_fragments: u8,
    /// Current fragment index
    pub fragment_index: u8,
    /// Message payload
    pub payload: Vec<u8>,
}

impl SyncMessage {
    /// Create a new sync message
    pub fn new(msg_type: SyncMessageType, seq: u16, payload: Vec<u8>) -> Self {
        Self {
            msg_type,
            seq,
            total_fragments: 1,
            fragment_index: 0,
            payload,
        }
    }

    /// Encode to bytes for GATT write
    pub fn encode(&self) -> Vec<u8> {
        let op = match self.msg_type {
            SyncMessageType::SyncVector => SyncDataOp::Vector,
            SyncMessageType::Document => SyncDataOp::Document,
            SyncMessageType::Ack => SyncDataOp::Ack,
            SyncMessageType::EndSync | SyncMessageType::Error => SyncDataOp::End,
        };

        let header = SyncDataHeader {
            op,
            seq: self.seq,
            total_fragments: self.total_fragments,
            fragment_index: self.fragment_index,
        };

        let mut buf = Vec::with_capacity(SyncDataHeader::SIZE + self.payload.len());
        buf.extend_from_slice(&header.encode());
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Decode from GATT write data
    pub fn decode(data: &[u8]) -> Option<Self> {
        let header = SyncDataHeader::decode(data)?;
        let payload = if data.len() > SyncDataHeader::SIZE {
            data[SyncDataHeader::SIZE..].to_vec()
        } else {
            Vec::new()
        };

        let msg_type = match header.op {
            SyncDataOp::Vector => SyncMessageType::SyncVector,
            SyncDataOp::Document => SyncMessageType::Document,
            SyncDataOp::Ack => SyncMessageType::Ack,
            SyncDataOp::End => SyncMessageType::EndSync,
        };

        Some(Self {
            msg_type,
            seq: header.seq,
            total_fragments: header.total_fragments,
            fragment_index: header.fragment_index,
            payload,
        })
    }
}

/// Fragment a large payload into multiple sync messages
pub fn fragment_payload(
    msg_type: SyncMessageType,
    seq: u16,
    payload: &[u8],
    max_fragment_size: usize,
) -> Vec<SyncMessage> {
    if payload.is_empty() || payload.len() <= max_fragment_size {
        return vec![SyncMessage::new(msg_type, seq, payload.to_vec())];
    }

    let total_fragments = (payload.len() + max_fragment_size - 1) / max_fragment_size;
    let total_fragments = total_fragments.min(255) as u8;

    payload
        .chunks(max_fragment_size)
        .enumerate()
        .map(|(i, chunk)| SyncMessage {
            msg_type,
            seq,
            total_fragments,
            fragment_index: i as u8,
            payload: chunk.to_vec(),
        })
        .collect()
}

/// Reassemble fragmented messages
#[derive(Debug)]
pub struct FragmentReassembler {
    /// Expected total fragments
    total_fragments: u8,
    /// Received fragments (indexed by fragment_index)
    fragments: Vec<Option<Vec<u8>>>,
    /// Sequence number being reassembled
    seq: u16,
    /// Message type
    msg_type: SyncMessageType,
}

impl FragmentReassembler {
    /// Create a new reassembler for a message
    pub fn new(msg: &SyncMessage) -> Self {
        let mut fragments = vec![None; msg.total_fragments as usize];
        fragments[msg.fragment_index as usize] = Some(msg.payload.clone());

        Self {
            total_fragments: msg.total_fragments,
            fragments,
            seq: msg.seq,
            msg_type: msg.msg_type,
        }
    }

    /// Add a fragment to the reassembler
    ///
    /// Returns true if all fragments have been received.
    pub fn add_fragment(&mut self, msg: &SyncMessage) -> bool {
        if msg.seq != self.seq || msg.total_fragments != self.total_fragments {
            return false;
        }

        if (msg.fragment_index as usize) < self.fragments.len() {
            self.fragments[msg.fragment_index as usize] = Some(msg.payload.clone());
        }

        self.is_complete()
    }

    /// Check if all fragments have been received
    pub fn is_complete(&self) -> bool {
        self.fragments.iter().all(|f| f.is_some())
    }

    /// Get the reassembled payload
    ///
    /// Returns None if not all fragments have been received.
    pub fn reassemble(&self) -> Option<Vec<u8>> {
        if !self.is_complete() {
            return None;
        }

        let total_size: usize = self.fragments.iter().flatten().map(|f| f.len()).sum();
        let mut result = Vec::with_capacity(total_size);

        for data in self.fragments.iter().flatten() {
            result.extend_from_slice(data);
        }

        Some(result)
    }

    /// Get the sequence number
    pub fn seq(&self) -> u16 {
        self.seq
    }

    /// Get the message type
    pub fn msg_type(&self) -> SyncMessageType {
        self.msg_type
    }
}

/// Sync protocol state machine state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncProtocolState {
    /// Idle, not syncing
    Idle,
    /// Waiting to send sync vector
    SendingVector,
    /// Waiting for documents
    ReceivingDocuments,
    /// Sending documents
    SendingDocuments,
    /// Waiting for acknowledgements
    WaitingAck,
    /// Sync complete
    Complete,
    /// Error state
    Error,
}

/// Sync protocol handler
///
/// Manages the state machine for sync operations over BLE GATT.
pub struct SyncProtocol {
    /// Current state
    state: SyncProtocolState,
    /// Current sequence number
    seq: u16,
    /// Outgoing message queue
    outgoing: VecDeque<SyncMessage>,
    /// Pending acknowledgement sequence numbers
    pending_acks: Vec<u16>,
    /// Current fragment reassembler
    reassembler: Option<FragmentReassembler>,
    /// Maximum payload size (based on MTU)
    max_payload: usize,
}

impl SyncProtocol {
    /// Create a new sync protocol handler
    pub fn new() -> Self {
        Self {
            state: SyncProtocolState::Idle,
            seq: 0,
            outgoing: VecDeque::new(),
            pending_acks: Vec::new(),
            reassembler: None,
            max_payload: DEFAULT_MAX_PAYLOAD,
        }
    }

    /// Set MTU for fragmentation
    pub fn set_mtu(&mut self, mtu: u16) {
        self.max_payload = max_payload_size(mtu);
    }

    /// Get current state
    pub fn state(&self) -> SyncProtocolState {
        self.state
    }

    /// Start a sync session
    pub fn start_sync(&mut self, sync_vector: Vec<u8>) {
        self.state = SyncProtocolState::SendingVector;
        self.seq = 0;

        // Queue sync vector message(s)
        let messages = fragment_payload(
            SyncMessageType::SyncVector,
            self.next_seq(),
            &sync_vector,
            self.max_payload,
        );

        for msg in messages {
            self.outgoing.push_back(msg);
        }
    }

    /// Queue a document to send
    pub fn queue_document(&mut self, doc_data: Vec<u8>) {
        if self.state == SyncProtocolState::Idle {
            self.state = SyncProtocolState::SendingDocuments;
        }

        let messages = fragment_payload(
            SyncMessageType::Document,
            self.next_seq(),
            &doc_data,
            self.max_payload,
        );

        for msg in messages {
            self.outgoing.push_back(msg);
        }
    }

    /// End the sync session
    pub fn end_sync(&mut self) {
        let msg = SyncMessage::new(SyncMessageType::EndSync, self.next_seq(), Vec::new());
        self.outgoing.push_back(msg);
        self.state = SyncProtocolState::Complete;
    }

    /// Get next message to send
    pub fn next_outgoing(&mut self) -> Option<SyncMessage> {
        self.outgoing.pop_front()
    }

    /// Check if there are messages to send
    pub fn has_outgoing(&self) -> bool {
        !self.outgoing.is_empty()
    }

    /// Process an incoming message
    ///
    /// Returns the reassembled payload if a complete message was received.
    pub fn process_incoming(&mut self, data: &[u8]) -> Option<(SyncMessageType, Vec<u8>)> {
        let msg = SyncMessage::decode(data)?;

        // Handle fragmented messages
        if msg.total_fragments > 1 {
            if let Some(ref mut reassembler) = self.reassembler {
                if reassembler.seq() == msg.seq {
                    if reassembler.add_fragment(&msg) {
                        let payload = reassembler.reassemble()?;
                        let msg_type = reassembler.msg_type();
                        self.reassembler = None;
                        return Some((msg_type, payload));
                    }
                    return None;
                }
            }
            // Start new reassembly
            self.reassembler = Some(FragmentReassembler::new(&msg));
            if self.reassembler.as_ref().unwrap().is_complete() {
                let reassembler = self.reassembler.take().unwrap();
                let payload = reassembler.reassemble()?;
                return Some((reassembler.msg_type(), payload));
            }
            return None;
        }

        // Non-fragmented message
        match msg.msg_type {
            SyncMessageType::Ack => {
                self.pending_acks.retain(|&seq| seq != msg.seq);
                None
            }
            SyncMessageType::SyncVector => {
                self.state = SyncProtocolState::ReceivingDocuments;
                Some((SyncMessageType::SyncVector, msg.payload))
            }
            SyncMessageType::Document => {
                // Queue ACK
                let ack = SyncMessage::new(SyncMessageType::Ack, msg.seq, Vec::new());
                self.outgoing.push_back(ack);
                Some((SyncMessageType::Document, msg.payload))
            }
            SyncMessageType::EndSync => {
                self.state = SyncProtocolState::Complete;
                Some((SyncMessageType::EndSync, Vec::new()))
            }
            SyncMessageType::Error => {
                self.state = SyncProtocolState::Error;
                Some((SyncMessageType::Error, msg.payload))
            }
        }
    }

    /// Reset the protocol state
    pub fn reset(&mut self) {
        self.state = SyncProtocolState::Idle;
        self.seq = 0;
        self.outgoing.clear();
        self.pending_acks.clear();
        self.reassembler = None;
    }

    /// Get next sequence number
    fn next_seq(&mut self) -> u16 {
        let seq = self.seq;
        self.seq = self.seq.wrapping_add(1);
        seq
    }
}

impl Default for SyncProtocol {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_payload_size() {
        assert_eq!(max_payload_size(23), 15); // Default MTU
        assert_eq!(max_payload_size(251), 243); // Target MTU
        assert_eq!(max_payload_size(8), 0); // Too small
    }

    #[test]
    fn test_sync_message_encode_decode() {
        let msg = SyncMessage::new(SyncMessageType::Document, 42, vec![1, 2, 3, 4, 5]);

        let encoded = msg.encode();
        let decoded = SyncMessage::decode(&encoded).unwrap();

        assert_eq!(decoded.msg_type, SyncMessageType::Document);
        assert_eq!(decoded.seq, 42);
        assert_eq!(decoded.payload, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_fragment_payload() {
        let payload = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let fragments = fragment_payload(SyncMessageType::Document, 1, &payload, 4);

        assert_eq!(fragments.len(), 3);
        assert_eq!(fragments[0].total_fragments, 3);
        assert_eq!(fragments[0].fragment_index, 0);
        assert_eq!(fragments[0].payload, vec![1, 2, 3, 4]);
        assert_eq!(fragments[1].fragment_index, 1);
        assert_eq!(fragments[1].payload, vec![5, 6, 7, 8]);
        assert_eq!(fragments[2].fragment_index, 2);
        assert_eq!(fragments[2].payload, vec![9, 10]);
    }

    #[test]
    fn test_fragment_reassembler() {
        let payload = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let fragments = fragment_payload(SyncMessageType::Document, 1, &payload, 4);

        let mut reassembler = FragmentReassembler::new(&fragments[0]);
        assert!(!reassembler.is_complete());

        reassembler.add_fragment(&fragments[1]);
        assert!(!reassembler.is_complete());

        reassembler.add_fragment(&fragments[2]);
        assert!(reassembler.is_complete());

        let result = reassembler.reassemble().unwrap();
        assert_eq!(result, payload);
    }

    #[test]
    fn test_sync_protocol_basic_flow() {
        let mut initiator = SyncProtocol::new();
        let mut responder = SyncProtocol::new();

        // Initiator starts sync
        initiator.start_sync(vec![1, 2, 3]);
        assert_eq!(initiator.state(), SyncProtocolState::SendingVector);

        // Get message from initiator
        let msg = initiator.next_outgoing().unwrap();
        let encoded = msg.encode();

        // Responder processes message
        let (msg_type, payload) = responder.process_incoming(&encoded).unwrap();
        assert_eq!(msg_type, SyncMessageType::SyncVector);
        assert_eq!(payload, vec![1, 2, 3]);

        // Responder sends document
        responder.queue_document(vec![4, 5, 6]);
        let msg = responder.next_outgoing().unwrap();
        let encoded = msg.encode();

        // Initiator processes document
        let (msg_type, payload) = initiator.process_incoming(&encoded).unwrap();
        assert_eq!(msg_type, SyncMessageType::Document);
        assert_eq!(payload, vec![4, 5, 6]);

        // Initiator should have ACK queued
        assert!(initiator.has_outgoing());

        // End sync
        initiator.end_sync();
        assert_eq!(initiator.state(), SyncProtocolState::Complete);
    }

    #[test]
    fn test_sync_protocol_with_mtu() {
        let mut protocol = SyncProtocol::new();
        protocol.set_mtu(251);

        // Queue a large document
        let large_doc = vec![0u8; 500];
        protocol.queue_document(large_doc);

        // Should be fragmented
        let mut count = 0;
        while protocol.has_outgoing() {
            protocol.next_outgoing();
            count += 1;
        }
        assert!(count > 1);
    }

    #[test]
    fn test_protocol_reset() {
        let mut protocol = SyncProtocol::new();
        protocol.start_sync(vec![1, 2, 3]);
        protocol.queue_document(vec![4, 5, 6]);

        protocol.reset();

        assert_eq!(protocol.state(), SyncProtocolState::Idle);
        assert!(!protocol.has_outgoing());
    }
}
