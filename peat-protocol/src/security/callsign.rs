//! Re-exported from peat-mesh. See [`peat_mesh::security::callsign`].
//!
//! This module is a thin re-export wrapper. The canonical implementation and
//! unit tests live in `peat_mesh::security::callsign`. Tests here verify
//! that the re-exports are accessible through `peat_protocol`.
#[allow(unused_imports)]
pub use peat_mesh::security::callsign::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reexport_generate_callsign() {
        let mut gen = CallsignGenerator::new();
        let callsign = gen.generate().unwrap();
        assert!(CallsignGenerator::is_valid_format(&callsign));
    }

    #[test]
    fn reexport_generate_unique() {
        let mut gen = CallsignGenerator::new();
        let cs1 = gen.generate().unwrap();
        let cs2 = gen.generate().unwrap();
        assert_ne!(cs1, cs2);
    }

    #[test]
    fn reexport_reserve_and_release() {
        let mut gen = CallsignGenerator::new();
        gen.reserve("ALPHA-01").unwrap();
        assert!(!gen.is_available("ALPHA-01"));
        assert!(gen.release("ALPHA-01"));
        assert!(gen.is_available("ALPHA-01"));
    }

    #[test]
    fn reexport_reserve_already_in_use() {
        let mut gen = CallsignGenerator::new();
        gen.reserve("BRAVO-42").unwrap();
        assert!(matches!(
            gen.reserve("BRAVO-42"),
            Err(CallsignError::AlreadyInUse(_))
        ));
    }

    #[test]
    fn reexport_reserve_invalid_format() {
        let mut gen = CallsignGenerator::new();
        assert!(matches!(
            gen.reserve("INVALID"),
            Err(CallsignError::InvalidFormat(_))
        ));
        assert!(matches!(
            gen.reserve("ALPHA-100"),
            Err(CallsignError::InvalidFormat(_))
        ));
    }

    #[test]
    fn reexport_parse() {
        assert_eq!(CallsignGenerator::parse("ALPHA-00"), Some((0, 0)));
        assert_eq!(CallsignGenerator::parse("ZULU-99"), Some((25, 99)));
        assert_eq!(CallsignGenerator::parse("INVALID"), None);
    }

    #[test]
    fn reexport_counts() {
        let mut gen = CallsignGenerator::new();
        assert_eq!(gen.used_count(), 0);
        assert_eq!(gen.available_count(), TOTAL_CALLSIGNS);
        gen.reserve("ECHO-05").unwrap();
        assert_eq!(gen.used_count(), 1);
        assert_eq!(gen.available_count(), TOTAL_CALLSIGNS - 1);
    }

    #[test]
    fn reexport_constants_accessible() {
        assert_eq!(NATO_ALPHABET.len(), 26);
        assert_eq!(NATO_ALPHABET[0], "ALPHA");
        assert_eq!(MAX_CALLSIGN_LENGTH, 11);
        assert_eq!(TOTAL_CALLSIGNS, 2600);
    }
}
