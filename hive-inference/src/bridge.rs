//! Bridge module - HIVE-TAK Bridge for CoT translation
//!
//! Translates between HIVE messages and Cursor on Target (CoT) XML.

use serde::{Deserialize, Serialize};

/// Placeholder for CoT (Cursor on Target) message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CotMessage {
    /// CoT event UID
    pub uid: String,
    /// CoT event type (e.g., "a-f-G-U-C" for friendly ground unit)
    pub cot_type: String,
    /// Latitude
    pub lat: f64,
    /// Longitude
    pub lon: f64,
}

/// Bridge for HIVE-TAK translation
pub struct HiveTakBridge {
    // TODO: Add TAK server connection details
}

impl HiveTakBridge {
    /// Create a new HIVE-TAK bridge
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for HiveTakBridge {
    fn default() -> Self {
        Self::new()
    }
}
