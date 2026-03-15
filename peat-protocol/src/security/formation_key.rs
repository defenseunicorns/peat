//! Re-exported from peat-mesh. See [`peat_mesh::security::formation_key`].
//!
//! This module is a thin re-export wrapper. The canonical implementation and
//! unit tests live in `peat_mesh::security::formation_key`. Tests here verify
//! that the re-exports are accessible through `peat_protocol`.
#[allow(unused_imports)]
pub use peat_mesh::security::formation_key::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reexport_formation_key_challenge_response() {
        let secret = [0x42u8; 32];
        let key = FormationKey::new("test-formation", &secret);
        assert_eq!(key.formation_id(), "test-formation");

        let (nonce, _expected) = key.create_challenge();
        let response = key.respond_to_challenge(&nonce);
        assert!(key.verify_response(&nonce, &response));
    }

    #[test]
    fn reexport_wrong_key_rejected() {
        let key1 = FormationKey::new("formation", &[0x01; 32]);
        let key2 = FormationKey::new("formation", &[0x02; 32]);

        let (nonce, _) = key1.create_challenge();
        let response = key2.respond_to_challenge(&nonce);
        assert!(!key1.verify_response(&nonce, &response));
    }

    #[test]
    fn reexport_different_formation_rejected() {
        let secret = [0x42u8; 32];
        let key1 = FormationKey::new("alpha", &secret);
        let key2 = FormationKey::new("bravo", &secret);

        let (nonce, _) = key1.create_challenge();
        let response = key2.respond_to_challenge(&nonce);
        assert!(!key1.verify_response(&nonce, &response));
    }

    #[test]
    fn reexport_challenge_serialization_roundtrip() {
        let challenge = FormationChallenge {
            formation_id: "roundtrip-test".to_string(),
            nonce: [0xAB; FORMATION_CHALLENGE_SIZE],
        };
        let bytes = challenge.to_bytes();
        let restored = FormationChallenge::from_bytes(&bytes).unwrap();
        assert_eq!(challenge.formation_id, restored.formation_id);
        assert_eq!(challenge.nonce, restored.nonce);
    }

    #[test]
    fn reexport_challenge_from_bytes_too_short() {
        assert!(FormationChallenge::from_bytes(&[0u8; 1]).is_err());
    }

    #[test]
    fn reexport_response_serialization_roundtrip() {
        let resp = FormationChallengeResponse {
            response: [0xCD; FORMATION_RESPONSE_SIZE],
        };
        let bytes = resp.to_bytes();
        let restored = FormationChallengeResponse::from_bytes(&bytes).unwrap();
        assert_eq!(resp.response, restored.response);
    }

    #[test]
    fn reexport_response_from_bytes_too_short() {
        assert!(FormationChallengeResponse::from_bytes(&[0u8; 10]).is_err());
    }

    #[test]
    fn reexport_auth_result_byte_roundtrip() {
        assert_eq!(
            FormationAuthResult::from_byte(FormationAuthResult::Accepted.to_byte()),
            FormationAuthResult::Accepted
        );
        assert_eq!(
            FormationAuthResult::from_byte(FormationAuthResult::Rejected.to_byte()),
            FormationAuthResult::Rejected
        );
    }

    #[test]
    fn reexport_from_base64() {
        let secret = FormationKey::generate_secret();
        let key = FormationKey::from_base64("b64-test", &secret).unwrap();
        assert_eq!(key.formation_id(), "b64-test");
    }

    #[test]
    fn reexport_from_base64_invalid() {
        assert!(FormationKey::from_base64("x", "not-valid-base64!!!").is_err());
    }

    #[test]
    fn reexport_constants_accessible() {
        assert_eq!(FORMATION_CHALLENGE_SIZE, 32);
        assert_eq!(FORMATION_RESPONSE_SIZE, 32);
    }
}
