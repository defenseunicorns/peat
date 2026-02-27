//! BLE Translation Layer (ADR-041, #557)
//!
//! Provides bidirectional translation between peat-btle lightweight CRDTs
//! and Peat Protocol Automerge documents. This enables gateway nodes to
//! bridge between:
//!
//! - **Full Peat nodes** (ATAK, CLI) using Automerge documents
//! - **WearTAK devices** (Samsung Watch) using peat-btle lightweight CRDTs
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                       Gateway Node (ATAK)                           │
//! │  ┌─────────────────────────────────────────────────────────────────┐│
//! │  │                    BLE Translation Layer                        ││
//! │  │                                                                 ││
//! │  │   peat-btle Position ←──────────→ TrackInfo document           ││
//! │  │   peat-btle HealthStatus ←──────→ Platform health fields       ││
//! │  │   peat-btle EmergencyEvent ←────→ Alert document               ││
//! │  │   peat-btle CannedMessage ←─────→ CannedMessage document      ││
//! │  │   peat-btle GCounter ←──────────→ Automerge counter            ││
//! │  └─────────────────────────────────────────────────────────────────┘│
//! │            ▲                                    ▲                   │
//! │            │                                    │                   │
//! │   ┌────────▼────────┐                ┌─────────▼─────────┐        │
//! │   │  PeatBleTransport│                │  IrohMeshTransport│        │
//! │   │  (BLE mesh)      │                │  (QUIC/WiFi)      │        │
//! │   └─────────────────┘                └───────────────────┘        │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use peat_protocol::sync::ble_translation::{BleTranslator, TranslationConfig};
//!
//! let translator = BleTranslator::new(TranslationConfig::default());
//!
//! // Translate BLE position to track document
//! let track_doc = translator.position_to_track(&ble_position, &peripheral_id);
//!
//! // Translate track document to BLE position
//! let ble_position = translator.track_to_position(&track_doc);
//! ```

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Configuration for BLE translation
#[derive(Debug, Clone)]
pub struct TranslationConfig {
    /// Collection name for tracks (default: "tracks")
    pub tracks_collection: String,
    /// Collection name for platforms/peripherals (default: "platforms")
    pub platforms_collection: String,
    /// Collection name for alerts/emergencies (default: "alerts")
    pub alerts_collection: String,
    /// Collection name for canned messages (default: "canned_messages")
    pub canned_messages_collection: String,
    /// Default classification for BLE-originated tracks
    pub default_classification: String,
    /// ID prefix for BLE-originated documents
    pub ble_id_prefix: String,
}

impl Default for TranslationConfig {
    fn default() -> Self {
        Self {
            tracks_collection: "tracks".to_string(),
            platforms_collection: "platforms".to_string(),
            alerts_collection: "alerts".to_string(),
            canned_messages_collection: "canned_messages".to_string(),
            default_classification: "a-f-G-U-C".to_string(), // Friendly ground unit
            ble_id_prefix: "ble-".to_string(),
        }
    }
}

/// BLE position data (mirrors peat_btle::Position)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlePosition {
    /// Latitude in degrees (WGS84)
    pub latitude: f32,
    /// Longitude in degrees (WGS84)
    pub longitude: f32,
    /// Altitude in meters (optional)
    pub altitude: Option<f32>,
    /// Accuracy/CEP in meters (optional)
    pub accuracy: Option<f32>,
}

/// BLE health status (mirrors peat_btle::HealthStatus)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BleHealthStatus {
    /// Battery percentage (0-100)
    pub battery_percent: u8,
    /// Heart rate in BPM (optional)
    pub heart_rate: Option<u8>,
    /// Activity level (0=still, 1=walking, 2=running, 3=vehicle)
    pub activity: u8,
    /// Alert flags (bitfield)
    pub alerts: u8,
}

impl BleHealthStatus {
    /// Alert flag: Man down detected
    pub const ALERT_MAN_DOWN: u8 = 0x01;
    /// Alert flag: Low battery
    pub const ALERT_LOW_BATTERY: u8 = 0x02;
    /// Alert flag: Out of range
    pub const ALERT_OUT_OF_RANGE: u8 = 0x04;
    /// Alert flag: Custom alert 1
    pub const ALERT_CUSTOM_1: u8 = 0x08;

    /// Check if man-down alert is active
    pub fn is_man_down(&self) -> bool {
        self.alerts & Self::ALERT_MAN_DOWN != 0
    }

    /// Check if low battery alert is active
    pub fn is_low_battery(&self) -> bool {
        self.alerts & Self::ALERT_LOW_BATTERY != 0
    }
}

/// BLE emergency event (mirrors peat_btle::EmergencyEvent)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BleEmergencyEvent {
    /// Source node ID that triggered emergency
    pub source_node: u32,
    /// Timestamp when triggered (ms since epoch)
    pub timestamp: u64,
    /// ACK status for each known peer (node_id -> acked)
    pub acks: HashMap<u32, bool>,
}

/// BLE canned message (mirrors peat_lite::CannedMessageAckEvent)
///
/// Represents a pre-defined message from a WearTAK device, with ACK tracking.
/// The ACK map uses OR-set CRDT semantics: each acker_node maps to its ack_timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BleCannedMessage {
    /// Message code (0x00-0xFF, maps to peat-lite CannedMessage enum)
    pub message_code: u8,
    /// Human-readable message name
    pub message_name: String,
    /// Source node ID that sent this message
    pub source_node: u32,
    /// Target node (None for broadcast)
    pub target_node: Option<u32>,
    /// Timestamp when sent (ms since epoch)
    pub timestamp: u64,
    /// Sequence number for deduplication
    pub sequence: u32,
    /// ACK tracking: acker_node_id -> ack_timestamp (OR-set CRDT)
    pub acks: HashMap<u32, u64>,
}

/// Map a peat-lite CannedMessage code to its human-readable name.
///
/// Covers all 20 defined codes from peat-lite v0.0.4 plus a fallback for unknown codes.
pub fn message_name_from_code(code: u8) -> &'static str {
    match code {
        // Acknowledgments (0x00-0x0F)
        0x00 => "Ack",
        0x01 => "Ack Wilco",
        0x02 => "Ack Negative",
        0x03 => "Say Again",
        // Status (0x10-0x1F)
        0x10 => "Check In",
        0x11 => "Moving",
        0x12 => "Holding",
        0x13 => "On Station",
        0x14 => "Returning",
        0x15 => "Complete",
        // Alerts (0x20-0x2F)
        0x20 => "Emergency",
        0x21 => "Alert",
        0x22 => "All Clear",
        0x23 => "Contact",
        0x24 => "Under Fire",
        // Requests (0x30-0x3F)
        0x30 => "Need Extract",
        0x31 => "Need Support",
        0x32 => "Need Medic",
        0x33 => "Need Resupply",
        // Reserved
        0xFF => "Custom",
        _ => "Unknown",
    }
}

/// BLE peripheral data (mirrors peat_btle::Peripheral)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlePeripheral {
    /// Unique peripheral ID
    pub id: u32,
    /// Parent node ID (0 if unpaired)
    pub parent_node: u32,
    /// Peripheral type
    pub peripheral_type: BlePeripheralType,
    /// Callsign (up to 12 chars)
    pub callsign: String,
    /// Health status
    pub health: BleHealthStatus,
    /// Last update timestamp (ms since epoch)
    pub timestamp: u64,
    /// Position (optional)
    pub position: Option<BlePosition>,
}

/// BLE peripheral types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum BlePeripheralType {
    Unknown = 0,
    SoldierSensor = 1,
    FixedSensor = 2,
    Relay = 3,
}

impl From<u8> for BlePeripheralType {
    fn from(v: u8) -> Self {
        match v {
            1 => Self::SoldierSensor,
            2 => Self::FixedSensor,
            3 => Self::Relay,
            _ => Self::Unknown,
        }
    }
}

/// Translator between BLE CRDTs and Automerge documents
#[derive(Debug, Clone)]
pub struct BleTranslator {
    config: TranslationConfig,
}

impl BleTranslator {
    /// Create a new translator with the given configuration
    pub fn new(config: TranslationConfig) -> Self {
        Self { config }
    }

    /// Create a translator with default configuration
    pub fn with_defaults() -> Self {
        Self::new(TranslationConfig::default())
    }

    // =========================================================================
    // Position <-> Track Translation
    // =========================================================================

    /// Convert BLE position to track document JSON
    ///
    /// Creates a track document suitable for storage in the tracks collection.
    /// Note: This version does not set cell_id. Use `position_to_track_in_cell`
    /// to include cell membership based on BLE mesh_id.
    pub fn position_to_track(
        &self,
        position: &BlePosition,
        peripheral_id: u32,
        callsign: Option<&str>,
    ) -> Value {
        self.position_to_track_in_cell(position, peripheral_id, callsign, None)
    }

    /// Convert BLE position to track document JSON with cell membership
    ///
    /// The `mesh_id` parameter (from BLE mesh configuration) is used as the cell_id,
    /// allowing BLE-originated tracks to be associated with PEAT cells.
    ///
    /// # Arguments
    /// * `position` - The BLE position data
    /// * `peripheral_id` - The BLE peripheral ID
    /// * `callsign` - Optional callsign for the track
    /// * `mesh_id` - Optional BLE mesh ID to use as cell_id
    pub fn position_to_track_in_cell(
        &self,
        position: &BlePosition,
        peripheral_id: u32,
        callsign: Option<&str>,
        mesh_id: Option<&str>,
    ) -> Value {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let track_id = format!("{}{:08X}", self.config.ble_id_prefix, peripheral_id);
        let source = callsign.unwrap_or(&track_id);

        let mut track = json!({
            "id": track_id,
            "source_platform": format!("ble-{:08X}", peripheral_id),
            "lat": position.latitude as f64,
            "lon": position.longitude as f64,
            "hae": position.altitude.map(|a| a as f64),
            "cep": position.accuracy.map(|a| a as f64),
            "classification": self.config.default_classification,
            "confidence": 0.9,
            "category": "friendly",
            "callsign": source,
            "created_at": now_ms,
            "last_update": now_ms,
            "ble_origin": true
        });

        // Set cell_id from BLE mesh_id (mesh_id == cell_id mapping)
        if let Some(cell_id) = mesh_id {
            track["cell_id"] = json!(cell_id);
        }

        track
    }

    /// Extract BLE position from track document JSON
    ///
    /// Returns None if the document doesn't have required position fields.
    pub fn track_to_position(&self, track: &Value) -> Option<BlePosition> {
        let lat = track.get("lat")?.as_f64()? as f32;
        let lon = track.get("lon")?.as_f64()? as f32;

        Some(BlePosition {
            latitude: lat,
            longitude: lon,
            altitude: track.get("hae").and_then(|v| v.as_f64()).map(|a| a as f32),
            accuracy: track.get("cep").and_then(|v| v.as_f64()).map(|a| a as f32),
        })
    }

    // =========================================================================
    // Peripheral <-> Platform Translation
    // =========================================================================

    /// Convert BLE peripheral to platform document JSON
    ///
    /// Note: This version does not set cell_id. Use `peripheral_to_platform_in_cell`
    /// to include cell membership based on BLE mesh_id.
    pub fn peripheral_to_platform(&self, peripheral: &BlePeripheral) -> Value {
        self.peripheral_to_platform_in_cell(peripheral, None)
    }

    /// Convert BLE peripheral to platform document JSON with cell membership
    ///
    /// The `mesh_id` parameter (from BLE mesh configuration) is used as the cell_id,
    /// allowing BLE peripherals to be associated with PEAT cells.
    ///
    /// # Arguments
    /// * `peripheral` - The BLE peripheral data
    /// * `mesh_id` - Optional BLE mesh ID to use as cell_id
    pub fn peripheral_to_platform_in_cell(
        &self,
        peripheral: &BlePeripheral,
        mesh_id: Option<&str>,
    ) -> Value {
        let platform_id = format!("{}{:08X}", self.config.ble_id_prefix, peripheral.id);

        let mut platform = json!({
            "id": platform_id,
            "name": peripheral.callsign,
            "type": match peripheral.peripheral_type {
                BlePeripheralType::SoldierSensor => "wearable",
                BlePeripheralType::FixedSensor => "sensor",
                BlePeripheralType::Relay => "relay",
                BlePeripheralType::Unknown => "unknown",
            },
            "status": if peripheral.health.battery_percent > 20 { "active" } else { "low_battery" },
            "battery_percent": peripheral.health.battery_percent,
            "activity": match peripheral.health.activity {
                0 => "still",
                1 => "walking",
                2 => "running",
                3 => "vehicle",
                _ => "unknown",
            },
            "last_update": peripheral.timestamp,
            "ble_origin": true,
            "parent_node": format!("{:08X}", peripheral.parent_node),
        });

        // Set cell_id from BLE mesh_id (mesh_id == cell_id mapping)
        if let Some(cell_id) = mesh_id {
            platform["cell_id"] = json!(cell_id);
        }

        // Add optional health data
        if let Some(hr) = peripheral.health.heart_rate {
            platform["heart_rate"] = json!(hr);
        }

        // Add position if available
        if let Some(ref pos) = peripheral.position {
            platform["lat"] = json!(pos.latitude as f64);
            platform["lon"] = json!(pos.longitude as f64);
            if let Some(alt) = pos.altitude {
                platform["hae"] = json!(alt as f64);
            }
        }

        // Add alerts
        if peripheral.health.alerts != 0 {
            let mut alerts = Vec::new();
            if peripheral.health.is_man_down() {
                alerts.push("man_down");
            }
            if peripheral.health.is_low_battery() {
                alerts.push("low_battery");
            }
            if peripheral.health.alerts & BleHealthStatus::ALERT_OUT_OF_RANGE != 0 {
                alerts.push("out_of_range");
            }
            platform["alerts"] = json!(alerts);
        }

        platform
    }

    /// Extract BLE peripheral data from platform document JSON
    pub fn platform_to_peripheral(&self, platform: &Value) -> Option<BlePeripheral> {
        let id_str = platform.get("id")?.as_str()?;
        let id = self.parse_ble_id(id_str)?;

        let peripheral_type = match platform.get("type").and_then(|v| v.as_str()) {
            Some("wearable") => BlePeripheralType::SoldierSensor,
            Some("sensor") => BlePeripheralType::FixedSensor,
            Some("relay") => BlePeripheralType::Relay,
            _ => BlePeripheralType::Unknown,
        };

        let activity = match platform.get("activity").and_then(|v| v.as_str()) {
            Some("walking") => 1,
            Some("running") => 2,
            Some("vehicle") => 3,
            _ => 0,
        };

        let mut alerts: u8 = 0;
        if let Some(alert_arr) = platform.get("alerts").and_then(|v| v.as_array()) {
            for alert in alert_arr {
                if let Some(s) = alert.as_str() {
                    match s {
                        "man_down" => alerts |= BleHealthStatus::ALERT_MAN_DOWN,
                        "low_battery" => alerts |= BleHealthStatus::ALERT_LOW_BATTERY,
                        "out_of_range" => alerts |= BleHealthStatus::ALERT_OUT_OF_RANGE,
                        _ => {}
                    }
                }
            }
        }

        let position = if platform.get("lat").is_some() && platform.get("lon").is_some() {
            Some(BlePosition {
                latitude: platform.get("lat")?.as_f64()? as f32,
                longitude: platform.get("lon")?.as_f64()? as f32,
                altitude: platform
                    .get("hae")
                    .and_then(|v| v.as_f64())
                    .map(|a| a as f32),
                accuracy: platform
                    .get("cep")
                    .and_then(|v| v.as_f64())
                    .map(|a| a as f32),
            })
        } else {
            None
        };

        Some(BlePeripheral {
            id,
            parent_node: self
                .parse_ble_id(
                    platform
                        .get("parent_node")
                        .and_then(|v| v.as_str())
                        .unwrap_or("0"),
                )
                .unwrap_or(0),
            peripheral_type,
            callsign: platform
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            health: BleHealthStatus {
                battery_percent: platform
                    .get("battery_percent")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(100) as u8,
                heart_rate: platform
                    .get("heart_rate")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u8),
                activity,
                alerts,
            },
            timestamp: platform
                .get("last_update")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            position,
        })
    }

    // =========================================================================
    // Emergency <-> Alert Translation
    // =========================================================================

    /// Convert BLE emergency event to alert document JSON
    pub fn emergency_to_alert(
        &self,
        emergency: &BleEmergencyEvent,
        callsign: Option<&str>,
    ) -> Value {
        let alert_id = format!(
            "{}emergency-{:08X}-{}",
            self.config.ble_id_prefix, emergency.source_node, emergency.timestamp
        );

        let default_source = format!("{:08X}", emergency.source_node);
        let source = callsign.unwrap_or(&default_source);

        // Convert acks to JSON-friendly format
        let acks: HashMap<String, bool> = emergency
            .acks
            .iter()
            .map(|(k, v)| (format!("{:08X}", k), *v))
            .collect();

        json!({
            "id": alert_id,
            "type": "emergency",
            "source": source,
            "source_node": format!("{:08X}", emergency.source_node),
            "timestamp": emergency.timestamp,
            "acks": acks,
            "ack_count": emergency.acks.values().filter(|&&v| v).count(),
            "total_peers": emergency.acks.len(),
            "active": true,
            "ble_origin": true
        })
    }

    /// Extract BLE emergency from alert document JSON
    pub fn alert_to_emergency(&self, alert: &Value) -> Option<BleEmergencyEvent> {
        // Only process emergency type alerts
        if alert.get("type").and_then(|v| v.as_str()) != Some("emergency") {
            return None;
        }

        let source_node_str = alert.get("source_node")?.as_str()?;
        let source_node = u32::from_str_radix(source_node_str.trim_start_matches("0x"), 16).ok()?;

        let timestamp = alert.get("timestamp")?.as_u64()?;

        let acks: HashMap<u32, bool> = alert
            .get("acks")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| {
                        let node_id = u32::from_str_radix(k.trim_start_matches("0x"), 16).ok()?;
                        let acked = v.as_bool()?;
                        Some((node_id, acked))
                    })
                    .collect()
            })
            .unwrap_or_default();

        Some(BleEmergencyEvent {
            source_node,
            timestamp,
            acks,
        })
    }

    // =========================================================================
    // CannedMessage <-> Document Translation
    // =========================================================================

    /// Convert BLE canned message to Automerge document JSON
    ///
    /// Convenience wrapper that does not set cell_id.
    /// Use `canned_message_to_doc_in_cell` to include cell membership.
    pub fn canned_message_to_doc(
        &self,
        message: &BleCannedMessage,
        callsign: Option<&str>,
    ) -> Value {
        self.canned_message_to_doc_in_cell(message, callsign, None)
    }

    /// Convert BLE canned message to Automerge document JSON with cell membership
    ///
    /// The `mesh_id` parameter (from BLE mesh configuration) is used as the cell_id,
    /// allowing BLE-originated canned messages to be associated with PEAT cells.
    ///
    /// # Arguments
    /// * `message` - The BLE canned message data
    /// * `callsign` - Optional callsign for the source
    /// * `mesh_id` - Optional BLE mesh ID to use as cell_id
    pub fn canned_message_to_doc_in_cell(
        &self,
        message: &BleCannedMessage,
        callsign: Option<&str>,
        mesh_id: Option<&str>,
    ) -> Value {
        let doc_id = format!(
            "{}canned-{:08X}-{}",
            self.config.ble_id_prefix, message.source_node, message.timestamp
        );

        let default_source = format!("{:08X}", message.source_node);
        let source = callsign.unwrap_or(&default_source);

        // Convert acks to JSON-friendly format (hex node IDs)
        let acks: HashMap<String, u64> = message
            .acks
            .iter()
            .map(|(k, v)| (format!("{:08X}", k), *v))
            .collect();

        let mut doc = json!({
            "id": doc_id,
            "type": "canned_message",
            "message_code": message.message_code,
            "message_name": message.message_name,
            "source": source,
            "source_node": format!("{:08X}", message.source_node),
            "target_node": message.target_node.map(|n| format!("{:08X}", n)),
            "timestamp": message.timestamp,
            "sequence": message.sequence,
            "acks": acks,
            "ack_count": message.acks.len(),
            "ble_origin": true
        });

        if let Some(cell_id) = mesh_id {
            doc["cell_id"] = json!(cell_id);
        }

        doc
    }

    /// Extract BLE canned message from document JSON
    ///
    /// Returns None if the document doesn't have `type: "canned_message"`.
    pub fn doc_to_canned_message(&self, doc: &Value) -> Option<BleCannedMessage> {
        if doc.get("type").and_then(|v| v.as_str()) != Some("canned_message") {
            return None;
        }

        let message_code = doc.get("message_code")?.as_u64()? as u8;

        let source_node_str = doc.get("source_node")?.as_str()?;
        let source_node = u32::from_str_radix(source_node_str.trim_start_matches("0x"), 16).ok()?;

        let target_node = doc
            .get("target_node")
            .and_then(|v| v.as_str())
            .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok());

        let timestamp = doc.get("timestamp")?.as_u64()?;
        let sequence = doc.get("sequence").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

        let acks: HashMap<u32, u64> = doc
            .get("acks")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| {
                        let node_id = u32::from_str_radix(k.trim_start_matches("0x"), 16).ok()?;
                        let ts = v.as_u64()?;
                        Some((node_id, ts))
                    })
                    .collect()
            })
            .unwrap_or_default();

        Some(BleCannedMessage {
            message_code,
            message_name: message_name_from_code(message_code).to_string(),
            source_node,
            target_node,
            timestamp,
            sequence,
            acks,
        })
    }

    // =========================================================================
    // Utility Methods
    // =========================================================================

    /// Parse a BLE ID from hex string (with or without prefix)
    fn parse_ble_id(&self, id: &str) -> Option<u32> {
        let hex_part = id
            .strip_prefix(&self.config.ble_id_prefix)
            .unwrap_or(id)
            .trim_start_matches("0x")
            .trim_start_matches("0X");
        u32::from_str_radix(hex_part, 16).ok()
    }

    /// Check if a document ID originated from BLE
    pub fn is_ble_origin(&self, doc_id: &str) -> bool {
        doc_id.starts_with(&self.config.ble_id_prefix)
    }

    /// Check if a document has BLE origin marker
    pub fn has_ble_marker(&self, doc: &Value) -> bool {
        doc.get("ble_origin")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Get the collection name for tracks
    pub fn tracks_collection(&self) -> &str {
        &self.config.tracks_collection
    }

    /// Get the collection name for platforms
    pub fn platforms_collection(&self) -> &str {
        &self.config.platforms_collection
    }

    /// Get the collection name for alerts
    pub fn alerts_collection(&self) -> &str {
        &self.config.alerts_collection
    }

    /// Get the collection name for canned messages
    pub fn canned_messages_collection(&self) -> &str {
        &self.config.canned_messages_collection
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_translator() -> BleTranslator {
        BleTranslator::with_defaults()
    }

    #[test]
    fn test_position_to_track_roundtrip() {
        let translator = test_translator();

        let original = BlePosition {
            latitude: 33.7490,
            longitude: -84.3880,
            altitude: Some(320.0),
            accuracy: Some(5.0),
        };

        let track = translator.position_to_track(&original, 0x12345678, Some("ALPHA-1"));
        let recovered = translator.track_to_position(&track).unwrap();

        assert!((recovered.latitude - original.latitude).abs() < 0.0001);
        assert!((recovered.longitude - original.longitude).abs() < 0.0001);
        assert!((recovered.altitude.unwrap() - original.altitude.unwrap()).abs() < 0.1);
        assert!((recovered.accuracy.unwrap() - original.accuracy.unwrap()).abs() < 0.1);
    }

    #[test]
    fn test_position_to_track_fields() {
        let translator = test_translator();

        let position = BlePosition {
            latitude: 33.7490,
            longitude: -84.3880,
            altitude: None,
            accuracy: None,
        };

        let track = translator.position_to_track(&position, 0xABCDEF12, Some("BRAVO-2"));

        assert_eq!(track["id"], "ble-ABCDEF12");
        assert_eq!(track["callsign"], "BRAVO-2");
        assert_eq!(track["ble_origin"], true);
        assert!(track["hae"].is_null());
    }

    #[test]
    fn test_peripheral_to_platform_roundtrip() {
        let translator = test_translator();

        let original = BlePeripheral {
            id: 0x11223344,
            parent_node: 0xAABBCCDD,
            peripheral_type: BlePeripheralType::SoldierSensor,
            callsign: "CHARLIE-3".to_string(),
            health: BleHealthStatus {
                battery_percent: 75,
                heart_rate: Some(72),
                activity: 1, // walking
                alerts: BleHealthStatus::ALERT_LOW_BATTERY,
            },
            timestamp: 1700000000000,
            position: Some(BlePosition {
                latitude: 34.0,
                longitude: -85.0,
                altitude: None,
                accuracy: None,
            }),
        };

        let platform = translator.peripheral_to_platform(&original);
        let recovered = translator.platform_to_peripheral(&platform).unwrap();

        assert_eq!(recovered.id, original.id);
        assert_eq!(recovered.callsign, original.callsign);
        assert_eq!(recovered.peripheral_type, original.peripheral_type);
        assert_eq!(
            recovered.health.battery_percent,
            original.health.battery_percent
        );
        assert_eq!(recovered.health.heart_rate, original.health.heart_rate);
        assert_eq!(recovered.health.activity, original.health.activity);
        assert!(recovered.health.is_low_battery());
    }

    #[test]
    fn test_emergency_to_alert_roundtrip() {
        let translator = test_translator();

        let mut acks = HashMap::new();
        acks.insert(0x11111111, true);
        acks.insert(0x22222222, false);
        acks.insert(0x33333333, true);

        let original = BleEmergencyEvent {
            source_node: 0xDEADBEEF,
            timestamp: 1700000000000,
            acks,
        };

        let alert = translator.emergency_to_alert(&original, Some("DELTA-4"));
        let recovered = translator.alert_to_emergency(&alert).unwrap();

        assert_eq!(recovered.source_node, original.source_node);
        assert_eq!(recovered.timestamp, original.timestamp);
        assert_eq!(recovered.acks.len(), original.acks.len());
        assert_eq!(recovered.acks.get(&0x11111111), Some(&true));
        assert_eq!(recovered.acks.get(&0x22222222), Some(&false));
    }

    #[test]
    fn test_is_ble_origin() {
        let translator = test_translator();

        assert!(translator.is_ble_origin("ble-12345678"));
        assert!(!translator.is_ble_origin("track-12345678"));
        assert!(!translator.is_ble_origin("12345678"));
    }

    #[test]
    fn test_health_status_alerts() {
        let health = BleHealthStatus {
            battery_percent: 15,
            heart_rate: None,
            activity: 0,
            alerts: BleHealthStatus::ALERT_MAN_DOWN | BleHealthStatus::ALERT_LOW_BATTERY,
        };

        assert!(health.is_man_down());
        assert!(health.is_low_battery());
    }

    #[test]
    fn test_parse_ble_id() {
        let translator = test_translator();

        assert_eq!(translator.parse_ble_id("ble-12345678"), Some(0x12345678));
        assert_eq!(translator.parse_ble_id("12345678"), Some(0x12345678));
        assert_eq!(translator.parse_ble_id("0x12345678"), Some(0x12345678));
        assert_eq!(translator.parse_ble_id("ABCDEF00"), Some(0xABCDEF00));
        assert_eq!(translator.parse_ble_id("not_hex"), None);
    }

    #[test]
    fn test_position_to_track_with_cell_id() {
        let translator = test_translator();

        let position = BlePosition {
            latitude: 33.7490,
            longitude: -84.3880,
            altitude: None,
            accuracy: None,
        };

        // Without mesh_id - no cell_id
        let track = translator.position_to_track(&position, 0xAABBCCDD, Some("ALPHA-1"));
        assert!(track.get("cell_id").is_none());

        // With mesh_id - cell_id set
        let track = translator.position_to_track_in_cell(
            &position,
            0xAABBCCDD,
            Some("ALPHA-1"),
            Some("SQUAD-A"),
        );
        assert_eq!(track["cell_id"], "SQUAD-A");
    }

    #[test]
    fn test_peripheral_to_platform_with_cell_id() {
        let translator = test_translator();

        let peripheral = BlePeripheral {
            id: 0x11223344,
            parent_node: 0xAABBCCDD,
            peripheral_type: BlePeripheralType::SoldierSensor,
            callsign: "BRAVO-2".to_string(),
            health: BleHealthStatus {
                battery_percent: 85,
                heart_rate: None,
                activity: 0,
                alerts: 0,
            },
            timestamp: 1700000000000,
            position: None,
        };

        // Without mesh_id - no cell_id
        let platform = translator.peripheral_to_platform(&peripheral);
        assert!(platform.get("cell_id").is_none());

        // With mesh_id - cell_id set (mesh_id == cell_id mapping)
        let platform = translator.peripheral_to_platform_in_cell(&peripheral, Some("ALPHA-SQUAD"));
        assert_eq!(platform["cell_id"], "ALPHA-SQUAD");
        assert_eq!(platform["ble_origin"], true);
    }

    // =========================================================================
    // CannedMessage Translation Tests
    // =========================================================================

    fn test_canned_message() -> BleCannedMessage {
        let mut acks = HashMap::new();
        acks.insert(0xDEADBEEF, 1706234567000u64); // source auto-ack
        acks.insert(0x11111111, 1706234568000u64); // peer ack

        BleCannedMessage {
            message_code: 0x20, // Emergency
            message_name: "Emergency".to_string(),
            source_node: 0xDEADBEEF,
            target_node: None,
            timestamp: 1706234567000,
            sequence: 42,
            acks,
        }
    }

    #[test]
    fn test_canned_message_to_doc_roundtrip() {
        let translator = test_translator();
        let original = test_canned_message();

        let doc = translator.canned_message_to_doc(&original, Some("ALPHA-1"));
        let recovered = translator.doc_to_canned_message(&doc).unwrap();

        assert_eq!(recovered.message_code, original.message_code);
        assert_eq!(recovered.message_name, "Emergency");
        assert_eq!(recovered.source_node, original.source_node);
        assert_eq!(recovered.target_node, original.target_node);
        assert_eq!(recovered.timestamp, original.timestamp);
        assert_eq!(recovered.sequence, original.sequence);
        assert_eq!(recovered.acks.len(), original.acks.len());
        assert_eq!(recovered.acks.get(&0x11111111), Some(&1706234568000u64));
    }

    #[test]
    fn test_canned_message_fields() {
        let translator = test_translator();
        let msg = test_canned_message();

        let doc = translator.canned_message_to_doc(&msg, Some("ALPHA-1"));

        assert_eq!(doc["id"], "ble-canned-DEADBEEF-1706234567000");
        assert_eq!(doc["type"], "canned_message");
        assert_eq!(doc["message_code"], 0x20);
        assert_eq!(doc["message_name"], "Emergency");
        assert_eq!(doc["source"], "ALPHA-1");
        assert_eq!(doc["source_node"], "DEADBEEF");
        assert!(doc["target_node"].is_null());
        assert_eq!(doc["timestamp"], 1706234567000u64);
        assert_eq!(doc["sequence"], 42);
        assert_eq!(doc["ack_count"], 2);
        assert_eq!(doc["ble_origin"], true);
    }

    #[test]
    fn test_canned_message_ack_map() {
        let translator = test_translator();
        let msg = test_canned_message();

        let doc = translator.canned_message_to_doc(&msg, None);

        // Verify hex-encoded ack keys survive the roundtrip
        let acks_obj = doc["acks"].as_object().unwrap();
        assert!(acks_obj.contains_key("DEADBEEF"));
        assert!(acks_obj.contains_key("11111111"));
        assert_eq!(acks_obj["11111111"], 1706234568000u64);

        let recovered = translator.doc_to_canned_message(&doc).unwrap();
        assert_eq!(recovered.acks.get(&0xDEADBEEF), Some(&1706234567000u64));
        assert_eq!(recovered.acks.get(&0x11111111), Some(&1706234568000u64));
    }

    #[test]
    fn test_canned_message_no_target() {
        let translator = test_translator();
        let msg = test_canned_message(); // target_node = None

        let doc = translator.canned_message_to_doc(&msg, None);
        assert!(doc["target_node"].is_null());

        let recovered = translator.doc_to_canned_message(&doc).unwrap();
        assert_eq!(recovered.target_node, None);

        // Now test with a target
        let mut directed = msg;
        directed.target_node = Some(0xAABBCCDD);
        let doc = translator.canned_message_to_doc(&directed, None);
        assert_eq!(doc["target_node"], "AABBCCDD");

        let recovered = translator.doc_to_canned_message(&doc).unwrap();
        assert_eq!(recovered.target_node, Some(0xAABBCCDD));
    }

    #[test]
    fn test_canned_message_with_cell_id() {
        let translator = test_translator();
        let msg = test_canned_message();

        // Without mesh_id - no cell_id
        let doc = translator.canned_message_to_doc(&msg, None);
        assert!(doc.get("cell_id").is_none());

        // With mesh_id - cell_id set
        let doc = translator.canned_message_to_doc_in_cell(&msg, None, Some("SQUAD-A"));
        assert_eq!(doc["cell_id"], "SQUAD-A");
    }

    #[test]
    fn test_canned_message_name_from_code() {
        // Acknowledgments
        assert_eq!(message_name_from_code(0x00), "Ack");
        assert_eq!(message_name_from_code(0x01), "Ack Wilco");
        assert_eq!(message_name_from_code(0x02), "Ack Negative");
        assert_eq!(message_name_from_code(0x03), "Say Again");
        // Status
        assert_eq!(message_name_from_code(0x10), "Check In");
        assert_eq!(message_name_from_code(0x11), "Moving");
        assert_eq!(message_name_from_code(0x12), "Holding");
        assert_eq!(message_name_from_code(0x13), "On Station");
        assert_eq!(message_name_from_code(0x14), "Returning");
        assert_eq!(message_name_from_code(0x15), "Complete");
        // Alerts
        assert_eq!(message_name_from_code(0x20), "Emergency");
        assert_eq!(message_name_from_code(0x21), "Alert");
        assert_eq!(message_name_from_code(0x22), "All Clear");
        assert_eq!(message_name_from_code(0x23), "Contact");
        assert_eq!(message_name_from_code(0x24), "Under Fire");
        // Requests
        assert_eq!(message_name_from_code(0x30), "Need Extract");
        assert_eq!(message_name_from_code(0x31), "Need Support");
        assert_eq!(message_name_from_code(0x32), "Need Medic");
        assert_eq!(message_name_from_code(0x33), "Need Resupply");
        // Reserved
        assert_eq!(message_name_from_code(0xFF), "Custom");
        // Unknown fallback
        assert_eq!(message_name_from_code(0x99), "Unknown");
        assert_eq!(message_name_from_code(0x04), "Unknown");
    }

    #[test]
    fn test_canned_message_wrong_type() {
        let translator = test_translator();

        // Emergency alert doc should NOT parse as canned message
        let emergency = BleEmergencyEvent {
            source_node: 0xDEADBEEF,
            timestamp: 1700000000000,
            acks: HashMap::new(),
        };
        let alert_doc = translator.emergency_to_alert(&emergency, Some("ALPHA-1"));
        assert!(translator.doc_to_canned_message(&alert_doc).is_none());

        // Doc with no type field
        let no_type = json!({"message_code": 0x10, "source_node": "DEADBEEF"});
        assert!(translator.doc_to_canned_message(&no_type).is_none());

        // Doc with wrong type
        let wrong_type = json!({"type": "track", "message_code": 0x10});
        assert!(translator.doc_to_canned_message(&wrong_type).is_none());
    }

    #[test]
    fn test_canned_message_accessor() {
        let translator = test_translator();
        assert_eq!(translator.canned_messages_collection(), "canned_messages");

        let custom = BleTranslator::new(TranslationConfig {
            canned_messages_collection: "my_messages".to_string(),
            ..TranslationConfig::default()
        });
        assert_eq!(custom.canned_messages_collection(), "my_messages");
    }
}
