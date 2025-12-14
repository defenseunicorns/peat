//! CRDT (Conflict-free Replicated Data Types) for HIVE-Lite
//!
//! Provides lightweight CRDT implementations optimized for BLE sync:
//! - LWW-Register: Last-Writer-Wins for single values
//! - G-Counter: Grow-only counter for metrics
//!
//! These are designed for minimal memory footprint and efficient
//! serialization over constrained BLE connections.

#[cfg(not(feature = "std"))]
use alloc::{collections::BTreeMap, string::String, string::ToString, vec, vec::Vec};
#[cfg(feature = "std")]
use std::collections::BTreeMap;

use crate::NodeId;

/// Timestamp for CRDT operations (milliseconds since epoch or monotonic)
pub type Timestamp = u64;

/// A Last-Writer-Wins Register
///
/// Stores a single value where concurrent writes are resolved by timestamp.
/// Higher timestamp wins. In case of tie, higher node ID wins.
#[derive(Debug, Clone, PartialEq)]
pub struct LwwRegister<T: Clone> {
    /// Current value
    value: T,
    /// Timestamp when value was set
    timestamp: Timestamp,
    /// Node that set the value
    node_id: NodeId,
}

impl<T: Clone + Default> Default for LwwRegister<T> {
    fn default() -> Self {
        Self {
            value: T::default(),
            timestamp: 0,
            node_id: NodeId::default(),
        }
    }
}

impl<T: Clone> LwwRegister<T> {
    /// Create a new register with an initial value
    pub fn new(value: T, timestamp: Timestamp, node_id: NodeId) -> Self {
        Self {
            value,
            timestamp,
            node_id,
        }
    }

    /// Get the current value
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get the timestamp
    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    /// Get the node that set the value
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    /// Set a new value if it has a higher timestamp
    ///
    /// Returns true if the value was updated
    pub fn set(&mut self, value: T, timestamp: Timestamp, node_id: NodeId) -> bool {
        if self.should_update(timestamp, &node_id) {
            self.value = value;
            self.timestamp = timestamp;
            self.node_id = node_id;
            true
        } else {
            false
        }
    }

    /// Merge with another register (LWW semantics)
    ///
    /// Returns true if our value was updated
    pub fn merge(&mut self, other: &LwwRegister<T>) -> bool {
        if self.should_update(other.timestamp, &other.node_id) {
            self.value = other.value.clone();
            self.timestamp = other.timestamp;
            self.node_id = other.node_id;
            true
        } else {
            false
        }
    }

    /// Check if we should update based on timestamp/node_id
    fn should_update(&self, timestamp: Timestamp, node_id: &NodeId) -> bool {
        timestamp > self.timestamp
            || (timestamp == self.timestamp && node_id.as_u32() > self.node_id.as_u32())
    }
}

/// A Grow-only Counter (G-Counter)
///
/// Each node maintains its own count, total is the sum of all counts.
/// Only supports increment operations.
#[derive(Debug, Clone, Default)]
pub struct GCounter {
    /// Per-node counts
    counts: BTreeMap<u32, u64>,
}

impl GCounter {
    /// Create a new empty counter
    pub fn new() -> Self {
        Self {
            counts: BTreeMap::new(),
        }
    }

    /// Get the total count
    pub fn value(&self) -> u64 {
        self.counts.values().sum()
    }

    /// Increment the counter for a node
    pub fn increment(&mut self, node_id: &NodeId, amount: u64) {
        let count = self.counts.entry(node_id.as_u32()).or_insert(0);
        *count = count.saturating_add(amount);
    }

    /// Get the count for a specific node
    pub fn node_count(&self, node_id: &NodeId) -> u64 {
        self.counts.get(&node_id.as_u32()).copied().unwrap_or(0)
    }

    /// Merge with another counter
    ///
    /// Takes the max of each node's count
    pub fn merge(&mut self, other: &GCounter) {
        for (&node_id, &count) in &other.counts {
            let our_count = self.counts.entry(node_id).or_insert(0);
            *our_count = (*our_count).max(count);
        }
    }

    /// Get the number of nodes that have contributed
    pub fn node_count_total(&self) -> usize {
        self.counts.len()
    }

    /// Encode to bytes for transmission
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(4 + self.counts.len() * 12);
        // Number of entries
        buf.extend_from_slice(&(self.counts.len() as u32).to_le_bytes());
        // Each entry: node_id (4 bytes) + count (8 bytes)
        for (&node_id, &count) in &self.counts {
            buf.extend_from_slice(&node_id.to_le_bytes());
            buf.extend_from_slice(&count.to_le_bytes());
        }
        buf
    }

    /// Decode from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }
        let num_entries = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if data.len() < 4 + num_entries * 12 {
            return None;
        }

        let mut counts = BTreeMap::new();
        let mut offset = 4;
        for _ in 0..num_entries {
            let node_id = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            let count = u64::from_le_bytes([
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
                data[offset + 8],
                data[offset + 9],
                data[offset + 10],
                data[offset + 11],
            ]);
            counts.insert(node_id, count);
            offset += 12;
        }

        Some(Self { counts })
    }
}

/// Position data with LWW semantics
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Position {
    /// Latitude in degrees
    pub latitude: f32,
    /// Longitude in degrees
    pub longitude: f32,
    /// Altitude in meters (optional)
    pub altitude: Option<f32>,
    /// Accuracy in meters (optional)
    pub accuracy: Option<f32>,
}

impl Position {
    /// Create a new position
    pub fn new(latitude: f32, longitude: f32) -> Self {
        Self {
            latitude,
            longitude,
            altitude: None,
            accuracy: None,
        }
    }

    /// Create with altitude
    pub fn with_altitude(mut self, altitude: f32) -> Self {
        self.altitude = Some(altitude);
        self
    }

    /// Create with accuracy
    pub fn with_accuracy(mut self, accuracy: f32) -> Self {
        self.accuracy = Some(accuracy);
        self
    }

    /// Encode to bytes (12-20 bytes)
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(20);
        buf.extend_from_slice(&self.latitude.to_le_bytes());
        buf.extend_from_slice(&self.longitude.to_le_bytes());

        // Flags byte: bit 0 = has altitude, bit 1 = has accuracy
        let mut flags = 0u8;
        if self.altitude.is_some() {
            flags |= 0x01;
        }
        if self.accuracy.is_some() {
            flags |= 0x02;
        }
        buf.push(flags);

        if let Some(alt) = self.altitude {
            buf.extend_from_slice(&alt.to_le_bytes());
        }
        if let Some(acc) = self.accuracy {
            buf.extend_from_slice(&acc.to_le_bytes());
        }
        buf
    }

    /// Decode from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 9 {
            return None;
        }

        let latitude = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let longitude = f32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let flags = data[8];

        let mut pos = Self::new(latitude, longitude);
        let mut offset = 9;

        if flags & 0x01 != 0 {
            if data.len() < offset + 4 {
                return None;
            }
            pos.altitude = Some(f32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]));
            offset += 4;
        }

        if flags & 0x02 != 0 {
            if data.len() < offset + 4 {
                return None;
            }
            pos.accuracy = Some(f32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]));
        }

        Some(pos)
    }
}

/// Health status data with LWW semantics
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HealthStatus {
    /// Battery percentage (0-100)
    pub battery_percent: u8,
    /// Heart rate BPM (optional)
    pub heart_rate: Option<u8>,
    /// Activity level (0=still, 1=walking, 2=running, 3=vehicle)
    pub activity: u8,
    /// Alert status flags
    pub alerts: u8,
}

impl HealthStatus {
    /// Alert flag: Man down
    pub const ALERT_MAN_DOWN: u8 = 0x01;
    /// Alert flag: Low battery
    pub const ALERT_LOW_BATTERY: u8 = 0x02;
    /// Alert flag: Out of range
    pub const ALERT_OUT_OF_RANGE: u8 = 0x04;
    /// Alert flag: Custom alert 1
    pub const ALERT_CUSTOM_1: u8 = 0x08;

    /// Create a new health status
    pub fn new(battery_percent: u8) -> Self {
        Self {
            battery_percent,
            heart_rate: None,
            activity: 0,
            alerts: 0,
        }
    }

    /// Set heart rate
    pub fn with_heart_rate(mut self, hr: u8) -> Self {
        self.heart_rate = Some(hr);
        self
    }

    /// Set activity level
    pub fn with_activity(mut self, activity: u8) -> Self {
        self.activity = activity;
        self
    }

    /// Set alert flag
    pub fn set_alert(&mut self, flag: u8) {
        self.alerts |= flag;
    }

    /// Clear alert flag
    pub fn clear_alert(&mut self, flag: u8) {
        self.alerts &= !flag;
    }

    /// Check if alert is set
    pub fn has_alert(&self, flag: u8) -> bool {
        self.alerts & flag != 0
    }

    /// Encode to bytes (3-4 bytes)
    pub fn encode(&self) -> Vec<u8> {
        vec![
            self.battery_percent,
            self.activity,
            self.alerts,
            // Heart rate: 0 means not present, otherwise value
            self.heart_rate.unwrap_or(0),
        ]
    }

    /// Decode from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }
        let mut status = Self::new(data[0]);
        status.activity = data[1];
        status.alerts = data[2];
        if data[3] > 0 {
            status.heart_rate = Some(data[3]);
        }
        Some(status)
    }
}

// ============================================================================
// Peripheral (Sub-node) Types - for soldier-attached devices like M5Stack Core2
// ============================================================================

/// Type of peripheral device
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum PeripheralType {
    /// Unknown/unspecified
    #[default]
    Unknown = 0,
    /// Soldier-worn sensor (e.g., M5Stack Core2)
    SoldierSensor = 1,
    /// Fixed/stationary sensor
    FixedSensor = 2,
    /// Mesh relay only (no sensors)
    Relay = 3,
}

impl PeripheralType {
    /// Convert from u8 value
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::SoldierSensor,
            2 => Self::FixedSensor,
            3 => Self::Relay,
            _ => Self::Unknown,
        }
    }
}

/// Event types that a peripheral can emit (e.g., from tap input)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum EventType {
    /// No event / cleared
    #[default]
    None = 0,
    /// "I'm OK" ping
    Ping = 1,
    /// Request assistance
    NeedAssist = 2,
    /// Emergency / SOS
    Emergency = 3,
    /// Moving / in transit
    Moving = 4,
    /// In position / stationary
    InPosition = 5,
    /// Acknowledged / copy
    Ack = 6,
}

impl EventType {
    /// Convert from u8 value
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Ping,
            2 => Self::NeedAssist,
            3 => Self::Emergency,
            4 => Self::Moving,
            5 => Self::InPosition,
            6 => Self::Ack,
            _ => Self::None,
        }
    }

    /// Human-readable label for display
    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "",
            Self::Ping => "PING",
            Self::NeedAssist => "NEED ASSIST",
            Self::Emergency => "EMERGENCY",
            Self::Moving => "MOVING",
            Self::InPosition => "IN POSITION",
            Self::Ack => "ACK",
        }
    }
}

/// An event emitted by a peripheral (e.g., tap on Core2)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PeripheralEvent {
    /// Type of event
    pub event_type: EventType,
    /// Timestamp when event occurred (ms since epoch or boot)
    pub timestamp: u64,
}

impl PeripheralEvent {
    /// Create a new peripheral event
    pub fn new(event_type: EventType, timestamp: u64) -> Self {
        Self {
            event_type,
            timestamp,
        }
    }

    /// Encode to bytes (9 bytes)
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(9);
        buf.push(self.event_type as u8);
        buf.extend_from_slice(&self.timestamp.to_le_bytes());
        buf
    }

    /// Decode from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 9 {
            return None;
        }
        Some(Self {
            event_type: EventType::from_u8(data[0]),
            timestamp: u64::from_le_bytes([
                data[1], data[2], data[3], data[4], data[5], data[6], data[7], data[8],
            ]),
        })
    }
}

/// A peripheral device attached to a Node (soldier)
///
/// Peripherals are sub-tier devices that augment a soldier's capabilities
/// with sensors and input (e.g., M5Stack Core2 wearable).
#[derive(Debug, Clone, Default)]
pub struct Peripheral {
    /// Unique peripheral ID (derived from device MAC or similar)
    pub id: u32,
    /// Parent Node ID this peripheral is attached to (0 if not paired)
    pub parent_node: u32,
    /// Type of peripheral
    pub peripheral_type: PeripheralType,
    /// Callsign/name (inherited from parent or configured)
    pub callsign: [u8; 12],
    /// Current health status
    pub health: HealthStatus,
    /// Most recent event (if any)
    pub last_event: Option<PeripheralEvent>,
    /// Last update timestamp
    pub timestamp: u64,
}

impl Peripheral {
    /// Create a new peripheral
    pub fn new(id: u32, peripheral_type: PeripheralType) -> Self {
        Self {
            id,
            parent_node: 0,
            peripheral_type,
            callsign: [0u8; 12],
            health: HealthStatus::default(),
            last_event: None,
            timestamp: 0,
        }
    }

    /// Set the callsign (truncated to 12 bytes)
    pub fn with_callsign(mut self, callsign: &str) -> Self {
        let bytes = callsign.as_bytes();
        let len = bytes.len().min(12);
        self.callsign[..len].copy_from_slice(&bytes[..len]);
        self
    }

    /// Get callsign as string
    pub fn callsign_str(&self) -> &str {
        let len = self.callsign.iter().position(|&b| b == 0).unwrap_or(12);
        core::str::from_utf8(&self.callsign[..len]).unwrap_or("")
    }

    /// Set parent node
    pub fn with_parent(mut self, parent_node: u32) -> Self {
        self.parent_node = parent_node;
        self
    }

    /// Record an event
    pub fn set_event(&mut self, event_type: EventType, timestamp: u64) {
        self.last_event = Some(PeripheralEvent::new(event_type, timestamp));
        self.timestamp = timestamp;
    }

    /// Clear the last event
    pub fn clear_event(&mut self) {
        self.last_event = None;
    }

    /// Encode to bytes for BLE transmission
    /// Format: [id:4][parent:4][type:1][callsign:12][health:4][has_event:1][event:9?][timestamp:8]
    /// Size: 34 bytes without event, 43 bytes with event
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(43);
        buf.extend_from_slice(&self.id.to_le_bytes());
        buf.extend_from_slice(&self.parent_node.to_le_bytes());
        buf.push(self.peripheral_type as u8);
        buf.extend_from_slice(&self.callsign);
        buf.extend_from_slice(&self.health.encode());

        if let Some(ref event) = self.last_event {
            buf.push(1); // has event
            buf.extend_from_slice(&event.encode());
        } else {
            buf.push(0); // no event
        }

        buf.extend_from_slice(&self.timestamp.to_le_bytes());
        buf
    }

    /// Decode from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 34 {
            return None;
        }

        let id = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let parent_node = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let peripheral_type = PeripheralType::from_u8(data[8]);

        let mut callsign = [0u8; 12];
        callsign.copy_from_slice(&data[9..21]);

        let health = HealthStatus::decode(&data[21..25])?;

        let has_event = data[25] != 0;
        let (last_event, timestamp_offset) = if has_event {
            if data.len() < 43 {
                return None;
            }
            (PeripheralEvent::decode(&data[26..35]), 35)
        } else {
            (None, 26)
        };

        if data.len() < timestamp_offset + 8 {
            return None;
        }

        let timestamp = u64::from_le_bytes([
            data[timestamp_offset],
            data[timestamp_offset + 1],
            data[timestamp_offset + 2],
            data[timestamp_offset + 3],
            data[timestamp_offset + 4],
            data[timestamp_offset + 5],
            data[timestamp_offset + 6],
            data[timestamp_offset + 7],
        ]);

        Some(Self {
            id,
            parent_node,
            peripheral_type,
            callsign,
            health,
            last_event,
            timestamp,
        })
    }
}

/// CRDT operation types for sync
#[derive(Debug, Clone)]
pub enum CrdtOperation {
    /// Update a position register
    UpdatePosition {
        /// Node ID that owns this position
        node_id: NodeId,
        /// Position data
        position: Position,
        /// Timestamp of the update
        timestamp: Timestamp,
    },
    /// Update health status register
    UpdateHealth {
        /// Node ID that owns this status
        node_id: NodeId,
        /// Health status data
        status: HealthStatus,
        /// Timestamp of the update
        timestamp: Timestamp,
    },
    /// Increment a counter
    IncrementCounter {
        /// Counter identifier
        counter_id: u8,
        /// Node performing the increment
        node_id: NodeId,
        /// Amount to increment
        amount: u64,
    },
    /// Generic LWW update (key-value)
    UpdateRegister {
        /// Key for the register
        key: String,
        /// Value data
        value: Vec<u8>,
        /// Timestamp of the update
        timestamp: Timestamp,
        /// Node that set the value
        node_id: NodeId,
    },
}

impl CrdtOperation {
    /// Get the approximate size in bytes
    pub fn size(&self) -> usize {
        match self {
            CrdtOperation::UpdatePosition { position, .. } => 4 + 8 + position.encode().len(),
            CrdtOperation::UpdateHealth { status, .. } => 4 + 8 + status.encode().len(),
            CrdtOperation::IncrementCounter { .. } => 1 + 4 + 8,
            CrdtOperation::UpdateRegister { key, value, .. } => 4 + 8 + key.len() + value.len(),
        }
    }

    /// Encode to bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        match self {
            CrdtOperation::UpdatePosition {
                node_id,
                position,
                timestamp,
            } => {
                buf.push(0x01); // Type tag
                buf.extend_from_slice(&node_id.as_u32().to_le_bytes());
                buf.extend_from_slice(&timestamp.to_le_bytes());
                buf.extend_from_slice(&position.encode());
            }
            CrdtOperation::UpdateHealth {
                node_id,
                status,
                timestamp,
            } => {
                buf.push(0x02); // Type tag
                buf.extend_from_slice(&node_id.as_u32().to_le_bytes());
                buf.extend_from_slice(&timestamp.to_le_bytes());
                buf.extend_from_slice(&status.encode());
            }
            CrdtOperation::IncrementCounter {
                counter_id,
                node_id,
                amount,
            } => {
                buf.push(0x03); // Type tag
                buf.push(*counter_id);
                buf.extend_from_slice(&node_id.as_u32().to_le_bytes());
                buf.extend_from_slice(&amount.to_le_bytes());
            }
            CrdtOperation::UpdateRegister {
                key,
                value,
                timestamp,
                node_id,
            } => {
                buf.push(0x04); // Type tag
                buf.extend_from_slice(&node_id.as_u32().to_le_bytes());
                buf.extend_from_slice(&timestamp.to_le_bytes());
                buf.push(key.len() as u8);
                buf.extend_from_slice(key.as_bytes());
                buf.extend_from_slice(&(value.len() as u16).to_le_bytes());
                buf.extend_from_slice(value);
            }
        }
        buf
    }

    /// Decode from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }

        match data[0] {
            0x01 => {
                // UpdatePosition
                if data.len() < 13 {
                    return None;
                }
                let node_id = NodeId::new(u32::from_le_bytes([data[1], data[2], data[3], data[4]]));
                let timestamp = u64::from_le_bytes([
                    data[5], data[6], data[7], data[8], data[9], data[10], data[11], data[12],
                ]);
                let position = Position::decode(&data[13..])?;
                Some(CrdtOperation::UpdatePosition {
                    node_id,
                    position,
                    timestamp,
                })
            }
            0x02 => {
                // UpdateHealth
                if data.len() < 13 {
                    return None;
                }
                let node_id = NodeId::new(u32::from_le_bytes([data[1], data[2], data[3], data[4]]));
                let timestamp = u64::from_le_bytes([
                    data[5], data[6], data[7], data[8], data[9], data[10], data[11], data[12],
                ]);
                let status = HealthStatus::decode(&data[13..])?;
                Some(CrdtOperation::UpdateHealth {
                    node_id,
                    status,
                    timestamp,
                })
            }
            0x03 => {
                // IncrementCounter
                if data.len() < 14 {
                    return None;
                }
                let counter_id = data[1];
                let node_id = NodeId::new(u32::from_le_bytes([data[2], data[3], data[4], data[5]]));
                let amount = u64::from_le_bytes([
                    data[6], data[7], data[8], data[9], data[10], data[11], data[12], data[13],
                ]);
                Some(CrdtOperation::IncrementCounter {
                    counter_id,
                    node_id,
                    amount,
                })
            }
            0x04 => {
                // UpdateRegister
                if data.len() < 14 {
                    return None;
                }
                let node_id = NodeId::new(u32::from_le_bytes([data[1], data[2], data[3], data[4]]));
                let timestamp = u64::from_le_bytes([
                    data[5], data[6], data[7], data[8], data[9], data[10], data[11], data[12],
                ]);
                let key_len = data[13] as usize;
                if data.len() < 14 + key_len + 2 {
                    return None;
                }
                let key = core::str::from_utf8(&data[14..14 + key_len])
                    .ok()?
                    .to_string();
                let value_len =
                    u16::from_le_bytes([data[14 + key_len], data[15 + key_len]]) as usize;
                if data.len() < 16 + key_len + value_len {
                    return None;
                }
                let value = data[16 + key_len..16 + key_len + value_len].to_vec();
                Some(CrdtOperation::UpdateRegister {
                    key,
                    value,
                    timestamp,
                    node_id,
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lww_register_basic() {
        let mut reg = LwwRegister::new(42u32, 100, NodeId::new(1));
        assert_eq!(*reg.get(), 42);
        assert_eq!(reg.timestamp(), 100);

        // Higher timestamp wins
        assert!(reg.set(99, 200, NodeId::new(2)));
        assert_eq!(*reg.get(), 99);

        // Lower timestamp loses
        assert!(!reg.set(50, 150, NodeId::new(3)));
        assert_eq!(*reg.get(), 99);
    }

    #[test]
    fn test_lww_register_tiebreak() {
        let mut reg = LwwRegister::new(1u32, 100, NodeId::new(1));

        // Same timestamp, higher node_id wins
        assert!(reg.set(2, 100, NodeId::new(2)));
        assert_eq!(*reg.get(), 2);

        // Same timestamp, lower node_id loses
        assert!(!reg.set(3, 100, NodeId::new(1)));
        assert_eq!(*reg.get(), 2);
    }

    #[test]
    fn test_lww_register_merge() {
        let mut reg1 = LwwRegister::new(1u32, 100, NodeId::new(1));
        let reg2 = LwwRegister::new(2u32, 200, NodeId::new(2));

        assert!(reg1.merge(&reg2));
        assert_eq!(*reg1.get(), 2);
    }

    #[test]
    fn test_gcounter_basic() {
        let mut counter = GCounter::new();
        let node1 = NodeId::new(1);
        let node2 = NodeId::new(2);

        counter.increment(&node1, 5);
        counter.increment(&node2, 3);
        counter.increment(&node1, 2);

        assert_eq!(counter.value(), 10);
        assert_eq!(counter.node_count(&node1), 7);
        assert_eq!(counter.node_count(&node2), 3);
    }

    #[test]
    fn test_gcounter_merge() {
        let mut counter1 = GCounter::new();
        let mut counter2 = GCounter::new();
        let node1 = NodeId::new(1);
        let node2 = NodeId::new(2);

        counter1.increment(&node1, 5);
        counter2.increment(&node1, 3);
        counter2.increment(&node2, 4);

        counter1.merge(&counter2);

        assert_eq!(counter1.value(), 9); // max(5,3) + 4
        assert_eq!(counter1.node_count(&node1), 5);
        assert_eq!(counter1.node_count(&node2), 4);
    }

    #[test]
    fn test_gcounter_encode_decode() {
        let mut counter = GCounter::new();
        counter.increment(&NodeId::new(1), 5);
        counter.increment(&NodeId::new(2), 10);

        let encoded = counter.encode();
        let decoded = GCounter::decode(&encoded).unwrap();

        assert_eq!(decoded.value(), counter.value());
        assert_eq!(decoded.node_count(&NodeId::new(1)), 5);
        assert_eq!(decoded.node_count(&NodeId::new(2)), 10);
    }

    #[test]
    fn test_position_encode_decode() {
        let pos = Position::new(37.7749, -122.4194)
            .with_altitude(100.0)
            .with_accuracy(5.0);

        let encoded = pos.encode();
        let decoded = Position::decode(&encoded).unwrap();

        assert_eq!(decoded.latitude, pos.latitude);
        assert_eq!(decoded.longitude, pos.longitude);
        assert_eq!(decoded.altitude, pos.altitude);
        assert_eq!(decoded.accuracy, pos.accuracy);
    }

    #[test]
    fn test_position_minimal_encode() {
        let pos = Position::new(0.0, 0.0);
        let encoded = pos.encode();
        assert_eq!(encoded.len(), 9); // lat + lon + flags

        let pos_with_alt = Position::new(0.0, 0.0).with_altitude(0.0);
        let encoded_alt = pos_with_alt.encode();
        assert_eq!(encoded_alt.len(), 13);
    }

    #[test]
    fn test_health_status() {
        let mut status = HealthStatus::new(85).with_heart_rate(72).with_activity(1);

        assert_eq!(status.battery_percent, 85);
        assert_eq!(status.heart_rate, Some(72));
        assert!(!status.has_alert(HealthStatus::ALERT_MAN_DOWN));

        status.set_alert(HealthStatus::ALERT_MAN_DOWN);
        assert!(status.has_alert(HealthStatus::ALERT_MAN_DOWN));

        let encoded = status.encode();
        let decoded = HealthStatus::decode(&encoded).unwrap();
        assert_eq!(decoded.battery_percent, 85);
        assert_eq!(decoded.heart_rate, Some(72));
        assert!(decoded.has_alert(HealthStatus::ALERT_MAN_DOWN));
    }

    #[test]
    fn test_crdt_operation_position() {
        let op = CrdtOperation::UpdatePosition {
            node_id: NodeId::new(0x1234),
            position: Position::new(37.0, -122.0),
            timestamp: 1000,
        };

        let encoded = op.encode();
        let decoded = CrdtOperation::decode(&encoded).unwrap();

        if let CrdtOperation::UpdatePosition {
            node_id,
            position,
            timestamp,
        } = decoded
        {
            assert_eq!(node_id.as_u32(), 0x1234);
            assert_eq!(timestamp, 1000);
            assert_eq!(position.latitude, 37.0);
        } else {
            panic!("Wrong operation type");
        }
    }

    #[test]
    fn test_crdt_operation_counter() {
        let op = CrdtOperation::IncrementCounter {
            counter_id: 1,
            node_id: NodeId::new(0x5678),
            amount: 42,
        };

        let encoded = op.encode();
        let decoded = CrdtOperation::decode(&encoded).unwrap();

        if let CrdtOperation::IncrementCounter {
            counter_id,
            node_id,
            amount,
        } = decoded
        {
            assert_eq!(counter_id, 1);
            assert_eq!(node_id.as_u32(), 0x5678);
            assert_eq!(amount, 42);
        } else {
            panic!("Wrong operation type");
        }
    }

    #[test]
    fn test_crdt_operation_size() {
        let pos_op = CrdtOperation::UpdatePosition {
            node_id: NodeId::new(1),
            position: Position::new(0.0, 0.0),
            timestamp: 0,
        };
        assert!(pos_op.size() > 0);

        let counter_op = CrdtOperation::IncrementCounter {
            counter_id: 0,
            node_id: NodeId::new(1),
            amount: 1,
        };
        assert_eq!(counter_op.size(), 13);
    }

    // ============================================================================
    // Peripheral Tests
    // ============================================================================

    #[test]
    fn test_peripheral_type_from_u8() {
        assert_eq!(PeripheralType::from_u8(0), PeripheralType::Unknown);
        assert_eq!(PeripheralType::from_u8(1), PeripheralType::SoldierSensor);
        assert_eq!(PeripheralType::from_u8(2), PeripheralType::FixedSensor);
        assert_eq!(PeripheralType::from_u8(3), PeripheralType::Relay);
        assert_eq!(PeripheralType::from_u8(99), PeripheralType::Unknown);
    }

    #[test]
    fn test_event_type_from_u8() {
        assert_eq!(EventType::from_u8(0), EventType::None);
        assert_eq!(EventType::from_u8(1), EventType::Ping);
        assert_eq!(EventType::from_u8(2), EventType::NeedAssist);
        assert_eq!(EventType::from_u8(3), EventType::Emergency);
        assert_eq!(EventType::from_u8(4), EventType::Moving);
        assert_eq!(EventType::from_u8(5), EventType::InPosition);
        assert_eq!(EventType::from_u8(6), EventType::Ack);
        assert_eq!(EventType::from_u8(99), EventType::None);
    }

    #[test]
    fn test_event_type_labels() {
        assert_eq!(EventType::None.label(), "");
        assert_eq!(EventType::Emergency.label(), "EMERGENCY");
        assert_eq!(EventType::Ping.label(), "PING");
    }

    #[test]
    fn test_peripheral_event_encode_decode() {
        let event = PeripheralEvent::new(EventType::Emergency, 1234567890);
        let encoded = event.encode();
        assert_eq!(encoded.len(), 9);

        let decoded = PeripheralEvent::decode(&encoded).unwrap();
        assert_eq!(decoded.event_type, EventType::Emergency);
        assert_eq!(decoded.timestamp, 1234567890);
    }

    #[test]
    fn test_peripheral_new() {
        let peripheral = Peripheral::new(0x12345678, PeripheralType::SoldierSensor);
        assert_eq!(peripheral.id, 0x12345678);
        assert_eq!(peripheral.peripheral_type, PeripheralType::SoldierSensor);
        assert_eq!(peripheral.parent_node, 0);
        assert!(peripheral.last_event.is_none());
    }

    #[test]
    fn test_peripheral_with_callsign() {
        let peripheral = Peripheral::new(1, PeripheralType::SoldierSensor).with_callsign("ALPHA-1");
        assert_eq!(peripheral.callsign_str(), "ALPHA-1");

        // Test truncation
        let peripheral2 = Peripheral::new(2, PeripheralType::SoldierSensor)
            .with_callsign("THIS_IS_A_VERY_LONG_CALLSIGN");
        assert_eq!(peripheral2.callsign_str(), "THIS_IS_A_VE");
    }

    #[test]
    fn test_peripheral_set_event() {
        let mut peripheral = Peripheral::new(1, PeripheralType::SoldierSensor);
        peripheral.set_event(EventType::Emergency, 1000);

        assert!(peripheral.last_event.is_some());
        let event = peripheral.last_event.as_ref().unwrap();
        assert_eq!(event.event_type, EventType::Emergency);
        assert_eq!(event.timestamp, 1000);
        assert_eq!(peripheral.timestamp, 1000);

        peripheral.clear_event();
        assert!(peripheral.last_event.is_none());
    }

    #[test]
    fn test_peripheral_encode_decode_without_event() {
        let peripheral = Peripheral::new(0xAABBCCDD, PeripheralType::SoldierSensor)
            .with_callsign("BRAVO-2")
            .with_parent(0x11223344);

        let encoded = peripheral.encode();
        assert_eq!(encoded.len(), 34); // No event

        let decoded = Peripheral::decode(&encoded).unwrap();
        assert_eq!(decoded.id, 0xAABBCCDD);
        assert_eq!(decoded.parent_node, 0x11223344);
        assert_eq!(decoded.peripheral_type, PeripheralType::SoldierSensor);
        assert_eq!(decoded.callsign_str(), "BRAVO-2");
        assert!(decoded.last_event.is_none());
    }

    #[test]
    fn test_peripheral_encode_decode_with_event() {
        let mut peripheral = Peripheral::new(0x12345678, PeripheralType::SoldierSensor)
            .with_callsign("CHARLIE")
            .with_parent(0x87654321);
        peripheral.health = HealthStatus::new(85);
        peripheral.set_event(EventType::NeedAssist, 9999);

        let encoded = peripheral.encode();
        assert_eq!(encoded.len(), 43); // With event

        let decoded = Peripheral::decode(&encoded).unwrap();
        assert_eq!(decoded.id, 0x12345678);
        assert_eq!(decoded.parent_node, 0x87654321);
        assert_eq!(decoded.callsign_str(), "CHARLIE");
        assert_eq!(decoded.health.battery_percent, 85);
        assert!(decoded.last_event.is_some());
        let event = decoded.last_event.as_ref().unwrap();
        assert_eq!(event.event_type, EventType::NeedAssist);
        assert_eq!(event.timestamp, 9999);
    }

    #[test]
    fn test_peripheral_decode_invalid_data() {
        // Too short
        assert!(Peripheral::decode(&[0u8; 10]).is_none());

        // Valid length but no event
        let mut data = vec![0u8; 34];
        data[25] = 0; // no event flag
        assert!(Peripheral::decode(&data).is_some());

        // Claims to have event but too short
        data[25] = 1; // has event flag
        assert!(Peripheral::decode(&data).is_none());
    }
}
