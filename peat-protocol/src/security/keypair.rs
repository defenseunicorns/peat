//! Re-exported from peat-mesh. See [`peat_mesh::security::keypair`].
//!
//! This module is a thin re-export wrapper. The canonical implementation and
//! unit tests live in `peat_mesh::security::keypair`. Tests here verify
//! that the re-exports are accessible through `peat_protocol`.
pub use peat_mesh::security::keypair::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reexport_generate_and_device_id() {
        let kp = DeviceKeypair::generate();
        let id = kp.device_id();
        assert_eq!(id.to_hex().len(), 32);
    }

    #[test]
    fn reexport_sign_verify_roundtrip() {
        let kp = DeviceKeypair::generate();
        let message = b"peat-protocol keypair roundtrip";
        let sig = kp.sign(message);
        assert!(kp.verify(message, &sig).is_ok());
    }

    #[test]
    fn reexport_verify_wrong_message_fails() {
        let kp = DeviceKeypair::generate();
        let sig = kp.sign(b"original");
        assert!(kp.verify(b"tampered", &sig).is_err());
    }

    #[test]
    fn reexport_from_secret_bytes_roundtrip() {
        let kp1 = DeviceKeypair::generate();
        let secret = kp1.secret_key_bytes();
        let kp2 = DeviceKeypair::from_secret_bytes(&secret).unwrap();
        assert_eq!(kp1.device_id(), kp2.device_id());
    }

    #[test]
    fn reexport_from_secret_bytes_invalid_length() {
        assert!(DeviceKeypair::from_secret_bytes(&[0u8; 16]).is_err());
    }

    #[test]
    fn reexport_from_seed_deterministic() {
        let kp1 = DeviceKeypair::from_seed(b"seed", "ctx").unwrap();
        let kp2 = DeviceKeypair::from_seed(b"seed", "ctx").unwrap();
        assert_eq!(kp1.device_id(), kp2.device_id());
    }

    #[test]
    fn reexport_from_seed_different_context_differs() {
        let kp1 = DeviceKeypair::from_seed(b"seed", "ctx-a").unwrap();
        let kp2 = DeviceKeypair::from_seed(b"seed", "ctx-b").unwrap();
        assert_ne!(kp1.device_id(), kp2.device_id());
    }

    #[test]
    fn reexport_verify_with_key() {
        let kp = DeviceKeypair::generate();
        let msg = b"data";
        let sig = kp.sign(msg);
        let vk = kp.verifying_key();
        assert!(DeviceKeypair::verify_with_key(&vk, msg, &sig).is_ok());
        assert!(DeviceKeypair::verify_with_key(&vk, b"wrong", &sig).is_err());
    }

    #[test]
    fn reexport_signature_from_bytes_roundtrip() {
        let kp = DeviceKeypair::generate();
        let sig = kp.sign(b"msg");
        let sig_bytes = sig.to_bytes();
        let parsed = DeviceKeypair::signature_from_bytes(&sig_bytes).unwrap();
        assert_eq!(sig, parsed);
    }

    #[test]
    fn reexport_signature_from_bytes_invalid_length() {
        assert!(DeviceKeypair::signature_from_bytes(&[0u8; 32]).is_err());
    }
}
