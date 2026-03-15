//! Re-exported from peat-mesh. See [`peat_mesh::security::encryption`].
//!
//! This module is a thin re-export wrapper. The canonical implementation and
//! unit tests live in `peat_mesh::security::encryption`. Tests here verify
//! that the re-exports are accessible through `peat_protocol`.
#[allow(unused_imports)]
pub use peat_mesh::security::encryption::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reexport_symmetric_encrypt_decrypt_roundtrip() {
        let key = SymmetricKey::from_bytes(&[0xAB; SYMMETRIC_KEY_SIZE]);
        let plaintext = b"peat-protocol encryption roundtrip";
        let encrypted = key.encrypt(plaintext).unwrap();
        let decrypted = key.decrypt(&encrypted).unwrap();
        assert_eq!(plaintext.as_slice(), &decrypted);
    }

    #[test]
    fn reexport_encrypted_data_serialization_roundtrip() {
        let key = SymmetricKey::from_bytes(&[0xCD; SYMMETRIC_KEY_SIZE]);
        let encrypted = key.encrypt(b"serialize me").unwrap();
        let bytes = encrypted.to_bytes();
        let restored = EncryptedData::from_bytes(&bytes).unwrap();
        let decrypted = key.decrypt(&restored).unwrap();
        assert_eq!(decrypted, b"serialize me");
    }

    #[test]
    fn reexport_wrong_key_fails() {
        let key1 = SymmetricKey::from_bytes(&[0x01; SYMMETRIC_KEY_SIZE]);
        let key2 = SymmetricKey::from_bytes(&[0x02; SYMMETRIC_KEY_SIZE]);
        let encrypted = key1.encrypt(b"secret").unwrap();
        assert!(key2.decrypt(&encrypted).is_err());
    }

    #[test]
    fn reexport_encryption_keypair_dh_exchange() {
        let alice = EncryptionKeypair::generate();
        let bob = EncryptionKeypair::generate();
        let alice_shared = alice.dh_exchange(bob.public_key());
        let bob_shared = bob.dh_exchange(alice.public_key());
        assert_eq!(alice_shared.as_bytes(), bob_shared.as_bytes());
    }

    #[test]
    fn reexport_encrypted_data_from_bytes_too_short() {
        let result = EncryptedData::from_bytes(&[0u8; 5]);
        assert!(result.is_err());
    }

    #[test]
    fn reexport_group_key_encrypt_decrypt() {
        let key = GroupKey::generate("test-cell".to_string());
        let plaintext = b"cell broadcast via peat-protocol";
        let encrypted = key.encrypt(plaintext).unwrap();
        let decrypted = key.decrypt(&encrypted).unwrap();
        assert_eq!(plaintext.as_slice(), &decrypted);
    }

    #[test]
    fn reexport_group_key_rotation() {
        let key1 = GroupKey::generate("cell-r".to_string());
        let key2 = key1.rotate();
        assert_eq!(key2.cell_id, "cell-r");
        assert_eq!(key2.generation, key1.generation + 1);
        // Old key cannot decrypt new key's messages
        let encrypted = key2.encrypt(b"new").unwrap();
        assert!(key1.decrypt(&encrypted).is_err());
    }

    #[test]
    fn reexport_constants_accessible() {
        assert_eq!(NONCE_SIZE, 12);
        assert_eq!(SYMMETRIC_KEY_SIZE, 32);
        assert_eq!(X25519_PUBLIC_KEY_SIZE, 32);
    }
}
