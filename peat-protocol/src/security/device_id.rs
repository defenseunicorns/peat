//! Re-exported from peat-mesh. See [`peat_mesh::security::device_id`].
//!
//! This module is a thin re-export wrapper. The canonical implementation and
//! unit tests live in `peat_mesh::security::device_id`. Tests here verify
//! that the re-exports are accessible through `peat_protocol`.
pub use peat_mesh::security::device_id::*;

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand_core::OsRng;

    #[test]
    fn reexport_from_public_key_deterministic() {
        let key = SigningKey::generate(&mut OsRng);
        let id1 = DeviceId::from_public_key(&key.verifying_key());
        let id2 = DeviceId::from_public_key(&key.verifying_key());
        assert_eq!(id1, id2);
    }

    #[test]
    fn reexport_different_keys_different_ids() {
        let k1 = SigningKey::generate(&mut OsRng);
        let k2 = SigningKey::generate(&mut OsRng);
        assert_ne!(
            DeviceId::from_public_key(&k1.verifying_key()),
            DeviceId::from_public_key(&k2.verifying_key()),
        );
    }

    #[test]
    fn reexport_hex_roundtrip() {
        let key = SigningKey::generate(&mut OsRng);
        let id = DeviceId::from_public_key(&key.verifying_key());
        let hex = id.to_hex();
        assert_eq!(hex.len(), 32);
        let parsed = DeviceId::from_hex(&hex).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn reexport_from_hex_invalid() {
        assert!(DeviceId::from_hex("not-hex").is_err());
        assert!(DeviceId::from_hex("abc").is_err()); // odd length
        assert!(DeviceId::from_hex("00112233").is_err()); // too short
    }

    #[test]
    fn reexport_from_bytes_and_as_bytes() {
        let raw = [0xAB; 16];
        let id = DeviceId::from_bytes(raw);
        assert_eq!(*id.as_bytes(), raw);
    }

    #[test]
    fn reexport_from_public_key_bytes() {
        let key = SigningKey::generate(&mut OsRng);
        let pk_bytes = key.verifying_key().to_bytes();
        let id = DeviceId::from_public_key_bytes(&pk_bytes).unwrap();
        let expected = DeviceId::from_public_key(&key.verifying_key());
        assert_eq!(id, expected);
    }

    #[test]
    fn reexport_from_public_key_bytes_wrong_length() {
        assert!(DeviceId::from_public_key_bytes(&[0u8; 16]).is_err());
    }

    #[test]
    fn reexport_display_and_debug() {
        let id = DeviceId::from_bytes([0xCD; 16]);
        let display = format!("{}", id);
        let debug = format!("{:?}", id);
        assert_eq!(display, "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd");
        assert!(debug.starts_with("DeviceId("));
    }
}
