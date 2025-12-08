//! Time utilities for HIVE simulation.
//!
//! Provides functions for timestamp handling, including extraction from various
//! formats (plain integers, protobuf-style objects) and current time retrieval.

use std::time::{SystemTime, UNIX_EPOCH};

/// Get current time as microseconds since UNIX epoch.
pub fn now_micros() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_micros()
}

/// Extract a timestamp in microseconds from a serde_json::Value.
///
/// Handles multiple timestamp formats:
/// - Direct numeric values (u64, i64, f64)
/// - Protobuf-style objects with `{seconds, nanos}` fields
///
/// Returns 0 if the value cannot be parsed as a timestamp.
///
/// # Examples
///
/// ```ignore
/// // Direct u64 value
/// let val = serde_json::json!(1234567890123456u64);
/// assert_eq!(extract_timestamp_us(&val), 1234567890123456);
///
/// // Protobuf-style {seconds, nanos}
/// let val = serde_json::json!({"seconds": 1234567890, "nanos": 123456789});
/// assert_eq!(extract_timestamp_us(&val), 1234567890123456);
/// ```
pub fn extract_timestamp_us(val: &serde_json::Value) -> u128 {
    // Try direct numeric formats first
    if let Some(n) = val.as_u64() {
        return n as u128;
    }
    if let Some(n) = val.as_i64() {
        return n as u128;
    }
    if let Some(n) = val.as_f64() {
        return n as u128;
    }

    // Try protobuf-style {seconds, nanos} object
    if let Some(obj) = val.as_object() {
        let seconds = obj
            .get("seconds")
            .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|n| n as u64)))
            .unwrap_or(0);
        let nanos = obj
            .get("nanos")
            .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|n| n as u64)))
            .unwrap_or(0);
        // Convert to microseconds: seconds * 1_000_000 + nanos / 1_000
        return (seconds as u128 * 1_000_000) + (nanos as u128 / 1_000);
    }

    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_now_micros_is_reasonable() {
        let now = now_micros();
        // Should be after year 2020 (in microseconds)
        assert!(now > 1_577_836_800_000_000);
    }

    #[test]
    fn test_extract_timestamp_u64() {
        let val = json!(1234567890123456u64);
        assert_eq!(extract_timestamp_us(&val), 1234567890123456);
    }

    #[test]
    fn test_extract_timestamp_i64() {
        let val = json!(1234567890123456i64);
        assert_eq!(extract_timestamp_us(&val), 1234567890123456);
    }

    #[test]
    fn test_extract_timestamp_f64() {
        let val = json!(1234567890123456.0);
        assert_eq!(extract_timestamp_us(&val), 1234567890123456);
    }

    #[test]
    fn test_extract_timestamp_protobuf_style() {
        // 1234567890 seconds + 123456789 nanos = 1234567890123456 microseconds
        let val = json!({"seconds": 1234567890, "nanos": 123456789});
        assert_eq!(extract_timestamp_us(&val), 1234567890123456);
    }

    #[test]
    fn test_extract_timestamp_protobuf_style_only_seconds() {
        let val = json!({"seconds": 1234567890});
        assert_eq!(extract_timestamp_us(&val), 1234567890000000);
    }

    #[test]
    fn test_extract_timestamp_invalid_returns_zero() {
        let val = json!("not a timestamp");
        assert_eq!(extract_timestamp_us(&val), 0);

        let val = json!(null);
        assert_eq!(extract_timestamp_us(&val), 0);

        let val = json!([1, 2, 3]);
        assert_eq!(extract_timestamp_us(&val), 0);
    }
}
