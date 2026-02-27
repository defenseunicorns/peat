//! Secure transport wrapper for authenticated mesh connections.
//!
//! This module provides a decorator pattern wrapper that adds Ed25519 authentication
//! to any `MeshTransport` implementation. It implements the challenge-response protocol
//! to authenticate peers before allowing sync operations.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │          SecureMeshTransport            │
//! │  ┌──────────────────────────────────┐   │
//! │  │     DeviceAuthenticator          │   │
//! │  │  (Ed25519 challenge-response)    │   │
//! │  └──────────────────────────────────┘   │
//! │                  │                      │
//! │  ┌──────────────────────────────────┐   │
//! │  │   Inner MeshTransport            │   │
//! │  │   (Iroh or Ditto)                │   │
//! │  └──────────────────────────────────┘   │
//! └─────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use peat_protocol::security::{DeviceKeypair, SecureMeshTransport};
//! use peat_protocol::transport::MeshTransport;
//!
//! // Create keypair and inner transport
//! let keypair = DeviceKeypair::generate();
//! let inner_transport: Arc<dyn MeshTransport> = ...;
//!
//! // Wrap with security
//! let secure_transport = SecureMeshTransport::new(keypair, inner_transport);
//!
//! // Connect authenticates first
//! let conn = secure_transport.connect(&peer_id).await?;
//! // conn.peer_id() is now cryptographically verified
//! ```

use super::authenticator::DeviceAuthenticator;
use super::device_id::DeviceId;
use super::error::SecurityError;
use super::keypair::DeviceKeypair;
use crate::transport::{
    MeshConnection, MeshTransport, NodeId, Result as TransportResult, TransportError,
};
use async_trait::async_trait;
use peat_schema::security::v1::{Challenge, SignedChallengeResponse};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Authentication callback for custom transport-level auth message exchange.
///
/// This trait allows the secure transport to exchange authentication messages
/// over any underlying transport mechanism.
#[async_trait]
pub trait AuthenticationChannel: Send + Sync {
    /// Send an authentication challenge to a peer.
    async fn send_challenge(
        &self,
        peer_id: &NodeId,
        challenge: &Challenge,
    ) -> Result<(), SecurityError>;

    /// Receive a challenge response from a peer.
    async fn receive_response(
        &self,
        peer_id: &NodeId,
    ) -> Result<SignedChallengeResponse, SecurityError>;

    /// Send a challenge response to a peer.
    async fn send_response(
        &self,
        peer_id: &NodeId,
        response: &SignedChallengeResponse,
    ) -> Result<(), SecurityError>;

    /// Receive a challenge from a peer.
    async fn receive_challenge(&self, peer_id: &NodeId) -> Result<Challenge, SecurityError>;
}

/// Secure mesh transport that requires authentication before sync.
///
/// This wrapper adds Ed25519-based challenge-response authentication to any
/// `MeshTransport` implementation. Peers must complete mutual authentication
/// before the connection is considered established.
pub struct SecureMeshTransport<T: MeshTransport, A: AuthenticationChannel> {
    /// The device authenticator for crypto operations
    authenticator: DeviceAuthenticator,

    /// The underlying transport
    inner: Arc<T>,

    /// Authentication channel for message exchange
    auth_channel: Arc<A>,

    /// Mapping from NodeId to DeviceId for authenticated peers
    authenticated_peers: RwLock<HashMap<NodeId, DeviceId>>,
}

impl<T: MeshTransport, A: AuthenticationChannel> SecureMeshTransport<T, A> {
    /// Create a new secure transport wrapper.
    ///
    /// # Arguments
    ///
    /// * `keypair` - This device's keypair for authentication
    /// * `inner` - The underlying transport to wrap
    /// * `auth_channel` - Channel for exchanging authentication messages
    pub fn new(keypair: DeviceKeypair, inner: Arc<T>, auth_channel: Arc<A>) -> Self {
        Self {
            authenticator: DeviceAuthenticator::new(keypair),
            inner,
            auth_channel,
            authenticated_peers: RwLock::new(HashMap::new()),
        }
    }

    /// Get this device's ID.
    pub fn device_id(&self) -> DeviceId {
        self.authenticator.device_id()
    }

    /// Check if a peer is authenticated.
    pub fn is_authenticated(&self, peer_id: &NodeId) -> bool {
        self.authenticated_peers
            .read()
            .map(|peers| peers.contains_key(peer_id))
            .unwrap_or(false)
    }

    /// Get the DeviceId for an authenticated peer.
    pub fn get_peer_device_id(&self, peer_id: &NodeId) -> Option<DeviceId> {
        self.authenticated_peers
            .read()
            .ok()
            .and_then(|peers| peers.get(peer_id).copied())
    }

    /// Authenticate a peer using challenge-response.
    ///
    /// This performs mutual authentication:
    /// 1. We send a challenge to the peer
    /// 2. Peer responds with signed challenge
    /// 3. We verify the response
    /// 4. Peer sends us a challenge
    /// 5. We respond with signed challenge
    /// 6. Both sides are now authenticated
    pub async fn authenticate_peer(&self, peer_id: &NodeId) -> Result<DeviceId, SecurityError> {
        // Check if already authenticated
        if let Some(device_id) = self.get_peer_device_id(peer_id) {
            return Ok(device_id);
        }

        // Step 1: Generate and send challenge
        let challenge = self.authenticator.generate_challenge();
        self.auth_channel
            .send_challenge(peer_id, &challenge)
            .await?;

        // Step 2: Receive and verify response
        let response = self.auth_channel.receive_response(peer_id).await?;
        let device_id = self.authenticator.verify_response(&response)?;

        // Step 3: Receive challenge from peer (mutual auth)
        let peer_challenge = self.auth_channel.receive_challenge(peer_id).await?;

        // Step 4: Respond to peer's challenge
        let our_response = self.authenticator.respond_to_challenge(&peer_challenge)?;
        self.auth_channel
            .send_response(peer_id, &our_response)
            .await?;

        // Cache the authenticated peer
        if let Ok(mut peers) = self.authenticated_peers.write() {
            peers.insert(peer_id.clone(), device_id);
        }

        Ok(device_id)
    }

    /// Remove a peer from the authenticated cache.
    pub fn remove_authenticated_peer(&self, peer_id: &NodeId) {
        if let Ok(mut peers) = self.authenticated_peers.write() {
            if let Some(device_id) = peers.remove(peer_id) {
                self.authenticator.remove_peer(&device_id);
            }
        }
    }

    /// Get the number of authenticated peers.
    pub fn authenticated_peer_count(&self) -> usize {
        self.authenticated_peers
            .read()
            .map(|peers| peers.len())
            .unwrap_or(0)
    }

    /// Get the underlying authenticator (for testing or advanced use).
    pub fn authenticator(&self) -> &DeviceAuthenticator {
        &self.authenticator
    }
}

#[async_trait]
impl<T: MeshTransport + 'static, A: AuthenticationChannel + 'static> MeshTransport
    for SecureMeshTransport<T, A>
{
    async fn start(&self) -> TransportResult<()> {
        self.inner.start().await
    }

    async fn stop(&self) -> TransportResult<()> {
        self.inner.stop().await
    }

    async fn connect(&self, peer_id: &NodeId) -> TransportResult<Box<dyn MeshConnection>> {
        // First establish the underlying connection
        let conn = self.inner.connect(peer_id).await?;

        // Then authenticate the peer
        self.authenticate_peer(peer_id).await.map_err(|e| {
            TransportError::ConnectionFailed(format!("Authentication failed: {}", e))
        })?;

        // Return an authenticated connection wrapper
        Ok(Box::new(AuthenticatedConnection {
            inner: conn,
            device_id: self.get_peer_device_id(peer_id).unwrap(), // Safe: just authenticated
        }))
    }

    async fn disconnect(&self, peer_id: &NodeId) -> TransportResult<()> {
        self.remove_authenticated_peer(peer_id);
        self.inner.disconnect(peer_id).await
    }

    fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn MeshConnection>> {
        // Only return connection if peer is authenticated
        if let Some(device_id) = self.get_peer_device_id(peer_id) {
            self.inner.get_connection(peer_id).map(|conn| {
                Box::new(AuthenticatedConnection {
                    inner: conn,
                    device_id,
                }) as Box<dyn MeshConnection>
            })
        } else {
            None
        }
    }

    fn peer_count(&self) -> usize {
        self.authenticated_peer_count()
    }

    fn connected_peers(&self) -> Vec<NodeId> {
        self.authenticated_peers
            .read()
            .map(|peers| peers.keys().cloned().collect())
            .unwrap_or_default()
    }

    fn is_connected(&self, peer_id: &NodeId) -> bool {
        self.is_authenticated(peer_id) && self.inner.is_connected(peer_id)
    }

    fn subscribe_peer_events(&self) -> crate::transport::PeerEventReceiver {
        // Delegate to inner transport - events are emitted at the transport layer
        self.inner.subscribe_peer_events()
    }
}

/// An authenticated connection wrapper.
///
/// This wraps an underlying `MeshConnection` and tracks the verified DeviceId
/// of the remote peer.
pub struct AuthenticatedConnection {
    inner: Box<dyn MeshConnection>,
    device_id: DeviceId,
}

impl AuthenticatedConnection {
    /// Get the verified DeviceId of the remote peer.
    pub fn verified_device_id(&self) -> DeviceId {
        self.device_id
    }
}

impl MeshConnection for AuthenticatedConnection {
    fn peer_id(&self) -> &NodeId {
        self.inner.peer_id()
    }

    fn is_alive(&self) -> bool {
        self.inner.is_alive()
    }

    fn connected_at(&self) -> std::time::Instant {
        self.inner.connected_at()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{
        MeshConnection, MeshTransport, NodeId, Result as TransportResult, TransportError,
    };
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Mock transport for testing
    struct MockTransport {
        started: AtomicBool,
        connections: RwLock<HashMap<String, MockConnection>>,
    }

    impl MockTransport {
        fn new() -> Self {
            Self {
                started: AtomicBool::new(false),
                connections: RwLock::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl MeshTransport for MockTransport {
        async fn start(&self) -> TransportResult<()> {
            self.started.store(true, Ordering::SeqCst);
            Ok(())
        }

        async fn stop(&self) -> TransportResult<()> {
            self.started.store(false, Ordering::SeqCst);
            Ok(())
        }

        async fn connect(&self, peer_id: &NodeId) -> TransportResult<Box<dyn MeshConnection>> {
            if !self.started.load(Ordering::SeqCst) {
                return Err(TransportError::NotStarted);
            }
            let now = std::time::Instant::now();
            let conn = MockConnection {
                peer_id: peer_id.clone(),
                alive: AtomicBool::new(true),
                connected_at: now,
            };
            self.connections.write().unwrap().insert(
                peer_id.to_string(),
                MockConnection {
                    peer_id: peer_id.clone(),
                    alive: AtomicBool::new(true),
                    connected_at: now,
                },
            );
            Ok(Box::new(conn))
        }

        async fn disconnect(&self, peer_id: &NodeId) -> TransportResult<()> {
            self.connections
                .write()
                .unwrap()
                .remove(&peer_id.to_string());
            Ok(())
        }

        fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn MeshConnection>> {
            self.connections.read().ok().and_then(|conns| {
                conns.get(&peer_id.to_string()).map(|c| {
                    Box::new(MockConnection {
                        peer_id: c.peer_id.clone(),
                        alive: AtomicBool::new(c.alive.load(Ordering::SeqCst)),
                        connected_at: c.connected_at,
                    }) as Box<dyn MeshConnection>
                })
            })
        }

        fn peer_count(&self) -> usize {
            self.connections.read().map(|c| c.len()).unwrap_or(0)
        }

        fn connected_peers(&self) -> Vec<NodeId> {
            self.connections
                .read()
                .map(|c| c.values().map(|conn| conn.peer_id.clone()).collect())
                .unwrap_or_default()
        }

        fn subscribe_peer_events(&self) -> crate::transport::PeerEventReceiver {
            let (_tx, rx) = tokio::sync::mpsc::channel(256);
            rx
        }
    }

    struct MockConnection {
        peer_id: NodeId,
        alive: AtomicBool,
        connected_at: std::time::Instant,
    }

    impl MeshConnection for MockConnection {
        fn peer_id(&self) -> &NodeId {
            &self.peer_id
        }

        fn is_alive(&self) -> bool {
            self.alive.load(Ordering::SeqCst)
        }

        fn connected_at(&self) -> std::time::Instant {
            self.connected_at
        }
    }

    /// Mock auth channel that always succeeds (for basic transport tests)
    struct MockAuthChannel {
        /// Peer keypairs for simulating responses
        peer_keypairs: RwLock<HashMap<String, DeviceKeypair>>,
        /// Last challenge sent (for consistent response)
        last_challenge: RwLock<Option<Challenge>>,
    }

    impl MockAuthChannel {
        fn new() -> Self {
            Self {
                peer_keypairs: RwLock::new(HashMap::new()),
                last_challenge: RwLock::new(None),
            }
        }

        fn register_peer_keypair(&self, peer_id: &NodeId, keypair: DeviceKeypair) {
            if let Ok(mut peers) = self.peer_keypairs.write() {
                peers.insert(peer_id.to_string(), keypair);
            }
        }
    }

    #[async_trait]
    impl AuthenticationChannel for MockAuthChannel {
        async fn send_challenge(
            &self,
            _peer_id: &NodeId,
            challenge: &Challenge,
        ) -> Result<(), SecurityError> {
            // Store the challenge for when we need to create a response
            if let Ok(mut last) = self.last_challenge.write() {
                *last = Some(challenge.clone());
            }
            Ok(())
        }

        async fn receive_response(
            &self,
            peer_id: &NodeId,
        ) -> Result<SignedChallengeResponse, SecurityError> {
            // Return a valid response from the peer's keypair
            let keypair = self
                .peer_keypairs
                .read()
                .map_err(|e| SecurityError::Internal(e.to_string()))?
                .get(&peer_id.to_string())
                .cloned()
                .ok_or_else(|| SecurityError::PeerNotFound(peer_id.to_string()))?;

            // Use the challenge that was sent (with correct challenger_id)
            let challenge = self
                .last_challenge
                .read()
                .map_err(|e| SecurityError::Internal(e.to_string()))?
                .clone()
                .ok_or_else(|| SecurityError::Internal("no challenge sent".to_string()))?;

            let authenticator = DeviceAuthenticator::new(keypair);
            authenticator.respond_to_challenge(&challenge)
        }

        async fn send_response(
            &self,
            _peer_id: &NodeId,
            _response: &SignedChallengeResponse,
        ) -> Result<(), SecurityError> {
            Ok(())
        }

        async fn receive_challenge(&self, _peer_id: &NodeId) -> Result<Challenge, SecurityError> {
            Ok(Challenge {
                nonce: vec![0u8; 32],
                timestamp: None,
                challenger_id: "peer".to_string(),
                expires_at: Some(peat_schema::common::v1::Timestamp {
                    seconds: u64::MAX,
                    nanos: 0,
                }),
            })
        }
    }

    #[tokio::test]
    async fn test_secure_transport_creation() {
        let keypair = DeviceKeypair::generate();
        let transport = Arc::new(MockTransport::new());
        let auth_channel = Arc::new(MockAuthChannel::new());

        let secure = SecureMeshTransport::new(keypair, transport, auth_channel);

        assert_eq!(secure.authenticated_peer_count(), 0);
    }

    #[tokio::test]
    async fn test_secure_transport_start_stop() {
        let keypair = DeviceKeypair::generate();
        let transport = Arc::new(MockTransport::new());
        let auth_channel = Arc::new(MockAuthChannel::new());

        let secure = SecureMeshTransport::new(keypair, transport.clone(), auth_channel);

        assert!(!transport.started.load(Ordering::SeqCst));
        secure.start().await.unwrap();
        assert!(transport.started.load(Ordering::SeqCst));
        secure.stop().await.unwrap();
        assert!(!transport.started.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_secure_transport_connect_authenticates() {
        let our_keypair = DeviceKeypair::generate();
        let peer_keypair = DeviceKeypair::generate();
        let peer_id: NodeId = peer_keypair.device_id().into();

        let transport = Arc::new(MockTransport::new());
        let auth_channel = Arc::new(MockAuthChannel::new());
        auth_channel.register_peer_keypair(&peer_id, peer_keypair.clone());

        let secure = SecureMeshTransport::new(our_keypair, transport, auth_channel);

        secure.start().await.unwrap();
        let conn = secure.connect(&peer_id).await.unwrap();

        assert!(secure.is_authenticated(&peer_id));
        assert_eq!(conn.peer_id(), &peer_id);
        assert!(conn.is_alive());
    }

    #[tokio::test]
    async fn test_secure_transport_disconnect_removes_auth() {
        let our_keypair = DeviceKeypair::generate();
        let peer_keypair = DeviceKeypair::generate();
        let peer_id: NodeId = peer_keypair.device_id().into();

        let transport = Arc::new(MockTransport::new());
        let auth_channel = Arc::new(MockAuthChannel::new());
        auth_channel.register_peer_keypair(&peer_id, peer_keypair);

        let secure = SecureMeshTransport::new(our_keypair, transport, auth_channel);

        secure.start().await.unwrap();
        secure.connect(&peer_id).await.unwrap();
        assert!(secure.is_authenticated(&peer_id));

        secure.disconnect(&peer_id).await.unwrap();
        assert!(!secure.is_authenticated(&peer_id));
    }

    #[tokio::test]
    async fn test_authenticated_connection_exposes_device_id() {
        let our_keypair = DeviceKeypair::generate();
        let peer_keypair = DeviceKeypair::generate();
        let peer_device_id = peer_keypair.device_id();
        let peer_id: NodeId = peer_device_id.into();

        let transport = Arc::new(MockTransport::new());
        let auth_channel = Arc::new(MockAuthChannel::new());
        auth_channel.register_peer_keypair(&peer_id, peer_keypair);

        let secure = SecureMeshTransport::new(our_keypair, transport, auth_channel);

        secure.start().await.unwrap();
        let _conn = secure.connect(&peer_id).await.unwrap();

        // Verify we can get the peer's device ID through the transport
        assert!(secure.is_authenticated(&peer_id));
        assert_eq!(secure.get_peer_device_id(&peer_id), Some(peer_device_id));
    }
}
