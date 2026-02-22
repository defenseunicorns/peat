//! Message encoding/decoding errors.

/// Errors that can occur when encoding or decoding a HIVE-Lite message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageError {
    /// Output buffer is too small for the encoded message.
    BufferTooSmall,
    /// Input buffer is shorter than the minimum header size.
    TooShort,
    /// Magic bytes do not match.
    InvalidMagic,
    /// Protocol version is not supported.
    UnsupportedVersion,
    /// Message type byte is not recognised.
    InvalidMessageType,
    /// Payload exceeds `MAX_PAYLOAD_SIZE`.
    PayloadTooLarge,
}
