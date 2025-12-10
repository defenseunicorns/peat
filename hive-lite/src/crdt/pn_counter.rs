//! Positive-Negative Counter (PN-Counter)
//!
//! A CRDT counter that supports both increment and decrement operations.
//! Implemented as two G-Counters: one for increments, one for decrements.

use super::g_counter::GCounter;
use super::{CrdtError, LiteCrdt};

/// Positive-Negative Counter
///
/// Supports both increment and decrement by using two internal G-Counters.
/// The value is computed as: increments - decrements
#[derive(Debug, Clone)]
pub struct PnCounter {
    increments: GCounter,
    decrements: GCounter,
}

impl PnCounter {
    /// Create a new PN-Counter for the given node
    pub fn new(local_node_id: u32) -> Self {
        Self {
            increments: GCounter::new(local_node_id),
            decrements: GCounter::new(local_node_id),
        }
    }

    /// Increment the counter by 1
    pub fn increment(&mut self) {
        self.increments.increment();
    }

    /// Increment the counter by a specific amount
    pub fn increment_by(&mut self, amount: u64) {
        self.increments.increment_by(amount);
    }

    /// Decrement the counter by 1
    pub fn decrement(&mut self) {
        self.decrements.increment();
    }

    /// Decrement the counter by a specific amount
    pub fn decrement_by(&mut self, amount: u64) {
        self.decrements.increment_by(amount);
    }

    /// Get the current value (can be negative)
    pub fn value(&self) -> i64 {
        self.increments.count() as i64 - self.decrements.count() as i64
    }

    /// Get total increments
    pub fn total_increments(&self) -> u64 {
        self.increments.count()
    }

    /// Get total decrements
    pub fn total_decrements(&self) -> u64 {
        self.decrements.count()
    }
}

impl Default for PnCounter {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Operation for PN-Counter
#[derive(Debug, Clone)]
pub enum PnCounterOp {
    Increment(u64),
    Decrement(u64),
}

impl LiteCrdt for PnCounter {
    type Op = PnCounterOp;
    type Value = i64;

    fn apply(&mut self, op: &Self::Op) {
        match op {
            PnCounterOp::Increment(amount) => self.increment_by(*amount),
            PnCounterOp::Decrement(amount) => self.decrement_by(*amount),
        }
    }

    fn merge(&mut self, other: &Self) {
        self.increments.merge(&other.increments);
        self.decrements.merge(&other.decrements);
    }

    fn value(&self) -> Self::Value {
        PnCounter::value(self)
    }

    fn encode(&self, buf: &mut [u8]) -> Result<usize, CrdtError> {
        // Encode increments, then decrements
        let inc_len = self.increments.encode(buf)?;
        let dec_len = self.decrements.encode(&mut buf[inc_len..])?;
        Ok(inc_len + dec_len)
    }

    fn decode(buf: &[u8]) -> Result<Self, CrdtError> {
        // We need to figure out where increments ends and decrements begins
        // The GCounter encoding starts with local_node_id:4, num_entries:2
        // Then num_entries * 12 bytes
        if buf.len() < 6 {
            return Err(CrdtError::InvalidData);
        }

        let inc_num_entries = u16::from_le_bytes(buf[4..6].try_into().unwrap()) as usize;
        let inc_len = 6 + (inc_num_entries * 12);

        if buf.len() < inc_len + 6 {
            return Err(CrdtError::InvalidData);
        }

        let increments = GCounter::decode(&buf[..inc_len])?;
        let decrements = GCounter::decode(&buf[inc_len..])?;

        Ok(Self {
            increments,
            decrements,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pncounter_increment_decrement() {
        let mut counter = PnCounter::new(1);
        assert_eq!(counter.value(), 0);

        counter.increment_by(10);
        assert_eq!(counter.value(), 10);

        counter.decrement_by(3);
        assert_eq!(counter.value(), 7);
    }

    #[test]
    fn test_pncounter_negative() {
        let mut counter = PnCounter::new(1);
        counter.decrement_by(5);
        assert_eq!(counter.value(), -5);
    }

    #[test]
    fn test_pncounter_merge() {
        let mut counter1 = PnCounter::new(1);
        counter1.increment_by(10);
        counter1.decrement_by(2);

        let mut counter2 = PnCounter::new(2);
        counter2.increment_by(5);
        counter2.decrement_by(1);

        counter1.merge(&counter2);
        // Total: (10 + 5) - (2 + 1) = 12
        assert_eq!(counter1.value(), 12);
    }

    #[test]
    fn test_pncounter_encode_decode() {
        let mut counter = PnCounter::new(42);
        counter.increment_by(100);
        counter.decrement_by(30);

        let mut buf = [0u8; 512];
        let len = counter.encode(&mut buf).unwrap();

        let decoded = PnCounter::decode(&buf[..len]).unwrap();
        assert_eq!(decoded.value(), 70);
    }
}
