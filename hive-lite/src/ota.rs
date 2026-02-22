//! OTA (Over-The-Air) firmware update receiver for HIVE-Lite.
//!
//! Implements a state machine that receives firmware images from a HIVE Full node
//! over the existing UDP transport using the OTA wire protocol (0x10-0x16).
//!
//! # Architecture
//!
//! - Stop-and-wait: one chunk at a time, ACK before next
//! - Streaming SHA256: hash verified before committing
//! - Ed25519 signature verification: ensures firmware comes from trusted source
//! - A/B partitions: writes to inactive partition, updates otadata to boot it
//! - Validation record: tracks boot attempts for automatic rollback on crash

// Build-time generated: OTA_SIGNING_PUBKEY
include!(concat!(env!("OUT_DIR"), "/ota_pubkey.rs"));

// Wire-protocol OTA constants from the shared crate.
pub use hive_lite_protocol::ota::{
    OTA_CHUNK_DATA_SIZE, OTA_FLAG_SIGNED, OTA_OFFER_SIZE, OTA_OFFER_V2_SIZE,
};

/// Flash offset for ota_0 partition
pub const OTA_0_OFFSET: u32 = 0x10000;

/// Flash offset for ota_1 partition
pub const OTA_1_OFFSET: u32 = 0x310000;

/// Size of each OTA partition (3 MB)
pub const OTA_PARTITION_SIZE: u32 = 0x300000;

/// Flash offset for otadata partition
pub const OTADATA_OFFSET: u32 = 0xE000;

/// Size of otadata partition
pub const OTADATA_SIZE: u32 = 0x2000;

/// Flash offset for OTA validation record (first NVS sector, unused by bare-metal)
pub const VALIDATION_OFFSET: u32 = 0x9000;

/// Magic value for validation record: "OTVA"
pub const VALIDATION_MAGIC: u32 = 0x4F545641;

// OTA result and abort codes from the shared crate.
pub use hive_lite_protocol::ota::{
    OTA_ABORT_SESSION_MISMATCH, OTA_ABORT_TIMEOUT, OTA_ABORT_TOO_MANY_RETRIES,
    OTA_ABORT_USER_CANCEL, OTA_RESULT_FLASH_ERROR, OTA_RESULT_HASH_MISMATCH,
    OTA_RESULT_INVALID_OFFER, OTA_RESULT_SIGNATURE_INVALID, OTA_RESULT_SIGNATURE_REQUIRED,
    OTA_RESULT_SUCCESS,
};

/// OTA receiver states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtaState {
    /// Waiting for an OTA offer
    Idle,
    /// Received offer, sent accept, waiting for first data chunk
    WaitingForData,
    /// Actively receiving chunks
    Receiving,
    /// All chunks received, verifying hash
    Verifying,
    /// Update committed, ready to reboot
    ReadyToReboot,
    /// OTA failed
    Failed,
}

/// Parsed OTA offer from a Full node
#[derive(Debug, Clone)]
pub struct OtaOffer {
    pub version: [u8; 16],
    pub firmware_size: u32,
    pub total_chunks: u16,
    pub chunk_size: u16,
    pub sha256: [u8; 32],
    pub session_id: u16,
    pub flags: u16,
    /// Ed25519 signature over the SHA256 digest (present when flags & OTA_FLAG_SIGNED)
    pub signature: Option<[u8; 64]>,
}

impl OtaOffer {
    /// Parse an OTA offer from the message payload.
    /// Supports both legacy (76 bytes) and v2 (140 bytes) formats.
    pub fn from_payload(payload: &[u8]) -> Option<Self> {
        if payload.len() < OTA_OFFER_SIZE {
            return None;
        }

        let mut version = [0u8; 16];
        version.copy_from_slice(&payload[0..16]);

        let firmware_size = u32::from_le_bytes(payload[16..20].try_into().ok()?);
        let total_chunks = u16::from_le_bytes(payload[20..22].try_into().ok()?);
        let chunk_size = u16::from_le_bytes(payload[22..24].try_into().ok()?);

        let mut sha256 = [0u8; 32];
        sha256.copy_from_slice(&payload[24..56]);

        let session_id = u16::from_le_bytes(payload[56..58].try_into().ok()?);
        let flags = u16::from_le_bytes(payload[58..60].try_into().ok()?);

        // Parse signature if SIGNED flag is set and payload is large enough
        let signature = if (flags & OTA_FLAG_SIGNED) != 0 && payload.len() >= OTA_OFFER_V2_SIZE {
            let mut sig = [0u8; 64];
            sig.copy_from_slice(&payload[60..124]);
            Some(sig)
        } else {
            None
        };

        Some(Self {
            version,
            firmware_size,
            total_chunks,
            chunk_size,
            sha256,
            session_id,
            flags,
            signature,
        })
    }

    /// Get version as a string (trimmed of null bytes)
    pub fn version_str(&self) -> &str {
        let end = self.version.iter().position(|&b| b == 0).unwrap_or(16);
        core::str::from_utf8(&self.version[..end]).unwrap_or("???")
    }
}

/// OTA error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtaError {
    /// SHA256 hash of received firmware doesn't match offer
    HashMismatch,
    /// Flash write operation failed
    FlashWriteFailed,
    /// Offer is invalid (bad size, too many chunks, etc.)
    InvalidOffer,
    /// Session ID doesn't match active session
    SessionMismatch,
    /// Firmware too large for partition
    FirmwareTooLarge,
    /// Not in correct state for this operation
    InvalidState,
    /// Ed25519 signature verification failed
    SignatureInvalid,
    /// Signature required but offer was unsigned
    SignatureRequired,
}

// =============================================================================
// Signature Verification
// =============================================================================

/// Verify an Ed25519 signature over the SHA256 digest using the compiled-in pubkey.
///
/// Returns Ok(()) if:
/// - No pubkey is configured (verification disabled)
/// - Pubkey is configured and signature is valid
///
/// Returns Err if:
/// - Pubkey is configured but offer is unsigned → SignatureRequired
/// - Pubkey is configured and signature is invalid → SignatureInvalid
#[cfg(feature = "ota")]
fn verify_offer_signature(offer: &OtaOffer) -> Result<(), OtaError> {
    let pubkey_bytes = match OTA_SIGNING_PUBKEY {
        Some(bytes) => bytes,
        None => {
            // No pubkey configured — skip verification (dev builds)
            return Ok(());
        }
    };

    let signature_bytes = match &offer.signature {
        Some(sig) => sig,
        None => {
            esp_println::println!("[OTA] Signature REQUIRED but offer is unsigned");
            return Err(OtaError::SignatureRequired);
        }
    };

    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    let verifying_key = VerifyingKey::from_bytes(&pubkey_bytes)
        .map_err(|_| {
            esp_println::println!("[OTA] Invalid compiled-in public key");
            OtaError::SignatureInvalid
        })?;

    let signature = Signature::from_bytes(signature_bytes);

    verifying_key
        .verify(&offer.sha256, &signature)
        .map_err(|_| {
            esp_println::println!("[OTA] Signature verification FAILED");
            OtaError::SignatureInvalid
        })?;

    esp_println::println!("[OTA] Signature verified OK");
    Ok(())
}

// Host-side stub for tests (no esp_println, no flash)
#[cfg(not(feature = "ota"))]
fn verify_offer_signature(offer: &OtaOffer) -> Result<(), OtaError> {
    match OTA_SIGNING_PUBKEY {
        Some(_) => {
            if offer.signature.is_none() {
                return Err(OtaError::SignatureRequired);
            }
            // Can't verify without ed25519-dalek on host (feature gated)
            Ok(())
        }
        None => Ok(()),
    }
}

// =============================================================================
// OTA Validation Record (boot rollback protection)
// =============================================================================

/// Validation state for boot rollback tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ValidationState {
    Idle = 0,
    Pending = 1,
    Validated = 2,
}

/// OTA validation record stored in flash at VALIDATION_OFFSET.
/// Written BEFORE otadata to ensure safe rollback on power loss.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct OtaValidationRecord {
    pub magic: u32,
    pub state: u8,
    pub boot_attempts: u8,
    pub max_attempts: u8,
    pub reserved: u8,
    pub previous_partition_index: u32,
    pub previous_seq: u32,
    pub new_partition_index: u32,
    pub new_seq: u32,
    pub crc: u32,
    pub pad: u32,
}

impl OtaValidationRecord {
    /// Compute CRC32 of bytes 0..24 (everything before crc field)
    fn compute_crc(&self) -> u32 {
        let mut buf = [0u8; 24];
        buf[0..4].copy_from_slice(&self.magic.to_le_bytes());
        buf[4] = self.state;
        buf[5] = self.boot_attempts;
        buf[6] = self.max_attempts;
        buf[7] = self.reserved;
        buf[8..12].copy_from_slice(&self.previous_partition_index.to_le_bytes());
        buf[12..16].copy_from_slice(&self.previous_seq.to_le_bytes());
        buf[16..20].copy_from_slice(&self.new_partition_index.to_le_bytes());
        buf[20..24].copy_from_slice(&self.new_seq.to_le_bytes());
        crc32(&buf)
    }

    /// Serialize to 32 bytes
    fn to_bytes(&self) -> [u8; 32] {
        let mut buf = [0u8; 32];
        buf[0..4].copy_from_slice(&self.magic.to_le_bytes());
        buf[4] = self.state;
        buf[5] = self.boot_attempts;
        buf[6] = self.max_attempts;
        buf[7] = self.reserved;
        buf[8..12].copy_from_slice(&self.previous_partition_index.to_le_bytes());
        buf[12..16].copy_from_slice(&self.previous_seq.to_le_bytes());
        buf[16..20].copy_from_slice(&self.new_partition_index.to_le_bytes());
        buf[20..24].copy_from_slice(&self.new_seq.to_le_bytes());
        buf[24..28].copy_from_slice(&self.crc.to_le_bytes());
        buf[28..32].copy_from_slice(&self.pad.to_le_bytes());
        buf
    }

    /// Deserialize from 32 bytes
    fn from_bytes(buf: &[u8; 32]) -> Option<Self> {
        let magic = u32::from_le_bytes(buf[0..4].try_into().ok()?);
        if magic != VALIDATION_MAGIC {
            return None;
        }
        let rec = Self {
            magic,
            state: buf[4],
            boot_attempts: buf[5],
            max_attempts: buf[6],
            reserved: buf[7],
            previous_partition_index: u32::from_le_bytes(buf[8..12].try_into().ok()?),
            previous_seq: u32::from_le_bytes(buf[12..16].try_into().ok()?),
            new_partition_index: u32::from_le_bytes(buf[16..20].try_into().ok()?),
            new_seq: u32::from_le_bytes(buf[20..24].try_into().ok()?),
            crc: u32::from_le_bytes(buf[24..28].try_into().ok()?),
            pad: u32::from_le_bytes(buf[28..32].try_into().ok()?),
        };
        // Validate CRC
        if rec.crc != rec.compute_crc() {
            return None;
        }
        Some(rec)
    }
}

/// Read the validation record from flash.
#[cfg(feature = "ota")]
pub fn read_validation_record() -> Option<OtaValidationRecord> {
    use embedded_storage::ReadStorage;
    use esp_storage::FlashStorage;
    let mut flash = FlashStorage::new();
    let mut buf = [0u8; 32];
    if flash.read(VALIDATION_OFFSET, &mut buf).is_ok() {
        OtaValidationRecord::from_bytes(&buf)
    } else {
        None
    }
}

/// Write a validation record to flash (erases the 4KB sector first).
#[cfg(feature = "ota")]
pub fn write_validation_record(record: &OtaValidationRecord) -> bool {
    use esp_storage::FlashStorage;
    let mut flash = FlashStorage::new();
    let sector_end = VALIDATION_OFFSET + 4096;

    // Erase the sector
    if embedded_storage::nor_flash::NorFlash::erase(&mut flash, VALIDATION_OFFSET, sector_end)
        .is_err()
    {
        esp_println::println!("[OTA] Failed to erase validation sector");
        return false;
    }

    // Write the record
    let bytes = record.to_bytes();
    if embedded_storage::Storage::write(&mut flash, VALIDATION_OFFSET, &bytes).is_err() {
        esp_println::println!("[OTA] Failed to write validation record");
        return false;
    }
    true
}

/// Write a PENDING validation record before updating otadata.
/// Captures the current partition state so we can rollback if needed.
#[cfg(feature = "ota")]
pub fn write_pending_validation(
    prev_part_index: u32,
    prev_seq: u32,
    new_part_index: u32,
    new_seq: u32,
) -> bool {
    let mut record = OtaValidationRecord {
        magic: VALIDATION_MAGIC,
        state: ValidationState::Pending as u8,
        boot_attempts: 0,
        max_attempts: 3,
        reserved: 0,
        previous_partition_index: prev_part_index,
        previous_seq: prev_seq,
        new_partition_index: new_part_index,
        new_seq: new_seq,
        crc: 0,
        pad: 0,
    };
    record.crc = record.compute_crc();
    write_validation_record(&record)
}

/// Early-boot validation check. Call immediately after heap/hal init.
///
/// If a PENDING validation record exists:
/// - Increments boot_attempts
/// - If boot_attempts > max_attempts: rolls back to previous partition and reboots
/// - Otherwise: re-writes the record with incremented count
#[cfg(feature = "ota")]
pub fn boot_validation_check() {
    let record = match read_validation_record() {
        Some(r) => r,
        None => return, // No record or invalid CRC — nothing to do
    };

    if record.state != ValidationState::Pending as u8 {
        return; // IDLE or VALIDATED — nothing to do
    }

    let attempts = record.boot_attempts + 1;
    esp_println::println!(
        "[OTA] Boot validation: attempt {}/{}",
        attempts,
        record.max_attempts
    );

    if attempts > record.max_attempts {
        // Too many failed boots — rollback
        esp_println::println!("[OTA] Max boot attempts exceeded! Rolling back...");
        rollback_to_previous(&record);
        // rollback_to_previous reboots — we should not reach here
        return;
    }

    // Increment boot_attempts and re-write
    let mut updated = record;
    updated.boot_attempts = attempts;
    updated.crc = updated.compute_crc();
    write_validation_record(&updated);
}

/// Mark the current firmware as validated (boot succeeded).
/// Sets the validation record to IDLE state.
#[cfg(feature = "ota")]
pub fn ota_mark_validated() {
    let record = match read_validation_record() {
        Some(r) => r,
        None => return,
    };

    if record.state != ValidationState::Pending as u8 {
        return;
    }

    esp_println::println!("[OTA] Firmware validated! Clearing pending record.");
    let mut updated = record;
    updated.state = ValidationState::Idle as u8;
    updated.boot_attempts = 0;
    updated.crc = updated.compute_crc();
    write_validation_record(&updated);
}

/// Rollback: rewrite otadata to point to the previous partition, clear record, reboot.
#[cfg(feature = "ota")]
fn rollback_to_previous(record: &OtaValidationRecord) {
    use embedded_storage::Storage;
    use esp_storage::FlashStorage;
    let mut flash = FlashStorage::new();

    // Build an otadata entry that points to the previous partition
    // with a sequence number higher than both current entries
    let mut entry0 = [0u8; 32];
    let mut entry1 = [0u8; 32];
    let _ = embedded_storage::ReadStorage::read(&mut flash, OTADATA_OFFSET, &mut entry0);
    let _ = embedded_storage::ReadStorage::read(&mut flash, OTADATA_OFFSET + 32, &mut entry1);

    let seq0 = u32::from_le_bytes(entry0[0..4].try_into().unwrap_or([0; 4]));
    let seq1 = u32::from_le_bytes(entry1[0..4].try_into().unwrap_or([0; 4]));
    let rollback_seq = seq0.max(seq1).wrapping_add(1);

    // Build new otadata entry pointing to previous partition
    let mut entry = [0xFFu8; 32];
    entry[0..4].copy_from_slice(&rollback_seq.to_le_bytes());
    entry[4..8].copy_from_slice(&record.previous_partition_index.to_le_bytes());
    let crc = crc32(&entry[0..28]);
    entry[28..32].copy_from_slice(&crc.to_le_bytes());

    // Determine which slot to write
    let slot_offset = if seq0 <= seq1 {
        OTADATA_OFFSET
    } else {
        OTADATA_OFFSET + 32
    };

    // Erase otadata
    if embedded_storage::nor_flash::NorFlash::erase(
        &mut flash,
        OTADATA_OFFSET,
        OTADATA_OFFSET + OTADATA_SIZE,
    )
    .is_err()
    {
        esp_println::println!("[OTA] Rollback: failed to erase otadata");
        esp_hal::system::software_reset();
    }

    // Write both entries
    if slot_offset == OTADATA_OFFSET {
        let _ = Storage::write(&mut flash, OTADATA_OFFSET, &entry);
        if seq1 != 0 || entry1.iter().any(|&b| b != 0xFF) {
            let _ = Storage::write(&mut flash, OTADATA_OFFSET + 32, &entry1);
        }
    } else {
        if seq0 != 0 || entry0.iter().any(|&b| b != 0xFF) {
            let _ = Storage::write(&mut flash, OTADATA_OFFSET, &entry0);
        }
        let _ = Storage::write(&mut flash, OTADATA_OFFSET + 32, &entry);
    }

    esp_println::println!(
        "[OTA] Rolled back to partition ota_{} (seq={})",
        record.previous_partition_index,
        rollback_seq
    );

    // Clear the validation record to IDLE
    let mut cleared = *record;
    cleared.state = ValidationState::Idle as u8;
    cleared.boot_attempts = 0;
    cleared.crc = cleared.compute_crc();
    write_validation_record(&cleared);

    // Reboot into the previous partition
    esp_hal::system::software_reset();
}

// =============================================================================
// OTA Receiver State Machine
// =============================================================================

/// OTA receiver state machine
///
/// Manages the entire OTA update lifecycle on the Lite device:
/// 1. Parses incoming OtaOffer and verifies Ed25519 signature
/// 2. Writes firmware chunks to the inactive flash partition
/// 3. Verifies SHA256 hash
/// 4. Writes pending validation record (BEFORE otadata)
/// 5. Updates otadata to mark the new partition as bootable
pub struct OtaReceiver {
    pub state: OtaState,
    pub session_id: u16,
    pub offer: Option<OtaOffer>,
    pub chunks_received: u16,
    pub bytes_written: u32,
    /// Target flash partition offset (ota_0 or ota_1)
    pub target_offset: u32,
    #[cfg(feature = "ota")]
    hasher: sha2::Sha256,
}

impl OtaReceiver {
    /// Create a new OTA receiver in Idle state
    pub fn new() -> Self {
        Self {
            state: OtaState::Idle,
            session_id: 0,
            offer: None,
            chunks_received: 0,
            bytes_written: 0,
            target_offset: OTA_1_OFFSET, // Default: write to ota_1
            #[cfg(feature = "ota")]
            hasher: {
                use sha2::Digest;
                sha2::Sha256::new()
            },
        }
    }

    /// Handle an incoming OTA offer. Returns Ok(session_id) to ACK.
    pub fn handle_offer(&mut self, payload: &[u8]) -> Result<u16, OtaError> {
        let offer = OtaOffer::from_payload(payload).ok_or(OtaError::InvalidOffer)?;

        // Validate offer
        if offer.firmware_size == 0 || offer.total_chunks == 0 {
            return Err(OtaError::InvalidOffer);
        }
        if offer.firmware_size > OTA_PARTITION_SIZE {
            return Err(OtaError::FirmwareTooLarge);
        }
        if offer.chunk_size as usize > OTA_CHUNK_DATA_SIZE {
            return Err(OtaError::InvalidOffer);
        }

        // Verify Ed25519 signature
        verify_offer_signature(&offer)?;

        // Determine target partition: read otadata to find inactive slot
        self.target_offset = self.determine_target_partition();

        // Erase target partition before writing
        #[cfg(feature = "ota")]
        {
            if !self.erase_partition(self.target_offset, offer.firmware_size) {
                return Err(OtaError::FlashWriteFailed);
            }
        }

        self.session_id = offer.session_id;
        self.chunks_received = 0;
        self.bytes_written = 0;
        self.offer = Some(offer);
        self.state = OtaState::WaitingForData;

        // Reset hasher
        #[cfg(feature = "ota")]
        {
            use sha2::Digest;
            self.hasher = sha2::Sha256::new();
        }

        esp_println::println!(
            "[OTA] Accepted offer: session={}, size={}, chunks={}, signed={}",
            self.session_id,
            self.offer.as_ref().unwrap().firmware_size,
            self.offer.as_ref().unwrap().total_chunks,
            self.offer.as_ref().unwrap().signature.is_some()
        );

        Ok(self.session_id)
    }

    /// Handle an incoming OTA data chunk.
    /// Returns Ok(chunk_num) to ACK, or Err on failure.
    pub fn handle_data(&mut self, payload: &[u8]) -> Result<u16, OtaError> {
        if self.state != OtaState::WaitingForData && self.state != OtaState::Receiving {
            return Err(OtaError::InvalidState);
        }

        if payload.len() < 6 {
            return Err(OtaError::InvalidOffer);
        }

        let session_id = u16::from_le_bytes(payload[0..2].try_into().unwrap());
        let chunk_num = u16::from_le_bytes(payload[2..4].try_into().unwrap());
        let chunk_len = u16::from_le_bytes(payload[4..6].try_into().unwrap()) as usize;

        if session_id != self.session_id {
            return Err(OtaError::SessionMismatch);
        }

        // Handle duplicate (already ACK'd this chunk)
        if chunk_num < self.chunks_received {
            return Ok(chunk_num);
        }

        if payload.len() < 6 + chunk_len {
            return Err(OtaError::InvalidOffer);
        }

        let chunk_data = &payload[6..6 + chunk_len];

        // Write chunk to flash
        #[cfg(feature = "ota")]
        {
            let flash_addr = self.target_offset + self.bytes_written;
            if !self.write_flash(flash_addr, chunk_data) {
                self.state = OtaState::Failed;
                return Err(OtaError::FlashWriteFailed);
            }
        }

        // Update streaming hash
        #[cfg(feature = "ota")]
        {
            use sha2::Digest;
            self.hasher.update(chunk_data);
        }

        self.bytes_written += chunk_len as u32;
        self.chunks_received = chunk_num + 1;
        self.state = OtaState::Receiving;

        Ok(chunk_num)
    }

    /// Handle OTA complete message. Verifies hash, writes validation record,
    /// and updates otadata for reboot.
    /// Returns result code for OtaResult message.
    pub fn handle_complete(&mut self, payload: &[u8]) -> u8 {
        if self.state != OtaState::Receiving {
            return OTA_RESULT_FLASH_ERROR;
        }

        if payload.len() < 2 {
            return OTA_RESULT_INVALID_OFFER;
        }

        let session_id = u16::from_le_bytes(payload[0..2].try_into().unwrap());
        if session_id != self.session_id {
            return OTA_RESULT_INVALID_OFFER;
        }

        self.state = OtaState::Verifying;

        // Verify SHA256
        #[cfg(feature = "ota")]
        {
            use sha2::Digest;
            let hash = self.hasher.clone().finalize();
            if let Some(offer) = &self.offer {
                if hash.as_slice() != offer.sha256 {
                    esp_println::println!("[OTA] Hash mismatch!");
                    esp_println::println!("[OTA]   expected: {:02x?}", &offer.sha256[..8]);
                    esp_println::println!("[OTA]   got:      {:02x?}", &hash.as_slice()[..8]);
                    self.state = OtaState::Failed;
                    return OTA_RESULT_HASH_MISMATCH;
                }
            }
        }

        // Read current otadata to capture previous state for rollback
        #[cfg(feature = "ota")]
        {
            use embedded_storage::ReadStorage;
            use esp_storage::FlashStorage;
            let mut flash = FlashStorage::new();
            let mut entry0 = [0u8; 32];
            let mut entry1 = [0u8; 32];
            let _ = flash.read(OTADATA_OFFSET, &mut entry0);
            let _ = flash.read(OTADATA_OFFSET + 32, &mut entry1);

            let seq0 = u32::from_le_bytes(entry0[0..4].try_into().unwrap_or([0; 4]));
            let seq1 = u32::from_le_bytes(entry1[0..4].try_into().unwrap_or([0; 4]));

            // Current active partition (the one with higher seq)
            let (prev_part_index, prev_seq) = if seq1 >= seq0 {
                // ota_1 is active (index=1)
                let idx = u32::from_le_bytes(entry1[4..8].try_into().unwrap_or([1; 4]));
                (idx, seq1)
            } else {
                let idx = u32::from_le_bytes(entry0[4..8].try_into().unwrap_or([0; 4]));
                (idx, seq0)
            };

            let new_part_index: u32 = if self.target_offset == OTA_0_OFFSET {
                0
            } else {
                1
            };
            let new_seq = seq0.max(seq1).wrapping_add(1);

            // Write validation record BEFORE updating otadata (critical ordering)
            if !write_pending_validation(prev_part_index, prev_seq, new_part_index, new_seq) {
                esp_println::println!("[OTA] Failed to write validation record");
                self.state = OtaState::Failed;
                return OTA_RESULT_FLASH_ERROR;
            }
        }

        // Update otadata to boot from new partition
        #[cfg(feature = "ota")]
        {
            if !self.set_boot_partition(self.target_offset) {
                self.state = OtaState::Failed;
                return OTA_RESULT_FLASH_ERROR;
            }
        }

        self.state = OtaState::ReadyToReboot;
        esp_println::println!("[OTA] Update verified and committed! Ready to reboot.");
        OTA_RESULT_SUCCESS
    }

    /// Handle OTA abort from sender
    pub fn handle_abort(&mut self, payload: &[u8]) {
        if payload.len() >= 2 {
            let session_id = u16::from_le_bytes(payload[0..2].try_into().unwrap());
            if session_id == self.session_id || self.state == OtaState::Idle {
                let reason = if payload.len() >= 3 { payload[2] } else { 0 };
                esp_println::println!("[OTA] Abort received: reason={}", reason);
                self.reset();
            }
        }
    }

    /// Reset receiver to idle state
    pub fn reset(&mut self) {
        self.state = OtaState::Idle;
        self.session_id = 0;
        self.offer = None;
        self.chunks_received = 0;
        self.bytes_written = 0;
        #[cfg(feature = "ota")]
        {
            use sha2::Digest;
            self.hasher = sha2::Sha256::new();
        }
    }

    /// Get progress as percentage (0-100)
    pub fn progress_percent(&self) -> u8 {
        if let Some(offer) = &self.offer {
            if offer.total_chunks > 0 {
                ((self.chunks_received as u32 * 100) / offer.total_chunks as u32) as u8
            } else {
                0
            }
        } else {
            0
        }
    }

    /// Determine which partition to write to.
    /// Reads otadata to find the currently active partition and returns the other one.
    fn determine_target_partition(&self) -> u32 {
        #[cfg(feature = "ota")]
        {
            use embedded_storage::ReadStorage;
            use esp_storage::FlashStorage;
            let mut flash = FlashStorage::new();
            let mut buf = [0u8; 32];

            if flash.read(OTADATA_OFFSET, &mut buf).is_ok() {
                let seq0 = u32::from_le_bytes(buf[0..4].try_into().unwrap_or([0; 4]));
                let mut buf2 = [0u8; 32];
                if flash.read(OTADATA_OFFSET + 32, &mut buf2).is_ok() {
                    let seq1 = u32::from_le_bytes(buf2[0..4].try_into().unwrap_or([0; 4]));
                    if seq1 >= seq0 {
                        return OTA_0_OFFSET;
                    } else {
                        return OTA_1_OFFSET;
                    }
                }
            }
        }
        OTA_1_OFFSET
    }

    /// Erase flash sectors covering the firmware
    #[cfg(feature = "ota")]
    fn erase_partition(&self, offset: u32, size: u32) -> bool {
        use esp_storage::FlashStorage;
        let mut flash = FlashStorage::new();

        let sector_size: u32 = 4096;
        let erase_end = offset + ((size + sector_size - 1) / sector_size) * sector_size;

        esp_println::println!("[OTA] Erasing 0x{:08X}..0x{:08X}", offset, erase_end);

        if embedded_storage::nor_flash::NorFlash::erase(&mut flash, offset, erase_end).is_err() {
            esp_println::println!("[OTA] Erase failed");
            return false;
        }
        true
    }

    /// Write data to flash
    #[cfg(feature = "ota")]
    fn write_flash(&self, addr: u32, data: &[u8]) -> bool {
        use embedded_storage::Storage;
        use esp_storage::FlashStorage;
        let mut flash = FlashStorage::new();

        if flash.write(addr, data).is_err() {
            esp_println::println!("[OTA] Write failed at 0x{:08X}", addr);
            return false;
        }
        true
    }

    /// Update otadata partition to mark the new partition as bootable.
    #[cfg(feature = "ota")]
    fn set_boot_partition(&self, partition_offset: u32) -> bool {
        use embedded_storage::{ReadStorage, Storage};
        use esp_storage::FlashStorage;
        let mut flash = FlashStorage::new();

        let mut entry0 = [0u8; 32];
        let mut entry1 = [0u8; 32];
        let _ = flash.read(OTADATA_OFFSET, &mut entry0);
        let _ = flash.read(OTADATA_OFFSET + 32, &mut entry1);

        let seq0 = u32::from_le_bytes(entry0[0..4].try_into().unwrap_or([0; 4]));
        let seq1 = u32::from_le_bytes(entry1[0..4].try_into().unwrap_or([0; 4]));
        let new_seq = seq0.max(seq1).wrapping_add(1);

        let slot_offset = if seq0 <= seq1 {
            OTADATA_OFFSET
        } else {
            OTADATA_OFFSET + 32
        };

        let mut entry = [0xFFu8; 32];
        entry[0..4].copy_from_slice(&new_seq.to_le_bytes());

        let part_index: u32 = if partition_offset == OTA_0_OFFSET {
            0
        } else {
            1
        };
        entry[4..8].copy_from_slice(&part_index.to_le_bytes());

        let crc = crc32(&entry[0..28]);
        entry[28..32].copy_from_slice(&crc.to_le_bytes());

        if embedded_storage::nor_flash::NorFlash::erase(
            &mut flash,
            OTADATA_OFFSET,
            OTADATA_OFFSET + OTADATA_SIZE,
        )
        .is_err()
        {
            esp_println::println!("[OTA] Failed to erase otadata");
            return false;
        }

        if slot_offset == OTADATA_OFFSET {
            if Storage::write(&mut flash, OTADATA_OFFSET, &entry).is_err() {
                return false;
            }
            if seq1 != 0 || entry1.iter().any(|&b| b != 0xFF) {
                let _ = Storage::write(&mut flash, OTADATA_OFFSET + 32, &entry1);
            }
        } else {
            if seq0 != 0 || entry0.iter().any(|&b| b != 0xFF) {
                let _ = Storage::write(&mut flash, OTADATA_OFFSET, &entry0);
            }
            if Storage::write(&mut flash, OTADATA_OFFSET + 32, &entry).is_err() {
                return false;
            }
        }

        esp_println::println!(
            "[OTA] otadata updated: seq={}, partition=ota_{}",
            new_seq,
            part_index
        );
        true
    }
}

/// Simple CRC32 implementation (matches ESP-IDF's otadata CRC)
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    crc ^ 0xFFFFFFFF
}

/// Map OtaError to OTA result code for the wire protocol
pub fn ota_error_to_result_code(err: &OtaError) -> u8 {
    match err {
        OtaError::HashMismatch => OTA_RESULT_HASH_MISMATCH,
        OtaError::FlashWriteFailed => OTA_RESULT_FLASH_ERROR,
        OtaError::InvalidOffer => OTA_RESULT_INVALID_OFFER,
        OtaError::SignatureInvalid => OTA_RESULT_SIGNATURE_INVALID,
        OtaError::SignatureRequired => OTA_RESULT_SIGNATURE_REQUIRED,
        _ => OTA_RESULT_INVALID_OFFER,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ota_offer_parse_legacy() {
        let mut payload = [0u8; 76];
        payload[0..5].copy_from_slice(b"0.2.0");
        payload[16..20].copy_from_slice(&1024u32.to_le_bytes());
        payload[20..22].copy_from_slice(&3u16.to_le_bytes());
        payload[22..24].copy_from_slice(&448u16.to_le_bytes());
        payload[24..56].fill(0xAA);
        payload[56..58].copy_from_slice(&42u16.to_le_bytes());
        payload[58..60].copy_from_slice(&0u16.to_le_bytes());

        let offer = OtaOffer::from_payload(&payload).unwrap();
        assert_eq!(offer.version_str(), "0.2.0");
        assert_eq!(offer.firmware_size, 1024);
        assert_eq!(offer.total_chunks, 3);
        assert_eq!(offer.chunk_size, 448);
        assert_eq!(offer.session_id, 42);
        assert_eq!(offer.sha256, [0xAA; 32]);
        assert!(offer.signature.is_none());
    }

    #[test]
    fn test_ota_offer_parse_v2_signed() {
        let mut payload = [0u8; 140];
        payload[0..5].copy_from_slice(b"1.0.0");
        payload[16..20].copy_from_slice(&2048u32.to_le_bytes());
        payload[20..22].copy_from_slice(&5u16.to_le_bytes());
        payload[22..24].copy_from_slice(&448u16.to_le_bytes());
        payload[24..56].fill(0xBB);
        payload[56..58].copy_from_slice(&99u16.to_le_bytes());
        payload[58..60].copy_from_slice(&OTA_FLAG_SIGNED.to_le_bytes());
        // Signature: 64 bytes of 0xCC
        payload[60..124].fill(0xCC);

        let offer = OtaOffer::from_payload(&payload).unwrap();
        assert_eq!(offer.version_str(), "1.0.0");
        assert_eq!(offer.firmware_size, 2048);
        assert_eq!(offer.session_id, 99);
        assert_eq!(offer.flags, OTA_FLAG_SIGNED);
        assert!(offer.signature.is_some());
        assert_eq!(offer.signature.unwrap(), [0xCC; 64]);
    }

    #[test]
    fn test_ota_offer_signed_flag_but_short_payload() {
        // Flags say SIGNED but payload is only 76 bytes (legacy size)
        let mut payload = [0u8; 76];
        payload[58..60].copy_from_slice(&OTA_FLAG_SIGNED.to_le_bytes());
        payload[16..20].copy_from_slice(&100u32.to_le_bytes());
        payload[20..22].copy_from_slice(&1u16.to_le_bytes());
        payload[22..24].copy_from_slice(&100u16.to_le_bytes());

        let offer = OtaOffer::from_payload(&payload).unwrap();
        // Signature should be None since payload is too short
        assert!(offer.signature.is_none());
    }

    #[test]
    fn test_ota_offer_too_short() {
        let payload = [0u8; 50];
        assert!(OtaOffer::from_payload(&payload).is_none());
    }

    #[test]
    fn test_ota_receiver_initial_state() {
        let receiver = OtaReceiver::new();
        assert_eq!(receiver.state, OtaState::Idle);
        assert_eq!(receiver.session_id, 0);
        assert_eq!(receiver.chunks_received, 0);
        assert_eq!(receiver.progress_percent(), 0);
    }

    #[test]
    fn test_ota_receiver_reject_zero_size() {
        let mut receiver = OtaReceiver::new();
        let mut payload = [0u8; 76];
        assert_eq!(receiver.handle_offer(&payload), Err(OtaError::InvalidOffer));

        payload[16..20].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(receiver.handle_offer(&payload), Err(OtaError::InvalidOffer));
    }

    #[test]
    fn test_ota_receiver_reject_oversized() {
        let mut receiver = OtaReceiver::new();
        let mut payload = [0u8; 76];
        payload[16..20].copy_from_slice(&(OTA_PARTITION_SIZE + 1).to_le_bytes());
        payload[20..22].copy_from_slice(&1u16.to_le_bytes());
        payload[22..24].copy_from_slice(&448u16.to_le_bytes());
        assert_eq!(
            receiver.handle_offer(&payload),
            Err(OtaError::FirmwareTooLarge)
        );
    }

    #[test]
    fn test_crc32_known_value() {
        assert_eq!(crc32(&[]), 0x00000000);
        assert_eq!(crc32(b"123456789"), 0xCBF43926);
    }

    #[test]
    fn test_progress_percent() {
        let mut receiver = OtaReceiver::new();
        receiver.offer = Some(OtaOffer {
            version: [0; 16],
            firmware_size: 4480,
            total_chunks: 10,
            chunk_size: 448,
            sha256: [0; 32],
            session_id: 1,
            flags: 0,
            signature: None,
        });
        receiver.chunks_received = 0;
        assert_eq!(receiver.progress_percent(), 0);

        receiver.chunks_received = 5;
        assert_eq!(receiver.progress_percent(), 50);

        receiver.chunks_received = 10;
        assert_eq!(receiver.progress_percent(), 100);
    }

    #[test]
    fn test_reset() {
        let mut receiver = OtaReceiver::new();
        receiver.state = OtaState::Receiving;
        receiver.session_id = 42;
        receiver.chunks_received = 5;
        receiver.bytes_written = 2240;

        receiver.reset();
        assert_eq!(receiver.state, OtaState::Idle);
        assert_eq!(receiver.session_id, 0);
        assert_eq!(receiver.chunks_received, 0);
        assert_eq!(receiver.bytes_written, 0);
    }

    #[test]
    fn test_handle_data_wrong_state() {
        let mut receiver = OtaReceiver::new();
        let payload = [0u8; 10];
        assert_eq!(receiver.handle_data(&payload), Err(OtaError::InvalidState));
    }

    #[test]
    fn test_handle_data_session_mismatch() {
        let mut receiver = OtaReceiver::new();
        receiver.state = OtaState::WaitingForData;
        receiver.session_id = 42;

        let mut payload = [0u8; 10];
        payload[0..2].copy_from_slice(&99u16.to_le_bytes());
        payload[2..4].copy_from_slice(&0u16.to_le_bytes());
        payload[4..6].copy_from_slice(&4u16.to_le_bytes());

        assert_eq!(
            receiver.handle_data(&payload),
            Err(OtaError::SessionMismatch)
        );
    }

    #[test]
    fn test_validation_record_roundtrip() {
        let mut record = OtaValidationRecord {
            magic: VALIDATION_MAGIC,
            state: ValidationState::Pending as u8,
            boot_attempts: 2,
            max_attempts: 3,
            reserved: 0,
            previous_partition_index: 1,
            previous_seq: 5,
            new_partition_index: 0,
            new_seq: 6,
            crc: 0,
            pad: 0,
        };
        record.crc = record.compute_crc();

        let bytes = record.to_bytes();
        let decoded = OtaValidationRecord::from_bytes(&bytes).unwrap();

        assert_eq!(decoded.magic, VALIDATION_MAGIC);
        assert_eq!(decoded.state, ValidationState::Pending as u8);
        assert_eq!(decoded.boot_attempts, 2);
        assert_eq!(decoded.max_attempts, 3);
        assert_eq!(decoded.previous_partition_index, 1);
        assert_eq!(decoded.previous_seq, 5);
        assert_eq!(decoded.new_partition_index, 0);
        assert_eq!(decoded.new_seq, 6);
    }

    #[test]
    fn test_validation_record_bad_magic() {
        let mut buf = [0u8; 32];
        buf[0..4].copy_from_slice(&0xDEADBEEFu32.to_le_bytes());
        assert!(OtaValidationRecord::from_bytes(&buf).is_none());
    }

    #[test]
    fn test_validation_record_bad_crc() {
        let mut record = OtaValidationRecord {
            magic: VALIDATION_MAGIC,
            state: ValidationState::Pending as u8,
            boot_attempts: 0,
            max_attempts: 3,
            reserved: 0,
            previous_partition_index: 0,
            previous_seq: 1,
            new_partition_index: 1,
            new_seq: 2,
            crc: 0,
            pad: 0,
        };
        record.crc = record.compute_crc();

        let mut bytes = record.to_bytes();
        // Corrupt one byte
        bytes[5] = 0xFF;
        assert!(OtaValidationRecord::from_bytes(&bytes).is_none());
    }

    #[test]
    fn test_ota_error_to_result_code() {
        assert_eq!(
            ota_error_to_result_code(&OtaError::SignatureInvalid),
            OTA_RESULT_SIGNATURE_INVALID
        );
        assert_eq!(
            ota_error_to_result_code(&OtaError::SignatureRequired),
            OTA_RESULT_SIGNATURE_REQUIRED
        );
        assert_eq!(
            ota_error_to_result_code(&OtaError::HashMismatch),
            OTA_RESULT_HASH_MISMATCH
        );
    }
}
