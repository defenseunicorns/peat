//! CRDT type identifiers for Data messages.

/// CRDT type identifiers carried in the first byte of a `Data` payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CrdtType {
    LwwRegister = 0x01,
    GCounter = 0x02,
    PnCounter = 0x03,
    OrSet = 0x04,
}

impl CrdtType {
    /// Convert a raw byte to a `CrdtType`, if valid.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(Self::LwwRegister),
            0x02 => Some(Self::GCounter),
            0x03 => Some(Self::PnCounter),
            0x04 => Some(Self::OrSet),
            _ => None,
        }
    }
}
