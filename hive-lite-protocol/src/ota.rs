//! OTA (Over-The-Air) wire protocol constants.
//!
//! Chunk sizes, offer format sizes, flags, result codes, and abort reason
//! codes shared between OTA senders (hive-mesh) and receivers (hive-lite).

/// Maximum data bytes in a single OTA chunk (496 - 6 bytes framing).
pub const OTA_CHUNK_DATA_SIZE: usize = 448;

/// OTA offer payload size — legacy format, without signature.
pub const OTA_OFFER_SIZE: usize = 76;

/// OTA offer v2 payload size — includes Ed25519 signature.
pub const OTA_OFFER_V2_SIZE: usize = 140;

/// OTA offer flag: Ed25519 signature is present.
pub const OTA_FLAG_SIGNED: u16 = 0x0001;

// ---------------------------------------------------------------------------
// Result codes  (Lite -> Full, carried in OtaResult payload)
// ---------------------------------------------------------------------------

pub const OTA_RESULT_SUCCESS: u8 = 0x00;
pub const OTA_RESULT_HASH_MISMATCH: u8 = 0x01;
pub const OTA_RESULT_FLASH_ERROR: u8 = 0x02;
pub const OTA_RESULT_INVALID_OFFER: u8 = 0x03;
pub const OTA_RESULT_SIGNATURE_INVALID: u8 = 0x04;
pub const OTA_RESULT_SIGNATURE_REQUIRED: u8 = 0x05;

// ---------------------------------------------------------------------------
// Abort reason codes  (either direction, carried in OtaAbort payload)
// ---------------------------------------------------------------------------

pub const OTA_ABORT_TIMEOUT: u8 = 0x01;
pub const OTA_ABORT_SESSION_MISMATCH: u8 = 0x02;
pub const OTA_ABORT_USER_CANCEL: u8 = 0x03;
/// Too many retries without progress (sender-side).
pub const OTA_ABORT_TOO_MANY_RETRIES: u8 = 0x04;
