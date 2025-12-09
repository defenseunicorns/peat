//! Formation Authentication Handshake for Iroh Connections
//!
//! This module provides the challenge-response handshake protocol for
//! authenticating peers as members of the same formation.
//!
//! ## Protocol
//!
//! After QUIC connection establishment:
//!
//! ```text
//! Initiator                              Responder
//!     |                                      |
//!     |  ---- QUIC Connect (TLS) ---->       |
//!     |                                      |
//!     |  <---- Challenge (FormationChallenge) |
//!     |                                      |
//!     |  ---- Response (HMAC) ---------->    |
//!     |                                      |
//!     |  <---- Result (Accept/Reject) ---    |
//!     |                                      |
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use hive_protocol::network::formation_handshake::{
//!     perform_initiator_handshake, perform_responder_handshake
//! };
//!
//! // On connection initiator side:
//! let result = perform_initiator_handshake(&connection, &formation_key).await?;
//!
//! // On connection responder side:
//! let result = perform_responder_handshake(&connection, &formation_key).await?;
//! ```

#[cfg(feature = "automerge-backend")]
use crate::security::{
    FormationAuthResult, FormationChallenge, FormationChallengeResponse, FormationKey,
    FORMATION_RESPONSE_SIZE,
};
#[cfg(feature = "automerge-backend")]
use anyhow::{Context, Result};
#[cfg(feature = "automerge-backend")]
use iroh::endpoint::Connection;

/// ALPN for formation handshake stream
#[cfg(feature = "automerge-backend")]
pub const FORMATION_HANDSHAKE_ALPN: &[u8] = b"hive/formation-auth/1";

/// Timeout for handshake operations (5 seconds)
#[cfg(feature = "automerge-backend")]
const HANDSHAKE_TIMEOUT_SECS: u64 = 5;

/// Perform the initiator side of the formation handshake
///
/// Called by the node that initiated the QUIC connection.
/// Sends formation ID first, then receives challenge and sends HMAC response.
///
/// # Arguments
///
/// * `connection` - The established QUIC connection
/// * `formation_key` - The formation key for authentication
///
/// # Returns
///
/// `Ok(())` if authentication succeeded, error otherwise
#[cfg(feature = "automerge-backend")]
pub async fn perform_initiator_handshake(
    connection: &Connection,
    formation_key: &FormationKey,
) -> Result<()> {
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;

    // Open a bidirectional stream for the handshake
    let (mut send, mut recv) = tokio::time::timeout(
        Duration::from_secs(HANDSHAKE_TIMEOUT_SECS),
        connection.open_bi(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Handshake stream open timeout"))?
    .context("Failed to open handshake stream")?;

    // Step 1: Send our formation ID to trigger the handshake
    let formation_id_bytes = formation_key.formation_id().as_bytes();
    let len = formation_id_bytes.len() as u16;
    send.write_all(&len.to_le_bytes()).await?;
    send.write_all(formation_id_bytes).await?;
    send.flush().await?;

    // Step 2: Receive challenge from responder
    let mut challenge_buf = vec![0u8; 256];
    let n = tokio::time::timeout(
        Duration::from_secs(HANDSHAKE_TIMEOUT_SECS),
        recv.read(&mut challenge_buf),
    )
    .await
    .context("Challenge receive timeout")?
    .context("Failed to read challenge")?
    .ok_or_else(|| anyhow::anyhow!("Connection closed before challenge received"))?;

    let challenge = FormationChallenge::from_bytes(&challenge_buf[..n])
        .map_err(|e| anyhow::anyhow!("Invalid challenge: {}", e))?;

    // Verify formation ID matches
    if challenge.formation_id != formation_key.formation_id() {
        anyhow::bail!(
            "Formation ID mismatch: expected '{}', got '{}'",
            formation_key.formation_id(),
            challenge.formation_id
        );
    }

    // Step 3: Compute and send response
    let response_bytes = formation_key.respond_to_challenge(&challenge.nonce);
    let response = FormationChallengeResponse {
        response: response_bytes,
    };

    send.write_all(&response.to_bytes()).await?;
    send.flush().await?;

    // Step 4: Receive result
    let mut result_buf = [0u8; 1];
    tokio::time::timeout(
        Duration::from_secs(HANDSHAKE_TIMEOUT_SECS),
        recv.read_exact(&mut result_buf),
    )
    .await
    .context("Result receive timeout")?
    .context("Failed to read result")?;

    let result = FormationAuthResult::from_byte(result_buf[0]);

    match result {
        FormationAuthResult::Accepted => {
            tracing::debug!(
                "Formation handshake succeeded with {}",
                formation_key.formation_id()
            );
            Ok(())
        }
        FormationAuthResult::Rejected => {
            anyhow::bail!("Formation handshake rejected by peer")
        }
    }
}

/// Perform the responder side of the formation handshake
///
/// Called by the node that accepted the QUIC connection.
/// Receives formation ID, sends challenge, and verifies response.
///
/// # Arguments
///
/// * `connection` - The established QUIC connection
/// * `formation_key` - The formation key for authentication
///
/// # Returns
///
/// `Ok(())` if authentication succeeded, error otherwise
#[cfg(feature = "automerge-backend")]
pub async fn perform_responder_handshake(
    connection: &Connection,
    formation_key: &FormationKey,
) -> Result<()> {
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;

    // Accept the handshake stream from the initiator
    let (mut send, mut recv) = tokio::time::timeout(
        Duration::from_secs(HANDSHAKE_TIMEOUT_SECS),
        connection.accept_bi(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Handshake stream accept timeout"))?
    .context("Failed to accept handshake stream")?;

    // Step 1: Receive initiator's formation ID
    let mut len_buf = [0u8; 2];
    recv.read_exact(&mut len_buf).await?;
    let len = u16::from_le_bytes(len_buf) as usize;

    let mut formation_id_buf = vec![0u8; len];
    recv.read_exact(&mut formation_id_buf).await?;
    let peer_formation_id = String::from_utf8(formation_id_buf)
        .map_err(|e| anyhow::anyhow!("Invalid formation ID from peer: {}", e))?;

    // Verify formation ID matches (optional - we could allow different IDs)
    if peer_formation_id != formation_key.formation_id() {
        tracing::warn!(
            "Peer formation ID '{}' doesn't match ours '{}'",
            peer_formation_id,
            formation_key.formation_id()
        );
        // Still send challenge with our formation ID - initiator will detect mismatch
    }

    // Step 2: Generate and send challenge
    let (nonce, _expected_response) = formation_key.create_challenge();
    let challenge = FormationChallenge {
        formation_id: formation_key.formation_id().to_string(),
        nonce,
    };

    send.write_all(&challenge.to_bytes()).await?;
    send.flush().await?;

    // Step 3: Receive response
    let mut response_buf = [0u8; FORMATION_RESPONSE_SIZE];
    tokio::time::timeout(
        Duration::from_secs(HANDSHAKE_TIMEOUT_SECS),
        recv.read_exact(&mut response_buf),
    )
    .await
    .context("Response receive timeout")?
    .context("Failed to read response")?;

    let response = FormationChallengeResponse::from_bytes(&response_buf)
        .map_err(|e| anyhow::anyhow!("Invalid response: {}", e))?;

    // Step 4: Verify response and send result
    let verified = formation_key.verify_response(&nonce, &response.response);

    let result = if verified {
        FormationAuthResult::Accepted
    } else {
        FormationAuthResult::Rejected
    };

    send.write_all(&[result.to_byte()]).await?;
    send.flush().await?;

    if verified {
        tracing::debug!(
            "Formation handshake verified for {}",
            formation_key.formation_id()
        );
        Ok(())
    } else {
        anyhow::bail!("Formation handshake verification failed - peer has wrong key")
    }
}

#[cfg(all(test, feature = "automerge-backend"))]
mod tests {
    use super::*;
    use crate::network::iroh_transport::IrohTransport;
    use std::sync::Arc;
    use tokio::sync::oneshot;

    /// Helper to run handshake with proper synchronization
    async fn run_handshake_test(
        key1: FormationKey,
        key2: FormationKey,
    ) -> (Result<()>, Result<()>) {
        let transport1 = Arc::new(IrohTransport::new().await.unwrap());
        let transport2 = Arc::new(IrohTransport::new().await.unwrap());

        // With deterministic tie-breaking, only the lower ID initiates connections.
        // Determine which transport should be initiator vs responder.
        let t1_is_lower = transport1.endpoint_id().as_bytes() < transport2.endpoint_id().as_bytes();

        let (initiator, responder, initiator_key, responder_key) = if t1_is_lower {
            (transport1, transport2, key1, key2)
        } else {
            (transport2, transport1, key2, key1)
        };

        let responder_addr = responder.endpoint_addr();

        // Use oneshot channel to synchronize
        let (ready_tx, ready_rx) = oneshot::channel::<()>();

        // Spawn responder task
        let responder_clone = Arc::clone(&responder);
        let responder_task = tokio::spawn(async move {
            // Signal we're ready to accept
            let _ = ready_tx.send(());
            // accept() returns Option<Connection> - unwrap expects Some since this is first connection
            let conn = responder_clone
                .accept()
                .await
                .unwrap()
                .expect("Expected new connection, not duplicate");
            perform_responder_handshake(&conn, &responder_key).await
        });

        // Wait for responder to be ready
        ready_rx.await.unwrap();
        // Small additional delay to ensure accept() is called
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Initiator connects and handshakes (conflict resolution handled by transport layer)
        let conn = initiator
            .connect(responder_addr)
            .await
            .expect("Connection should succeed")
            .expect("Should get new connection (not handled by accept)");
        let initiator_result = perform_initiator_handshake(&conn, &initiator_key).await;

        // Wait for responder
        let responder_result = responder_task.await.unwrap();

        // Always return (initiator_result, responder_result) regardless of which transport
        // was the initiator. Tests expect the first element to be from the initiator.
        (initiator_result, responder_result)
    }

    #[tokio::test]
    async fn test_formation_handshake_success() {
        let secret = [0x42u8; 32];
        let key1 = FormationKey::new("test-formation", &secret);
        let key2 = FormationKey::new("test-formation", &secret);

        let (initiator_result, responder_result) = run_handshake_test(key1, key2).await;

        assert!(
            initiator_result.is_ok(),
            "Initiator failed: {:?}",
            initiator_result
        );
        assert!(
            responder_result.is_ok(),
            "Responder failed: {:?}",
            responder_result
        );
    }

    #[tokio::test]
    async fn test_formation_handshake_wrong_key() {
        let key1 = FormationKey::new("test-formation", &[0x42u8; 32]);
        let key2 = FormationKey::new("test-formation", &[0x43u8; 32]); // Different secret

        let (initiator_result, responder_result) = run_handshake_test(key1, key2).await;

        // Responder should reject (wrong key)
        assert!(responder_result.is_err());
        // Initiator should also fail (rejection received)
        assert!(initiator_result.is_err());
    }

    #[tokio::test]
    async fn test_formation_handshake_wrong_formation_id() {
        let secret = [0x42u8; 32];
        let key1 = FormationKey::new("formation-alpha", &secret);
        let key2 = FormationKey::new("formation-bravo", &secret);

        let (initiator_result, _responder_result) = run_handshake_test(key1, key2).await;

        // Initiator should fail because formation ID mismatch is detected early
        assert!(initiator_result.is_err());
        let err_msg = initiator_result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Formation ID mismatch"),
            "Expected 'Formation ID mismatch' but got: {}",
            err_msg
        );
    }
}
