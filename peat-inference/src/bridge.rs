//! Bridge module - Peat-TAK Bridge for CoT translation
//!
//! Translates between Peat messages and Cursor on Target (CoT) XML.

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

/// Bridge for Peat-TAK translation
pub struct PeatTakBridge {
    // TODO: Add TAK server connection details
}

impl PeatTakBridge {
    /// Create a new Peat-TAK bridge
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for PeatTakBridge {
    fn default() -> Self {
        Self::new()
    }
}
