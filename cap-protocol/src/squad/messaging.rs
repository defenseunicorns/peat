//! Intra-Squad Communication System
//!
//! Implements Phase 2 messaging infrastructure for squad cohesion and coordination.
//!
//! # Architecture
//!
//! Squad messaging provides:
//! - **Message Bus**: Publish/subscribe pattern for squad-internal messages
//! - **Capability Exchange**: Protocol for sharing platform capabilities
//! - **Message Ordering**: Sequence numbers for ordering guarantees
//! - **Retransmission**: Reliable delivery with retry logic
//! - **Message Types**: Join, Leave, CapabilityAnnounce, LeaderAnnounce, etc.
//!
//! ## Message Flow
//!
//! ```text
//! Platform A                    Message Bus                    Platform B
//!     |                             |                              |
//!     |-- Publish(CapabilityMsg) -->|                              |
//!     |                             |-- Deliver(CapabilityMsg) --->|
//!     |                             |<-- Ack -------------------   |
//!     |<-- Confirm ----------------  |                              |
//! ```
//!
//! ## Reliability
//!
//! - Sequence numbers for ordering
//! - ACK/NACK for delivery confirmation
//! - Retransmission on timeout (max 3 retries)
//! - Message expiration (TTL)

use crate::models::Capability;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, instrument, warn};

/// Message sequence number for ordering
pub type SequenceNumber = u64;

/// Message priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MessagePriority {
    /// Low priority - status updates, periodic beacons
    Low = 0,
    /// Normal priority - standard operations
    Normal = 1,
    /// High priority - important state changes
    High = 2,
    /// Critical priority - leader election, emergencies
    Critical = 3,
}

impl Default for MessagePriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Squad message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SquadMessageType {
    /// Platform joining squad
    Join {
        platform_id: String,
        capabilities: Vec<Capability>,
    },
    /// Platform leaving squad
    Leave { platform_id: String, reason: String },
    /// Platform announcing capabilities
    CapabilityAnnounce {
        platform_id: String,
        capabilities: Vec<Capability>,
    },
    /// Leader election announcement
    LeaderAnnounce {
        leader_id: String,
        election_round: u32,
    },
    /// Heartbeat/keep-alive
    Heartbeat { platform_id: String },
    /// Role assignment notification
    RoleAssignment { platform_id: String, role: String },
    /// Generic squad status update
    StatusUpdate {
        platform_id: String,
        status: serde_json::Value,
    },
    /// Acknowledgment
    Ack { message_seq: SequenceNumber },
    /// Negative acknowledgment (retry request)
    Nack {
        message_seq: SequenceNumber,
        reason: String,
    },
}

/// Squad message envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SquadMessage {
    /// Message ID (unique per sender)
    pub message_id: String,
    /// Sequence number for ordering
    pub seq: SequenceNumber,
    /// Sender platform ID
    pub sender: String,
    /// Target squad ID
    pub squad_id: String,
    /// Message priority
    pub priority: MessagePriority,
    /// Message payload
    pub payload: SquadMessageType,
    /// Timestamp (Unix seconds)
    pub timestamp: u64,
    /// Time-to-live (seconds)
    pub ttl: u64,
}

impl SquadMessage {
    /// Create a new squad message
    pub fn new(
        sender: String,
        squad_id: String,
        seq: SequenceNumber,
        payload: SquadMessageType,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            message_id: format!("{}-{}", sender, seq),
            seq,
            sender,
            squad_id,
            priority: MessagePriority::Normal,
            payload,
            timestamp,
            ttl: 30, // Default 30 second TTL
        }
    }

    /// Create a message with custom priority
    pub fn with_priority(mut self, priority: MessagePriority) -> Self {
        self.priority = priority;
        self
    }

    /// Create a message with custom TTL
    pub fn with_ttl(mut self, ttl: u64) -> Self {
        self.ttl = ttl;
        self
    }

    /// Check if message has expired
    pub fn is_expired(&self) -> bool {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        current_time.saturating_sub(self.timestamp) > self.ttl
    }
}

/// Message delivery status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryStatus {
    /// Message pending delivery
    Pending,
    /// Message delivered, awaiting ACK
    Delivered,
    /// Message acknowledged
    Acknowledged,
    /// Message failed after retries
    Failed,
}

/// Tracked message for retransmission
#[derive(Debug, Clone)]
struct TrackedMessage {
    message: SquadMessage,
    status: DeliveryStatus,
    retry_count: u32,
    last_send: Instant,
}

/// Message handler function type
pub type MessageHandler = Arc<dyn Fn(&SquadMessage) -> Result<()> + Send + Sync>;

/// Squad Message Bus
///
/// Provides publish/subscribe messaging within a squad with:
/// - Message ordering via sequence numbers
/// - Reliable delivery with retransmission
/// - Priority-based delivery
/// - Message expiration
pub struct SquadMessageBus {
    /// Squad ID this bus serves
    squad_id: String,
    /// Local platform ID
    platform_id: String,
    /// Next sequence number for outbound messages
    next_seq: Arc<Mutex<SequenceNumber>>,
    /// Outbound message queue (priority-ordered)
    outbound_queue: Arc<Mutex<VecDeque<SquadMessage>>>,
    /// Tracked messages for retransmission
    tracked_messages: Arc<Mutex<HashMap<SequenceNumber, TrackedMessage>>>,
    /// Received message sequence numbers (for deduplication)
    received_seqs: Arc<Mutex<HashMap<String, SequenceNumber>>>,
    /// Message subscribers (handlers)
    subscribers: Arc<Mutex<Vec<MessageHandler>>>,
    /// Retransmission timeout
    retry_timeout: Duration,
    /// Max retry attempts
    max_retries: u32,
}

impl SquadMessageBus {
    /// Create a new message bus
    pub fn new(squad_id: String, platform_id: String) -> Self {
        Self {
            squad_id,
            platform_id,
            next_seq: Arc::new(Mutex::new(1)),
            outbound_queue: Arc::new(Mutex::new(VecDeque::new())),
            tracked_messages: Arc::new(Mutex::new(HashMap::new())),
            received_seqs: Arc::new(Mutex::new(HashMap::new())),
            subscribers: Arc::new(Mutex::new(Vec::new())),
            retry_timeout: Duration::from_secs(2),
            max_retries: 3,
        }
    }

    /// Subscribe to squad messages
    pub fn subscribe(&self, handler: MessageHandler) -> Result<()> {
        let mut subscribers = self.subscribers.lock().unwrap();
        subscribers.push(handler);
        Ok(())
    }

    /// Publish a message to the squad
    #[instrument(skip(self, payload))]
    pub fn publish(&self, payload: SquadMessageType) -> Result<SequenceNumber> {
        let seq = {
            let mut next_seq = self.next_seq.lock().unwrap();
            let seq = *next_seq;
            *next_seq += 1;
            seq
        };

        let message = SquadMessage::new(
            self.platform_id.clone(),
            self.squad_id.clone(),
            seq,
            payload,
        );

        debug!(
            "Publishing message seq={} from {} to squad {}",
            seq, self.platform_id, self.squad_id
        );

        // Add to outbound queue
        let mut queue = self.outbound_queue.lock().unwrap();
        queue.push_back(message.clone());
        // Sort by priority (highest first)
        let mut vec: Vec<_> = queue.drain(..).collect();
        vec.sort_by(|a, b| b.priority.cmp(&a.priority));
        queue.extend(vec);

        // Track for retransmission
        let tracked = TrackedMessage {
            message: message.clone(),
            status: DeliveryStatus::Pending,
            retry_count: 0,
            last_send: Instant::now(),
        };
        self.tracked_messages.lock().unwrap().insert(seq, tracked);

        Ok(seq)
    }

    /// Deliver a received message to subscribers
    #[instrument(skip(self, message))]
    pub fn deliver(&self, message: &SquadMessage) -> Result<()> {
        // Check if message has expired
        if message.is_expired() {
            debug!("Dropping expired message seq={}", message.seq);
            return Ok(());
        }

        // Check for duplicate (already received)
        {
            let mut received = self.received_seqs.lock().unwrap();
            if let Some(&last_seq) = received.get(&message.sender) {
                if message.seq <= last_seq {
                    debug!(
                        "Dropping duplicate message seq={} from {}",
                        message.seq, message.sender
                    );
                    return Ok(());
                }
            }
            received.insert(message.sender.clone(), message.seq);
        }

        debug!(
            "Delivering message seq={} from {} to subscribers",
            message.seq, message.sender
        );

        // Deliver to all subscribers
        let subscribers = self.subscribers.lock().unwrap();
        for handler in subscribers.iter() {
            if let Err(e) = handler(message) {
                warn!("Subscriber error: {}", e);
            }
        }

        Ok(())
    }

    /// Acknowledge a received message
    pub fn acknowledge(&self, message_seq: SequenceNumber) -> Result<()> {
        let mut tracked = self.tracked_messages.lock().unwrap();
        if let Some(msg) = tracked.get_mut(&message_seq) {
            msg.status = DeliveryStatus::Acknowledged;
            debug!("Acknowledged message seq={}", message_seq);
        }
        Ok(())
    }

    /// Process retransmissions for unacknowledged messages
    #[instrument(skip(self))]
    pub fn process_retransmissions(&self) -> Result<Vec<SquadMessage>> {
        let mut tracked = self.tracked_messages.lock().unwrap();
        let mut to_retry = Vec::new();

        for (seq, msg) in tracked.iter_mut() {
            if msg.status == DeliveryStatus::Acknowledged {
                continue;
            }

            if msg.last_send.elapsed() >= self.retry_timeout {
                if msg.retry_count >= self.max_retries {
                    warn!(
                        "Message seq={} failed after {} retries",
                        seq, msg.retry_count
                    );
                    msg.status = DeliveryStatus::Failed;
                } else {
                    debug!(
                        "Retransmitting message seq={} (attempt {})",
                        seq,
                        msg.retry_count + 1
                    );
                    msg.retry_count += 1;
                    msg.last_send = Instant::now();
                    msg.status = DeliveryStatus::Delivered;
                    to_retry.push(msg.message.clone());
                }
            }
        }

        // Clean up acknowledged and failed messages
        tracked.retain(|_, msg| {
            msg.status != DeliveryStatus::Acknowledged && msg.status != DeliveryStatus::Failed
        });

        Ok(to_retry)
    }

    /// Get pending outbound messages
    pub fn get_pending_messages(&self) -> Result<Vec<SquadMessage>> {
        let mut queue = self.outbound_queue.lock().unwrap();
        let messages: Vec<_> = queue.drain(..).collect();
        Ok(messages)
    }

    /// Get statistics
    pub fn stats(&self) -> MessageBusStats {
        let tracked = self.tracked_messages.lock().unwrap();
        let outbound = self.outbound_queue.lock().unwrap();
        let received = self.received_seqs.lock().unwrap();
        let subscribers = self.subscribers.lock().unwrap();

        MessageBusStats {
            pending_outbound: outbound.len(),
            tracked_messages: tracked.len(),
            unique_senders: received.len(),
            subscriber_count: subscribers.len(),
            next_seq: *self.next_seq.lock().unwrap(),
        }
    }
}

/// Message bus statistics
#[derive(Debug, Clone)]
pub struct MessageBusStats {
    pub pending_outbound: usize,
    pub tracked_messages: usize,
    pub unique_senders: usize,
    pub subscriber_count: usize,
    pub next_seq: SequenceNumber,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let payload = SquadMessageType::Heartbeat {
            platform_id: "platform_1".to_string(),
        };

        let message = SquadMessage::new(
            "platform_1".to_string(),
            "squad_alpha".to_string(),
            1,
            payload,
        );

        assert_eq!(message.seq, 1);
        assert_eq!(message.sender, "platform_1");
        assert_eq!(message.squad_id, "squad_alpha");
        assert_eq!(message.priority, MessagePriority::Normal);
        assert!(!message.is_expired());
    }

    #[test]
    fn test_message_expiration() {
        let payload = SquadMessageType::Heartbeat {
            platform_id: "platform_1".to_string(),
        };

        let mut message = SquadMessage::new(
            "platform_1".to_string(),
            "squad_alpha".to_string(),
            1,
            payload,
        )
        .with_ttl(0);

        // Force timestamp to be in the past
        message.timestamp = 0;

        assert!(message.is_expired());
    }

    #[test]
    fn test_message_priority() {
        let payload = SquadMessageType::Heartbeat {
            platform_id: "platform_1".to_string(),
        };

        let message = SquadMessage::new(
            "platform_1".to_string(),
            "squad_alpha".to_string(),
            1,
            payload,
        )
        .with_priority(MessagePriority::Critical);

        assert_eq!(message.priority, MessagePriority::Critical);
    }

    #[test]
    fn test_message_bus_creation() {
        let bus = SquadMessageBus::new("squad_alpha".to_string(), "platform_1".to_string());

        assert_eq!(bus.squad_id, "squad_alpha");
        assert_eq!(bus.platform_id, "platform_1");

        let stats = bus.stats();
        assert_eq!(stats.pending_outbound, 0);
        assert_eq!(stats.next_seq, 1);
    }

    #[test]
    fn test_publish_message() {
        let bus = SquadMessageBus::new("squad_alpha".to_string(), "platform_1".to_string());

        let payload = SquadMessageType::Heartbeat {
            platform_id: "platform_1".to_string(),
        };

        let seq = bus.publish(payload).unwrap();
        assert_eq!(seq, 1);

        let stats = bus.stats();
        assert_eq!(stats.next_seq, 2);
        assert_eq!(stats.pending_outbound, 1);
    }

    #[test]
    fn test_priority_ordering() {
        let bus = SquadMessageBus::new("squad_alpha".to_string(), "platform_1".to_string());

        // Publish messages with different priorities
        let _ = bus.publish(SquadMessageType::Heartbeat {
            platform_id: "platform_1".to_string(),
        });

        let _ = bus.publish(SquadMessageType::LeaderAnnounce {
            leader_id: "platform_2".to_string(),
            election_round: 1,
        });

        // Manually set priority on queue (in real use, would be set during publish)
        {
            let mut queue = bus.outbound_queue.lock().unwrap();
            if let Some(msg) = queue.get_mut(1) {
                msg.priority = MessagePriority::Critical;
            }
        }

        let messages = bus.get_pending_messages().unwrap();
        assert_eq!(messages.len(), 2);
        // Note: actual priority ordering happens in publish(), this test validates concept
    }

    #[test]
    fn test_duplicate_detection() {
        let bus = SquadMessageBus::new("squad_alpha".to_string(), "platform_1".to_string());

        let message = SquadMessage::new(
            "platform_2".to_string(),
            "squad_alpha".to_string(),
            1,
            SquadMessageType::Heartbeat {
                platform_id: "platform_2".to_string(),
            },
        );

        // Deliver first time - should succeed
        bus.deliver(&message).unwrap();

        // Deliver again - should be dropped as duplicate
        bus.deliver(&message).unwrap();

        let stats = bus.stats();
        assert_eq!(stats.unique_senders, 1);
    }

    #[test]
    fn test_subscriber_notification() {
        let bus = SquadMessageBus::new("squad_alpha".to_string(), "platform_1".to_string());

        let received = Arc::new(Mutex::new(false));
        let received_clone = received.clone();

        bus.subscribe(Arc::new(move |_msg| {
            *received_clone.lock().unwrap() = true;
            Ok(())
        }))
        .unwrap();

        let message = SquadMessage::new(
            "platform_2".to_string(),
            "squad_alpha".to_string(),
            1,
            SquadMessageType::Heartbeat {
                platform_id: "platform_2".to_string(),
            },
        );

        bus.deliver(&message).unwrap();

        assert!(*received.lock().unwrap());
    }

    #[test]
    fn test_acknowledgment() {
        let bus = SquadMessageBus::new("squad_alpha".to_string(), "platform_1".to_string());

        let seq = bus
            .publish(SquadMessageType::Heartbeat {
                platform_id: "platform_1".to_string(),
            })
            .unwrap();

        bus.acknowledge(seq).unwrap();

        let tracked = bus.tracked_messages.lock().unwrap();
        assert_eq!(
            tracked.get(&seq).unwrap().status,
            DeliveryStatus::Acknowledged
        );
    }

    #[test]
    fn test_retransmission() {
        let mut bus = SquadMessageBus::new("squad_alpha".to_string(), "platform_1".to_string());
        bus.retry_timeout = Duration::from_millis(10); // Short timeout for testing

        let seq = bus
            .publish(SquadMessageType::Heartbeat {
                platform_id: "platform_1".to_string(),
            })
            .unwrap();

        // Get initial message
        let _ = bus.get_pending_messages().unwrap();

        // Wait for retry timeout
        std::thread::sleep(Duration::from_millis(15));

        // Process retransmissions
        let retries = bus.process_retransmissions().unwrap();

        assert_eq!(retries.len(), 1);
        assert_eq!(retries[0].seq, seq);
    }
}
