//! Last-Writer-Wins Register
//!
//! A simple CRDT where the value with the highest timestamp wins.
//! Ideal for sensor readings where we always want the latest value.

use super::{CrdtError, LiteCrdt};

/// Last-Writer-Wins Register for storing sensor values
///
/// The register stores a value along with a timestamp and node ID.
/// When merging, the value with the higher timestamp wins.
/// If timestamps are equal, the higher node ID wins (tie-breaker).
#[derive(Debug, Clone)]
pub struct LwwRegister<T: Clone + Default, const MAX_SIZE: usize = 64> {
    value: T,
    timestamp: u64,
    node_id: u32,
}

impl<T: Clone + Default, const MAX_SIZE: usize> Default for LwwRegister<T, MAX_SIZE> {
    fn default() -> Self {
        Self {
            value: T::default(),
            timestamp: 0,
            node_id: 0,
        }
    }
}

impl<T: Clone + Default, const MAX_SIZE: usize> LwwRegister<T, MAX_SIZE> {
    /// Create a new LWW register with initial value
    pub fn new(value: T, timestamp: u64, node_id: u32) -> Self {
        Self {
            value,
            timestamp,
            node_id,
        }
    }

    /// Set a new value with the given timestamp
    pub fn set(&mut self, value: T, timestamp: u64, node_id: u32) {
        if timestamp > self.timestamp || (timestamp == self.timestamp && node_id > self.node_id) {
            self.value = value;
            self.timestamp = timestamp;
            self.node_id = node_id;
        }
    }

    /// Get the current value
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get the timestamp of the current value
    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Get the node ID that set the current value
    pub fn node_id(&self) -> u32 {
        self.node_id
    }
}

/// Operation for LWW Register
#[derive(Debug, Clone)]
pub struct LwwOp<T: Clone> {
    pub value: T,
    pub timestamp: u64,
    pub node_id: u32,
}

impl<T: Clone + Default, const MAX_SIZE: usize> LiteCrdt for LwwRegister<T, MAX_SIZE>
where
    T: LwwEncodable,
{
    type Op = LwwOp<T>;
    type Value = T;

    fn apply(&mut self, op: &Self::Op) {
        self.set(op.value.clone(), op.timestamp, op.node_id);
    }

    fn merge(&mut self, other: &Self) {
        if other.timestamp > self.timestamp
            || (other.timestamp == self.timestamp && other.node_id > self.node_id)
        {
            self.value = other.value.clone();
            self.timestamp = other.timestamp;
            self.node_id = other.node_id;
        }
    }

    fn value(&self) -> Self::Value {
        self.value.clone()
    }

    fn encode(&self, buf: &mut [u8]) -> Result<usize, CrdtError> {
        // Format: [timestamp:8][node_id:4][value_len:2][value:N]
        let value_bytes = self.value.to_bytes();
        let total_len = 8 + 4 + 2 + value_bytes.len();

        if buf.len() < total_len {
            return Err(CrdtError::BufferTooSmall);
        }

        buf[0..8].copy_from_slice(&self.timestamp.to_le_bytes());
        buf[8..12].copy_from_slice(&self.node_id.to_le_bytes());
        buf[12..14].copy_from_slice(&(value_bytes.len() as u16).to_le_bytes());
        buf[14..14 + value_bytes.len()].copy_from_slice(&value_bytes);

        Ok(total_len)
    }

    fn decode(buf: &[u8]) -> Result<Self, CrdtError> {
        if buf.len() < 14 {
            return Err(CrdtError::InvalidData);
        }

        let timestamp = u64::from_le_bytes(buf[0..8].try_into().unwrap());
        let node_id = u32::from_le_bytes(buf[8..12].try_into().unwrap());
        let value_len = u16::from_le_bytes(buf[12..14].try_into().unwrap()) as usize;

        if buf.len() < 14 + value_len {
            return Err(CrdtError::InvalidData);
        }

        let value = T::from_bytes(&buf[14..14 + value_len]).ok_or(CrdtError::InvalidData)?;

        Ok(Self {
            value,
            timestamp,
            node_id,
        })
    }
}

/// Trait for types that can be encoded/decoded in LWW registers
pub trait LwwEncodable: Clone + Default {
    fn to_bytes(&self) -> heapless::Vec<u8, 64>;
    fn from_bytes(bytes: &[u8]) -> Option<Self>;
}

// Implement for common types
impl LwwEncodable for i32 {
    fn to_bytes(&self) -> heapless::Vec<u8, 64> {
        let mut v = heapless::Vec::new();
        v.extend_from_slice(&self.to_le_bytes()).ok();
        v
    }

    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() >= 4 {
            Some(i32::from_le_bytes(bytes[0..4].try_into().ok()?))
        } else {
            None
        }
    }
}

impl LwwEncodable for u32 {
    fn to_bytes(&self) -> heapless::Vec<u8, 64> {
        let mut v = heapless::Vec::new();
        v.extend_from_slice(&self.to_le_bytes()).ok();
        v
    }

    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() >= 4 {
            Some(u32::from_le_bytes(bytes[0..4].try_into().ok()?))
        } else {
            None
        }
    }
}

impl LwwEncodable for u8 {
    fn to_bytes(&self) -> heapless::Vec<u8, 64> {
        let mut v = heapless::Vec::new();
        v.push(*self).ok();
        v
    }

    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        bytes.first().copied()
    }
}

impl LwwEncodable for bool {
    fn to_bytes(&self) -> heapless::Vec<u8, 64> {
        let mut v = heapless::Vec::new();
        v.push(if *self { 1 } else { 0 }).ok();
        v
    }

    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        bytes.first().map(|&b| b != 0)
    }
}

/// Sensor reading with scaled integer (avoids floating point)
/// Value is in centiunits (e.g., 2350 = 23.50 degrees)
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SensorValue {
    pub value: i32,
    pub sensor_type: u8,
    pub quality: u8,
}

impl LwwEncodable for SensorValue {
    fn to_bytes(&self) -> heapless::Vec<u8, 64> {
        let mut v = heapless::Vec::new();
        v.extend_from_slice(&self.value.to_le_bytes()).ok();
        v.push(self.sensor_type).ok();
        v.push(self.quality).ok();
        v
    }

    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() >= 6 {
            Some(Self {
                value: i32::from_le_bytes(bytes[0..4].try_into().ok()?),
                sensor_type: bytes[4],
                quality: bytes[5],
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lww_set_newer() {
        let mut reg: LwwRegister<i32> = LwwRegister::new(10, 100, 1);
        reg.set(20, 200, 2);
        assert_eq!(*reg.get(), 20);
        assert_eq!(reg.timestamp(), 200);
    }

    #[test]
    fn test_lww_ignore_older() {
        let mut reg: LwwRegister<i32> = LwwRegister::new(10, 200, 1);
        reg.set(20, 100, 2); // Older timestamp
        assert_eq!(*reg.get(), 10); // Value unchanged
    }

    #[test]
    fn test_lww_merge() {
        let mut reg1: LwwRegister<i32> = LwwRegister::new(10, 100, 1);
        let reg2: LwwRegister<i32> = LwwRegister::new(20, 200, 2);
        reg1.merge(&reg2);
        assert_eq!(*reg1.get(), 20);
    }

    #[test]
    fn test_lww_encode_decode() {
        let reg: LwwRegister<i32> = LwwRegister::new(42, 12345, 99);
        let mut buf = [0u8; 64];
        let len = reg.encode(&mut buf).unwrap();

        let decoded: LwwRegister<i32> = LwwRegister::decode(&buf[..len]).unwrap();
        assert_eq!(*decoded.get(), 42);
        assert_eq!(decoded.timestamp(), 12345);
        assert_eq!(decoded.node_id(), 99);
    }
}
