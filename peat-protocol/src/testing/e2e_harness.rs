//! E2E Test Harness for Cell Formation
//!
//! Provides infrastructure for end-to-end testing with Automerge + Iroh synchronization.
//! Uses observer-based synchronization instead of polling/timeouts.
//!
//! # Architecture
//!
//! - **Isolated Sessions**: Each test gets unique persistence directories
//! - **Observer-Based Sync**: Backend observers with channels for deterministic assertions
//! - **Test Backplane**: Coordination layer separate from system-under-test
//! - **Clean Shutdown**: Proper resource cleanup to prevent test interference
//!
//! # Example
//!
//! ```ignore
//! let harness = E2EHarness::new("test_scenario").await?;
//! let backend = harness.create_automerge_backend().await?;
//! // Trigger formation...
//! ```

use crate::sync::DataSyncBackend;
use crate::sync::{BackendConfig, TransportConfig};
use crate::{Error, Result};
use std::collections::HashMap;
use std::sync::Arc;
#[cfg(feature = "automerge-backend")]
use std::time::Duration;
#[cfg(feature = "automerge-backend")]
#[allow(unused_imports)]
use tracing::{debug, info};

#[cfg(feature = "automerge-backend")]
use crate::network::IrohTransport;
#[cfg(feature = "automerge-backend")]
use crate::storage::AutomergeStore;
#[cfg(feature = "automerge-backend")]
use crate::sync::automerge::AutomergeIrohBackend;

/// Test harness for E2E cell formation testing
pub struct E2EHarness {
    /// Test scenario name (for logging/debugging)
    pub name: String,
    /// Temporary directories for test isolation (kept alive for test duration)
    temp_dirs: Vec<tempfile::TempDir>,
    /// Shared test secret for AutomergeIroh peer authentication
    /// All backends created by this harness will share this secret
    #[cfg(feature = "automerge-backend")]
    test_secret: String,
}

impl E2EHarness {
    /// Create a new E2E test harness
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            temp_dirs: Vec::new(),
            #[cfg(feature = "automerge-backend")]
            test_secret: crate::security::FormationKey::generate_secret(),
        }
    }

    /// Allocate a random available TCP port
    ///
    /// This prevents port conflicts when running multiple tests concurrently.
    /// Uses OS-assigned ephemeral ports by binding to port 0 and retrieving the assigned port.
    pub fn allocate_tcp_port() -> std::io::Result<u16> {
        use std::net::{SocketAddr, TcpListener};

        // Bind to port 0 to get an OS-assigned ephemeral port
        let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))?;
        let port = listener.local_addr()?.port();

        // Drop the listener to free the port
        drop(listener);

        Ok(port)
    }

    /// Create a new isolated Automerge+Iroh backend for testing
    ///
    /// This creates an AutomergeIrohBackend instance with:
    /// - Unique persistence directory (RocksDB)
    /// - Iroh QUIC transport on random available port
    /// - Automatic sync coordination
    ///
    /// # Feature Gate
    ///
    /// Only available with the `automerge-backend` feature enabled.
    #[cfg(feature = "automerge-backend")]
    pub async fn create_automerge_backend(&mut self) -> Result<Arc<AutomergeIrohBackend>> {
        self.create_automerge_backend_with_bind(None).await
    }

    /// Create a new isolated Automerge+Iroh backend with optional bind address
    ///
    /// Use this when you need to bind to a specific address/port for testing.
    ///
    /// # Arguments
    /// * `bind_addr` - Optional socket address to bind the Iroh endpoint to
    ///
    /// # Feature Gate
    ///
    /// Only available with the `automerge-backend` feature enabled.
    #[cfg(feature = "automerge-backend")]
    pub async fn create_automerge_backend_with_bind(
        &mut self,
        bind_addr: Option<std::net::SocketAddr>,
    ) -> Result<Arc<AutomergeIrohBackend>> {
        let temp_dir = tempfile::tempdir().map_err(|e| {
            Error::storage_error(
                format!("Failed to create temp dir: {}", e),
                "test_setup",
                None,
            )
        })?;

        // Create AutomergeStore with RocksDB persistence
        let store = Arc::new(AutomergeStore::open(temp_dir.path()).map_err(|e| {
            Error::storage_error(
                format!("Failed to create AutomergeStore: {}", e),
                "test_setup",
                None,
            )
        })?);

        // Create IrohTransport
        let transport = if let Some(addr) = bind_addr {
            Arc::new(IrohTransport::bind(addr).await.map_err(|e| {
                Error::network_error(format!("Failed to bind Iroh transport: {}", e), None)
            })?)
        } else {
            Arc::new(IrohTransport::new().await.map_err(|e| {
                Error::network_error(format!("Failed to create Iroh transport: {}", e), None)
            })?)
        };

        // Create the adapter backend
        let backend = Arc::new(AutomergeIrohBackend::from_parts(store, transport));

        // Initialize the backend with config (this also starts the accept loop via peer_discovery().start())
        // All backends in this harness share the same test_secret for peer authentication
        let config = BackendConfig {
            app_id: "automerge-test".to_string(),
            persistence_dir: temp_dir.path().to_path_buf(),
            shared_key: Some(self.test_secret.clone()),
            transport: TransportConfig::default(),
            extra: HashMap::new(),
        };

        backend.initialize(config).await?;

        self.temp_dirs.push(temp_dir);

        Ok(backend)
    }

    /// Immediately connect two Automerge backends
    ///
    /// This bypasses the background connection task's periodic interval, allowing
    /// tests to establish connections in milliseconds instead of 1-7 seconds.
    ///
    /// Both backends must have been initialized with `start_sync()` called
    /// (so the accept loop is running).
    ///
    /// # Returns
    ///
    /// Returns Ok(()) when at least one direction is connected.
    /// Returns Err if neither side could establish a connection after retries.
    ///
    /// # Feature Gate
    ///
    /// Only available with the `automerge-backend` feature enabled.
    #[cfg(feature = "automerge-backend")]
    pub async fn connect_backends_now(
        &self,
        backend_a: &Arc<AutomergeIrohBackend>,
        backend_b: &Arc<AutomergeIrohBackend>,
    ) -> Result<()> {
        // Give a tiny delay for accept loops to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Try connecting from both sides (deterministic tie-breaking means only one will succeed)
        let (result_a, result_b) = tokio::join!(
            backend_a.connect_to_discovered_peers_now(),
            backend_b.connect_to_discovered_peers_now()
        );

        // Check if at least one connection was made
        let count_a = result_a.unwrap_or(0);
        let count_b = result_b.unwrap_or(0);

        if count_a > 0 || count_b > 0 {
            debug!(
                "Fast connect: A made {} new, B made {} new",
                count_a, count_b
            );
            return Ok(());
        }

        // Retry a few times with small delays (connection might still be establishing)
        for i in 0..5 {
            tokio::time::sleep(Duration::from_millis(100)).await;

            let (result_a, result_b) = tokio::join!(
                backend_a.connect_to_discovered_peers_now(),
                backend_b.connect_to_discovered_peers_now()
            );

            if result_a.unwrap_or(0) > 0 || result_b.unwrap_or(0) > 0 {
                debug!("Fast connect succeeded on retry {}", i + 1);
                return Ok(());
            }

            // Also check if they're already connected (from previous attempt)
            let transport_a = backend_a.transport();
            let transport_b = backend_b.transport();
            if !transport_a.connected_peers().is_empty()
                || !transport_b.connected_peers().is_empty()
            {
                debug!("Already connected on retry {}", i + 1);
                return Ok(());
            }
        }

        Err(Error::network_error(
            "Failed to establish connection between backends after retries",
            None,
        ))
    }

    /// Wait for connection between Automerge backends with fast polling
    ///
    /// This uses immediate connect attempts rather than waiting for background tasks.
    /// Typical connection time is 50-500ms instead of 1-7 seconds.
    #[cfg(feature = "automerge-backend")]
    pub async fn wait_for_automerge_connection(
        &self,
        backend_a: &Arc<AutomergeIrohBackend>,
        backend_b: &Arc<AutomergeIrohBackend>,
        timeout_duration: Duration,
    ) -> Result<()> {
        let start = std::time::Instant::now();

        while start.elapsed() < timeout_duration {
            // Try immediate connect
            let _ = backend_a.connect_to_discovered_peers_now().await;
            let _ = backend_b.connect_to_discovered_peers_now().await;

            // Check if connected
            let transport_a = backend_a.transport();
            let transport_b = backend_b.transport();
            if !transport_a.connected_peers().is_empty()
                || !transport_b.connected_peers().is_empty()
            {
                info!("Connection established in {:?}", start.elapsed());
                return Ok(());
            }

            // Small delay before retry
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        Err(Error::network_error(
            format!("Connection timeout after {:?}", timeout_duration),
            None,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_harness_creation() {
        let harness = E2EHarness::new("test_scenario");
        assert_eq!(harness.name, "test_scenario");
        assert_eq!(harness.temp_dirs.len(), 0);
    }
}
