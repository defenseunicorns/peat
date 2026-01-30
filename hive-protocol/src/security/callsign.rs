//! Callsign generation for tactical mesh networks.
//!
//! Generates NATO phonetic alphabet callsigns in the format:
//! `{PHONETIC}-{NN}` (e.g., ALPHA-01, BRAVO-42, ZULU-99)
//!
//! Provides 26 × 100 = 2,600 unique callsigns per mesh.

use rand_core::{OsRng, RngCore};
use std::collections::HashSet;

/// NATO phonetic alphabet words.
pub const NATO_ALPHABET: [&str; 26] = [
    "ALPHA", "BRAVO", "CHARLIE", "DELTA", "ECHO", "FOXTROT", "GOLF", "HOTEL", "INDIA", "JULIET",
    "KILO", "LIMA", "MIKE", "NOVEMBER", "OSCAR", "PAPA", "QUEBEC", "ROMEO", "SIERRA", "TANGO",
    "UNIFORM", "VICTOR", "WHISKEY", "XRAY", "YANKEE", "ZULU",
];

/// Maximum callsign length: "NOVEMBER-99" = 11 chars
pub const MAX_CALLSIGN_LENGTH: usize = 11;

/// Total unique callsigns: 26 letters × 100 numbers
pub const TOTAL_CALLSIGNS: usize = 2600;

/// Callsign generator with collision avoidance.
///
/// Tracks used callsigns to ensure uniqueness within a mesh.
#[derive(Debug, Clone, Default)]
pub struct CallsignGenerator {
    used: HashSet<String>,
}

/// Error returned when callsign operations fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallsignError {
    /// Callsign format is invalid
    InvalidFormat(String),
    /// Callsign is already in use
    AlreadyInUse(String),
    /// No more callsigns available (all 2,600 exhausted)
    Exhausted,
}

impl std::fmt::Display for CallsignError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallsignError::InvalidFormat(s) => write!(f, "invalid callsign format: {}", s),
            CallsignError::AlreadyInUse(s) => write!(f, "callsign already in use: {}", s),
            CallsignError::Exhausted => write!(f, "all 2,600 callsigns exhausted"),
        }
    }
}

impl std::error::Error for CallsignError {}

impl CallsignGenerator {
    /// Create a new callsign generator.
    pub fn new() -> Self {
        Self {
            used: HashSet::new(),
        }
    }

    /// Generate a random unused callsign.
    ///
    /// Returns `Err(CallsignError::Exhausted)` if all 2,600 callsigns are in use.
    pub fn generate(&mut self) -> Result<String, CallsignError> {
        if self.used.len() >= TOTAL_CALLSIGNS {
            return Err(CallsignError::Exhausted);
        }

        loop {
            let callsign = Self::random_callsign();
            if !self.used.contains(&callsign) {
                self.used.insert(callsign.clone());
                return Ok(callsign);
            }
            // Loop until we find an unused one
            // This is efficient unless >99% of callsigns are used
        }
    }

    /// Reserve a specific callsign for manual assignment.
    ///
    /// Returns `Ok(())` if the callsign was successfully reserved.
    /// Returns `Err` if the callsign is invalid or already in use.
    pub fn reserve(&mut self, callsign: &str) -> Result<(), CallsignError> {
        let normalized = Self::normalize(callsign);

        if !Self::is_valid_format(&normalized) {
            return Err(CallsignError::InvalidFormat(callsign.to_string()));
        }

        if self.used.contains(&normalized) {
            return Err(CallsignError::AlreadyInUse(callsign.to_string()));
        }

        self.used.insert(normalized);
        Ok(())
    }

    /// Release a callsign for reuse.
    ///
    /// Returns `true` if the callsign was found and released.
    pub fn release(&mut self, callsign: &str) -> bool {
        let normalized = Self::normalize(callsign);
        self.used.remove(&normalized)
    }

    /// Check if a callsign is available.
    pub fn is_available(&self, callsign: &str) -> bool {
        let normalized = Self::normalize(callsign);
        Self::is_valid_format(&normalized) && !self.used.contains(&normalized)
    }

    /// Check if a callsign has valid format.
    ///
    /// Valid format: `{NATO_WORD}-{NN}` where NN is 00-99.
    pub fn is_valid_format(callsign: &str) -> bool {
        Self::parse(callsign).is_some()
    }

    /// Parse a callsign into (letter_index, number).
    ///
    /// Returns `None` if the format is invalid.
    pub fn parse(callsign: &str) -> Option<(usize, u8)> {
        let normalized = Self::normalize(callsign);
        let parts: Vec<&str> = normalized.split('-').collect();

        if parts.len() != 2 {
            return None;
        }

        let word = parts[0];
        let num_str = parts[1];

        // Find NATO word index
        let letter_idx = NATO_ALPHABET.iter().position(|&w| w == word)?;

        // Parse number (00-99)
        if num_str.len() != 2 {
            return None;
        }
        let number: u8 = num_str.parse().ok()?;
        if number > 99 {
            return None;
        }

        Some((letter_idx, number))
    }

    /// Get count of used callsigns.
    pub fn used_count(&self) -> usize {
        self.used.len()
    }

    /// Get count of available callsigns.
    pub fn available_count(&self) -> usize {
        TOTAL_CALLSIGNS.saturating_sub(self.used.len())
    }

    /// Generate a random callsign (not checking availability).
    fn random_callsign() -> String {
        let mut bytes = [0u8; 2];
        OsRng.fill_bytes(&mut bytes);

        let letter_idx = (bytes[0] as usize) % 26;
        let number = bytes[1] % 100;

        format!("{}-{:02}", NATO_ALPHABET[letter_idx], number)
    }

    /// Normalize callsign to uppercase.
    fn normalize(callsign: &str) -> String {
        callsign.trim().to_uppercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_callsign() {
        let mut gen = CallsignGenerator::new();
        let callsign = gen.generate().unwrap();

        // Should be in format WORD-NN
        assert!(CallsignGenerator::is_valid_format(&callsign));
        assert!(callsign.contains('-'));

        let parts: Vec<&str> = callsign.split('-').collect();
        assert_eq!(parts.len(), 2);
        assert!(NATO_ALPHABET.contains(&parts[0]));

        let num: u8 = parts[1].parse().unwrap();
        assert!(num <= 99);
    }

    #[test]
    fn test_generate_unique() {
        let mut gen = CallsignGenerator::new();
        let mut callsigns = HashSet::new();

        for _ in 0..100 {
            let cs = gen.generate().unwrap();
            assert!(callsigns.insert(cs), "duplicate callsign generated");
        }
    }

    #[test]
    fn test_reserve() {
        let mut gen = CallsignGenerator::new();

        assert!(gen.reserve("ALPHA-01").is_ok());
        assert!(gen.reserve("BRAVO-42").is_ok());

        // Already in use
        assert!(matches!(
            gen.reserve("ALPHA-01"),
            Err(CallsignError::AlreadyInUse(_))
        ));

        // Case insensitive
        assert!(matches!(
            gen.reserve("alpha-01"),
            Err(CallsignError::AlreadyInUse(_))
        ));
    }

    #[test]
    fn test_reserve_invalid() {
        let mut gen = CallsignGenerator::new();

        // Invalid format
        assert!(matches!(
            gen.reserve("INVALID"),
            Err(CallsignError::InvalidFormat(_))
        ));
        assert!(matches!(
            gen.reserve("ALPHA-100"),
            Err(CallsignError::InvalidFormat(_))
        ));
        assert!(matches!(
            gen.reserve("ALPHA-1"),
            Err(CallsignError::InvalidFormat(_))
        ));
        assert!(matches!(
            gen.reserve("BADWORD-01"),
            Err(CallsignError::InvalidFormat(_))
        ));
    }

    #[test]
    fn test_release() {
        let mut gen = CallsignGenerator::new();

        gen.reserve("ZULU-99").unwrap();
        assert!(!gen.is_available("ZULU-99"));

        assert!(gen.release("ZULU-99"));
        assert!(gen.is_available("ZULU-99"));

        // Release non-existent returns false
        assert!(!gen.release("ZULU-99"));
    }

    #[test]
    fn test_is_available() {
        let mut gen = CallsignGenerator::new();

        assert!(gen.is_available("CHARLIE-05"));
        gen.reserve("CHARLIE-05").unwrap();
        assert!(!gen.is_available("CHARLIE-05"));

        // Invalid format is not available
        assert!(!gen.is_available("INVALID-FORMAT"));
    }

    #[test]
    fn test_parse() {
        assert_eq!(CallsignGenerator::parse("ALPHA-00"), Some((0, 0)));
        assert_eq!(CallsignGenerator::parse("BRAVO-01"), Some((1, 1)));
        assert_eq!(CallsignGenerator::parse("ZULU-99"), Some((25, 99)));
        assert_eq!(CallsignGenerator::parse("november-42"), Some((13, 42)));

        assert_eq!(CallsignGenerator::parse("INVALID"), None);
        assert_eq!(CallsignGenerator::parse("ALPHA-100"), None);
        assert_eq!(CallsignGenerator::parse("ALPHA-1"), None);
    }

    #[test]
    fn test_counts() {
        let mut gen = CallsignGenerator::new();

        assert_eq!(gen.used_count(), 0);
        assert_eq!(gen.available_count(), TOTAL_CALLSIGNS);

        gen.reserve("ALPHA-01").unwrap();
        gen.reserve("BRAVO-02").unwrap();

        assert_eq!(gen.used_count(), 2);
        assert_eq!(gen.available_count(), TOTAL_CALLSIGNS - 2);
    }

    #[test]
    fn test_nato_alphabet() {
        assert_eq!(NATO_ALPHABET.len(), 26);
        assert_eq!(NATO_ALPHABET[0], "ALPHA");
        assert_eq!(NATO_ALPHABET[25], "ZULU");
    }

    #[test]
    fn test_max_length() {
        // NOVEMBER-99 is the longest possible callsign
        assert_eq!("NOVEMBER-99".len(), MAX_CALLSIGN_LENGTH);
    }
}
