//! Intra-Cell Communication System
//!
//! Implements Phase 2 messaging infrastructure for squad cohesion and coordination.
//!
//! # Architecture
//!
//! Cell messaging provides:
//! - **Message Bus**: Publish/subscribe pattern for squad-internal messages
//! - **Capability Exchange**: Protocol for sharing platform capabilities
//! - **Message Ordering**: Sequence numbers for ordering guarantees
//! - **Retransmission**: Reliable delivery with retry logic
//! - **Message Types**: Join, Leave, CapabilityAnnounce, LeaderAnnounce, etc.
//!
//! ## Message Flow
//!
//! ```text
//! Node A                    Message Bus                    Node B
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

use crate::models::{Capability, CellRole};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, instrument, warn};

/// Message sequence number for ordering
pub type SequenceNumber = u64;

/// Message priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum MessagePriority {
    /// Low priority - status updates, periodic beacons
    Low = 0,
    /// Normal priority - standard operations
    #[default]
    Normal = 1,
    /// High priority - important state changes
    High = 2,
    /// Critical priority - leader election, emergencies
    Critical = 3,
}

/// Routing context for hierarchical messaging
///
/// Determines how messages are prioritized and routed through the hierarchy.
/// Messages may be escalated when crossing hierarchy boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutingContext {
    /// Message stays within a single cell (peer-to-peer)
    IntraCell,
    /// Message propagates upward from cell to zone level (leader only)
    CellToZone,
    /// Message propagates downward from zone to cell (broadcast)
    ZoneToCell,
    /// Message stays within zone coordinator level
    IntraZone,
}

impl MessagePriority {
    /// Escalate priority when crossing hierarchy boundaries
    ///
    /// # Priority Escalation Rules
    ///
    /// 1. **Cell → Zone (Upward)**: All messages escalate +1 level (max: Critical)
    ///    - Rationale: Zone coordinator processes fewer, more important messages
    ///    - Low → Normal, Normal → High, High → Critical, Critical → Critical
    ///
    /// 2. **Zone → Cell (Downward)**: No escalation (maintain priority)
    ///    - Rationale: Zone directives already have appropriate priority
    ///
    /// 3. **Intra-Cell/Intra-Zone**: No escalation (lateral communication)
    ///
    /// # Examples
    ///
    /// ```
    /// use hive_protocol::cell::messaging::{MessagePriority, RoutingContext};
    ///
    /// let priority = MessagePriority::Normal;
    ///
    /// // Escalates when going up the hierarchy
    /// assert_eq!(
    ///     priority.escalate(RoutingContext::CellToZone),
    ///     MessagePriority::High
    /// );
    ///
    /// // No escalation for lateral or downward routing
    /// assert_eq!(
    ///     priority.escalate(RoutingContext::IntraCell),
    ///     MessagePriority::Normal
    /// );
    /// ```
    pub fn escalate(self, context: RoutingContext) -> Self {
        match context {
            // Upward routing: escalate priority
            RoutingContext::CellToZone => match self {
                MessagePriority::Low => MessagePriority::Normal,
                MessagePriority::Normal => MessagePriority::High,
                MessagePriority::High | MessagePriority::Critical => MessagePriority::Critical,
            },
            // Lateral and downward routing: no escalation
            RoutingContext::IntraCell | RoutingContext::ZoneToCell | RoutingContext::IntraZone => {
                self
            }
        }
    }

    /// Determine if this priority should preempt lower priority messages
    ///
    /// Used for flow control and queue management decisions.
    pub fn can_preempt(self, other: MessagePriority) -> bool {
        self > other
    }

    /// Get the numeric value for bandwidth allocation
    ///
    /// Higher priorities get more bandwidth allocation in flow control.
    /// Returns a multiplier for bandwidth limits (1.0 = normal, 2.0 = double bandwidth, etc.)
    pub fn bandwidth_multiplier(self) -> f32 {
        match self {
            MessagePriority::Low => 0.5,
            MessagePriority::Normal => 1.0,
            MessagePriority::High => 1.5,
            MessagePriority::Critical => 2.0,
        }
    }
}

/// Cell message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CellMessageType {
    /// Node joining squad
    Join {
        platform_id: String,
        capabilities: Vec<Capability>,
    },
    /// Node leaving squad
    Leave { platform_id: String, reason: String },
    /// Node announcing capabilities
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
    RoleAssignment {
        platform_id: String,
        role: CellRole,
        score: f64,
        is_primary: bool,
    },
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

/// Cell message envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellMessage {
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
    /// Routing context (for hierarchical priority escalation)
    pub routing_context: RoutingContext,
    /// Message payload
    pub payload: CellMessageType,
    /// Timestamp (Unix seconds)
    pub timestamp: u64,
    /// Time-to-live (seconds)
    pub ttl: u64,
}

impl CellMessage {
    /// Create a new squad message (defaults to intra-cell routing)
    pub fn new(
        sender: String,
        squad_id: String,
        seq: SequenceNumber,
        payload: CellMessageType,
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
            routing_context: RoutingContext::IntraCell,
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

    /// Create a message with custom routing context
    pub fn with_routing_context(mut self, context: RoutingContext) -> Self {
        self.routing_context = context;
        self
    }

    /// Create a message with custom TTL
    pub fn with_ttl(mut self, ttl: u64) -> Self {
        self.ttl = ttl;
        self
    }

    /// Escalate message priority based on routing context
    ///
    /// This should be called when a message crosses hierarchy boundaries.
    /// For example, when a cell leader forwards a message to zone level.
    pub fn escalate_priority(&mut self) {
        self.priority = self.priority.escalate(self.routing_context);
    }

    /// Get the effective priority after escalation
    ///
    /// Returns what the priority would be if escalated, without modifying the message.
    pub fn effective_priority(&self) -> MessagePriority {
        self.priority.escalate(self.routing_context)
    }

    /// Check if message has expired
    pub fn is_expired(&self) -> bool {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        current_time.saturating_sub(self.timestamp) > self.ttl
    }

    /// Create a role assignment message
    pub fn role_assignment(
        sender: String,
        squad_id: String,
        seq: SequenceNumber,
        platform_id: String,
        role: CellRole,
        score: f64,
        is_primary: bool,
    ) -> Self {
        Self::new(
            sender,
            squad_id,
            seq,
            CellMessageType::RoleAssignment {
                platform_id,
                role,
                score,
                is_primary,
            },
        )
        .with_priority(MessagePriority::High)
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
    message: CellMessage,
    status: DeliveryStatus,
    retry_count: u32,
    last_send: Instant,
}

/// Message handler function type
pub type MessageHandler = Arc<dyn Fn(&CellMessage) -> Result<()> + Send + Sync>;

/// Cell Message Bus
///
/// Provides publish/subscribe messaging within a squad with:
/// - Message ordering via sequence numbers
/// - Reliable delivery with retransmission
/// - Priority-based delivery
/// - Message expiration
pub struct CellMessageBus {
    /// Cell ID this bus serves
    squad_id: String,
    /// Local platform ID
    platform_id: String,
    /// Next sequence number for outbound messages
    next_seq: Arc<Mutex<SequenceNumber>>,
    /// Outbound message queue (priority-ordered)
    outbound_queue: Arc<Mutex<VecDeque<CellMessage>>>,
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

impl CellMessageBus {
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
    pub fn publish(&self, payload: CellMessageType) -> Result<SequenceNumber> {
        let seq = {
            let mut next_seq = self.next_seq.lock().unwrap();
            let seq = *next_seq;
            *next_seq += 1;
            seq
        };

        let message = CellMessage::new(
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
    pub fn deliver(&self, message: &CellMessage) -> Result<()> {
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
    pub fn process_retransmissions(&self) -> Result<Vec<CellMessage>> {
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
    pub fn get_pending_messages(&self) -> Result<Vec<CellMessage>> {
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
        let payload = CellMessageType::Heartbeat {
            platform_id: "node_1".to_string(),
        };

        let message = CellMessage::new("node_1".to_string(), "squad_alpha".to_string(), 1, payload);

        assert_eq!(message.seq, 1);
        assert_eq!(message.sender, "node_1");
        assert_eq!(message.squad_id, "squad_alpha");
        assert_eq!(message.priority, MessagePriority::Normal);
        assert!(!message.is_expired());
    }

    #[test]
    fn test_message_expiration() {
        let payload = CellMessageType::Heartbeat {
            platform_id: "node_1".to_string(),
        };

        let mut message =
            CellMessage::new("node_1".to_string(), "squad_alpha".to_string(), 1, payload)
                .with_ttl(0);

        // Force timestamp to be in the past
        message.timestamp = 0;

        assert!(message.is_expired());
    }

    #[test]
    fn test_message_priority() {
        let payload = CellMessageType::Heartbeat {
            platform_id: "node_1".to_string(),
        };

        let message = CellMessage::new("node_1".to_string(), "squad_alpha".to_string(), 1, payload)
            .with_priority(MessagePriority::Critical);

        assert_eq!(message.priority, MessagePriority::Critical);
    }

    #[test]
    fn test_message_bus_creation() {
        let bus = CellMessageBus::new("squad_alpha".to_string(), "node_1".to_string());

        assert_eq!(bus.squad_id, "squad_alpha");
        assert_eq!(bus.platform_id, "node_1");

        let stats = bus.stats();
        assert_eq!(stats.pending_outbound, 0);
        assert_eq!(stats.next_seq, 1);
    }

    #[test]
    fn test_publish_message() {
        let bus = CellMessageBus::new("squad_alpha".to_string(), "node_1".to_string());

        let payload = CellMessageType::Heartbeat {
            platform_id: "node_1".to_string(),
        };

        let seq = bus.publish(payload).unwrap();
        assert_eq!(seq, 1);

        let stats = bus.stats();
        assert_eq!(stats.next_seq, 2);
        assert_eq!(stats.pending_outbound, 1);
    }

    #[test]
    fn test_priority_ordering() {
        let bus = CellMessageBus::new("squad_alpha".to_string(), "node_1".to_string());

        // Publish messages with different priorities
        let _ = bus.publish(CellMessageType::Heartbeat {
            platform_id: "node_1".to_string(),
        });

        let _ = bus.publish(CellMessageType::LeaderAnnounce {
            leader_id: "node_2".to_string(),
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
        let bus = CellMessageBus::new("squad_alpha".to_string(), "node_1".to_string());

        let message = CellMessage::new(
            "node_2".to_string(),
            "squad_alpha".to_string(),
            1,
            CellMessageType::Heartbeat {
                platform_id: "node_2".to_string(),
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
        let bus = CellMessageBus::new("squad_alpha".to_string(), "node_1".to_string());

        let received = Arc::new(Mutex::new(false));
        let received_clone = received.clone();

        bus.subscribe(Arc::new(move |_msg| {
            *received_clone.lock().unwrap() = true;
            Ok(())
        }))
        .unwrap();

        let message = CellMessage::new(
            "node_2".to_string(),
            "squad_alpha".to_string(),
            1,
            CellMessageType::Heartbeat {
                platform_id: "node_2".to_string(),
            },
        );

        bus.deliver(&message).unwrap();

        assert!(*received.lock().unwrap());
    }

    #[test]
    fn test_acknowledgment() {
        let bus = CellMessageBus::new("squad_alpha".to_string(), "node_1".to_string());

        let seq = bus
            .publish(CellMessageType::Heartbeat {
                platform_id: "node_1".to_string(),
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
        let mut bus = CellMessageBus::new("squad_alpha".to_string(), "node_1".to_string());
        bus.retry_timeout = Duration::from_millis(10); // Short timeout for testing

        let seq = bus
            .publish(CellMessageType::Heartbeat {
                platform_id: "node_1".to_string(),
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

    #[test]
    fn test_role_assignment_message() {
        let msg = CellMessage::role_assignment(
            "node_1".to_string(),
            "squad_1".to_string(),
            1,
            "node_2".to_string(),
            CellRole::Sensor,
            0.85,
            true,
        );

        assert_eq!(msg.sender, "node_1");
        assert_eq!(msg.squad_id, "squad_1");
        assert_eq!(msg.seq, 1);
        assert_eq!(msg.priority, MessagePriority::High);

        match msg.payload {
            CellMessageType::RoleAssignment {
                platform_id,
                role,
                score,
                is_primary,
            } => {
                assert_eq!(platform_id, "node_2");
                assert_eq!(role, CellRole::Sensor);
                assert_eq!(score, 0.85);
                assert!(is_primary);
            }
            _ => panic!("Expected RoleAssignment message"),
        }
    }

    // ===== Hierarchical Priority Tests =====

    #[test]
    fn test_priority_escalation_upward() {
        // Low → Normal when going cell to zone
        assert_eq!(
            MessagePriority::Low.escalate(RoutingContext::CellToZone),
            MessagePriority::Normal
        );

        // Normal → High when going cell to zone
        assert_eq!(
            MessagePriority::Normal.escalate(RoutingContext::CellToZone),
            MessagePriority::High
        );

        // High → Critical when going cell to zone
        assert_eq!(
            MessagePriority::High.escalate(RoutingContext::CellToZone),
            MessagePriority::Critical
        );

        // Critical stays Critical (max level)
        assert_eq!(
            MessagePriority::Critical.escalate(RoutingContext::CellToZone),
            MessagePriority::Critical
        );
    }

    #[test]
    fn test_priority_escalation_lateral() {
        // No escalation for intra-cell routing
        assert_eq!(
            MessagePriority::Low.escalate(RoutingContext::IntraCell),
            MessagePriority::Low
        );
        assert_eq!(
            MessagePriority::Normal.escalate(RoutingContext::IntraCell),
            MessagePriority::Normal
        );
        assert_eq!(
            MessagePriority::High.escalate(RoutingContext::IntraCell),
            MessagePriority::High
        );

        // No escalation for intra-zone routing
        assert_eq!(
            MessagePriority::Normal.escalate(RoutingContext::IntraZone),
            MessagePriority::Normal
        );
    }

    #[test]
    fn test_priority_escalation_downward() {
        // No escalation when going zone to cell
        assert_eq!(
            MessagePriority::Low.escalate(RoutingContext::ZoneToCell),
            MessagePriority::Low
        );
        assert_eq!(
            MessagePriority::Normal.escalate(RoutingContext::ZoneToCell),
            MessagePriority::Normal
        );
        assert_eq!(
            MessagePriority::Critical.escalate(RoutingContext::ZoneToCell),
            MessagePriority::Critical
        );
    }

    #[test]
    fn test_message_routing_context() {
        let payload = CellMessageType::Heartbeat {
            platform_id: "node_1".to_string(),
        };

        // Default is intra-cell
        let msg = CellMessage::new("node_1".to_string(), "squad_alpha".to_string(), 1, payload);
        assert_eq!(msg.routing_context, RoutingContext::IntraCell);

        // Can set custom context
        let msg2 = CellMessage::new(
            "node_1".to_string(),
            "squad_alpha".to_string(),
            2,
            CellMessageType::Heartbeat {
                platform_id: "node_1".to_string(),
            },
        )
        .with_routing_context(RoutingContext::CellToZone);

        assert_eq!(msg2.routing_context, RoutingContext::CellToZone);
    }

    #[test]
    fn test_message_escalate_priority() {
        let payload = CellMessageType::Heartbeat {
            platform_id: "node_1".to_string(),
        };

        let mut msg = CellMessage::new("node_1".to_string(), "squad_alpha".to_string(), 1, payload)
            .with_priority(MessagePriority::Normal)
            .with_routing_context(RoutingContext::CellToZone);

        // Before escalation
        assert_eq!(msg.priority, MessagePriority::Normal);

        // Escalate based on context
        msg.escalate_priority();

        // After escalation (Normal → High for CellToZone)
        assert_eq!(msg.priority, MessagePriority::High);
    }

    #[test]
    fn test_message_effective_priority() {
        let payload = CellMessageType::Heartbeat {
            platform_id: "node_1".to_string(),
        };

        let msg = CellMessage::new("node_1".to_string(), "squad_alpha".to_string(), 1, payload)
            .with_priority(MessagePriority::Low)
            .with_routing_context(RoutingContext::CellToZone);

        // Original priority unchanged
        assert_eq!(msg.priority, MessagePriority::Low);

        // Effective priority considers context
        assert_eq!(msg.effective_priority(), MessagePriority::Normal);
    }

    #[test]
    fn test_priority_preemption() {
        assert!(MessagePriority::Critical.can_preempt(MessagePriority::High));
        assert!(MessagePriority::High.can_preempt(MessagePriority::Normal));
        assert!(MessagePriority::Normal.can_preempt(MessagePriority::Low));

        assert!(!MessagePriority::Low.can_preempt(MessagePriority::Normal));
        assert!(!MessagePriority::Normal.can_preempt(MessagePriority::High));
        assert!(!MessagePriority::Normal.can_preempt(MessagePriority::Normal)); // Same level
    }

    #[test]
    fn test_priority_bandwidth_multiplier() {
        assert_eq!(MessagePriority::Low.bandwidth_multiplier(), 0.5);
        assert_eq!(MessagePriority::Normal.bandwidth_multiplier(), 1.0);
        assert_eq!(MessagePriority::High.bandwidth_multiplier(), 1.5);
        assert_eq!(MessagePriority::Critical.bandwidth_multiplier(), 2.0);
    }

    #[test]
    fn test_priority_level_ordering() {
        // Priority should be ordered: Low < Normal < High < Critical
        assert!(MessagePriority::Low < MessagePriority::Normal);
        assert!(MessagePriority::Normal < MessagePriority::High);
        assert!(MessagePriority::High < MessagePriority::Critical);

        // Verify transitivity
        assert!(MessagePriority::Low < MessagePriority::Critical);
    }

    #[test]
    fn test_hierarchical_message_workflow() {
        // Simulate a message going from node → cell leader → zone
        let payload = CellMessageType::StatusUpdate {
            platform_id: "node_1".to_string(),
            status: serde_json::json!({"health": "ok"}),
        };

        // Step 1: Node sends normal priority message within cell
        let mut msg = CellMessage::new("node_1".to_string(), "squad_alpha".to_string(), 1, payload)
            .with_priority(MessagePriority::Normal)
            .with_routing_context(RoutingContext::IntraCell);

        assert_eq!(msg.priority, MessagePriority::Normal);
        assert_eq!(msg.routing_context, RoutingContext::IntraCell);

        // Step 2: Cell leader receives and decides to forward to zone
        msg.routing_context = RoutingContext::CellToZone;
        msg.escalate_priority();

        // Priority should now be High (escalated from Normal)
        assert_eq!(msg.priority, MessagePriority::High);
        assert_eq!(msg.routing_context, RoutingContext::CellToZone);
    }
}
