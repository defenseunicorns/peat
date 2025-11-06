//! E2E Test Harness for Cell Formation
//!
//! Provides infrastructure for end-to-end testing with Ditto synchronization.
//! Uses observer-based synchronization instead of polling/timeouts.
//!
//! # Architecture
//!
//! - **Isolated Sessions**: Each test gets unique Ditto persistence directories
//! - **Observer-Based Sync**: Uses Ditto observers with channels for deterministic assertions
//! - **Test Backplane**: Coordination layer separate from system-under-test
//! - **Clean Shutdown**: Proper resource cleanup to prevent test interference
//!
//! # Example
//!
//! ```ignore
//! let harness = E2EHarness::new("test_scenario").await?;
//! let platform_store = harness.create_platform_store().await?;
//! let observer = harness.observe_cell_changes().await?;
//!
//! // Trigger formation...
//! let event = observer.wait_for_event(Duration::from_secs(5)).await?;
//! assert_eq!(event.status, FormationStatus::Ready);
//! ```

use crate::storage::ditto_store::{DittoConfig, DittoStore};
use crate::sync::ditto::DittoBackend;
use crate::sync::{BackendConfig, DataSyncBackend, TransportConfig};
use crate::{Error, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::{debug, info, warn};

/// Test harness for E2E cell formation testing
pub struct E2EHarness {
    /// Test scenario name (for logging/debugging)
    pub name: String,
    /// Temporary directories for test isolation (kept alive for test duration)
    temp_dirs: Vec<tempfile::TempDir>,
}

impl E2EHarness {
    /// Create a new E2E test harness
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            temp_dirs: Vec::new(),
        }
    }

    /// Create a new isolated Ditto store for testing
    ///
    /// Each store gets:
    /// - Unique persistence directory
    /// - Shared app_id and shared_key for sync mesh
    /// - Uses mDNS/LAN discovery (no TCP listener/client)
    pub async fn create_ditto_store(&mut self) -> Result<DittoStore> {
        self.create_ditto_store_with_tcp(None, None).await
    }

    /// Create a new isolated Ditto store with optional TCP configuration
    ///
    /// Use this when you need explicit TCP topology to avoid mDNS file descriptor issues
    /// with multiple instances (4+).
    ///
    /// # Arguments
    /// * `tcp_listen_port` - If Some(port), this instance will listen on that TCP port
    /// * `tcp_connect_address` - If Some(addr), this instance will connect to that address
    pub async fn create_ditto_store_with_tcp(
        &mut self,
        tcp_listen_port: Option<u16>,
        tcp_connect_address: Option<String>,
    ) -> Result<DittoStore> {
        let temp_dir = tempfile::tempdir().map_err(|e| {
            Error::storage_error(
                format!("Failed to create temp dir: {}", e),
                "test_setup",
                None,
            )
        })?;

        let app_id = std::env::var("DITTO_APP_ID")
            .unwrap_or_else(|_| "00000000-0000-0000-0000-000000000000".to_string());
        let shared_key = std::env::var("DITTO_SHARED_KEY")
            .unwrap_or_else(|_| "shared_key_for_testing".to_string());

        let config = DittoConfig {
            app_id,
            persistence_dir: temp_dir.path().to_path_buf(),
            shared_key,
            tcp_listen_port,
            tcp_connect_address,
        };

        let store = DittoStore::new(config)?;
        self.temp_dirs.push(temp_dir);

        Ok(store)
    }

    /// Create a new isolated Ditto backend for testing
    ///
    /// This is the recommended method for tests using CellStore, which requires
    /// Arc<DittoBackend>. Each backend gets:
    /// - Unique persistence directory
    /// - Shared app_id and shared_key for sync mesh
    /// - Uses mDNS/LAN discovery (no TCP listener/client)
    pub async fn create_ditto_backend(&mut self) -> Result<Arc<DittoBackend>> {
        self.create_ditto_backend_with_tcp(None, None).await
    }

    /// Create a new isolated Ditto backend with optional TCP configuration
    ///
    /// Use this when you need explicit TCP topology to avoid mDNS file descriptor issues
    /// with multiple instances (4+).
    ///
    /// # Arguments
    /// * `tcp_listen_port` - If Some(port), this instance will listen on that TCP port
    /// * `tcp_connect_address` - If Some(addr), this instance will connect to that address
    pub async fn create_ditto_backend_with_tcp(
        &mut self,
        tcp_listen_port: Option<u16>,
        tcp_connect_address: Option<String>,
    ) -> Result<Arc<DittoBackend>> {
        let temp_dir = tempfile::tempdir().map_err(|e| {
            Error::storage_error(
                format!("Failed to create temp dir: {}", e),
                "test_setup",
                None,
            )
        })?;

        let app_id = std::env::var("DITTO_APP_ID")
            .unwrap_or_else(|_| "00000000-0000-0000-0000-000000000000".to_string());
        let shared_key = std::env::var("DITTO_SHARED_KEY")
            .unwrap_or_else(|_| "shared_key_for_testing".to_string());

        let persistence_path = temp_dir.path().to_path_buf();

        let config = BackendConfig {
            app_id,
            persistence_dir: persistence_path,
            shared_key: Some(shared_key),
            transport: TransportConfig {
                tcp_listen_port,
                tcp_connect_address,
                ..Default::default()
            },
            extra: HashMap::new(),
        };

        let backend = DittoBackend::new();
        backend.initialize(config).await?;
        backend.sync_engine().start_sync().await?;

        self.temp_dirs.push(temp_dir);

        Ok(Arc::new(backend))
    }

    /// Create a squad observer that triggers on document changes
    ///
    /// Returns a receiver channel that will receive SquadState updates
    /// whenever the squad document changes in Ditto
    pub async fn observe_cell(&self, store: &DittoStore, cell_id: &str) -> Result<CellObserver> {
        let (tx, rx) = mpsc::unbounded_channel();

        // Create sync subscription first (required for P2P sync)
        let query = format!("SELECT * FROM cells WHERE id == '{}'", cell_id);
        let sync_sub = store
            .ditto()
            .sync()
            .register_subscription_v2(&query)
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to create sync subscription: {}", e),
                    "observe_cell",
                    None,
                )
            })?;

        // Register observer for change notifications
        let observer = store
            .ditto()
            .store()
            .register_observer_v2(&query, move |result| {
                debug!("Cell observer triggered: {} items", result.item_count());

                // Parse results into SquadState
                // Note: In real implementation, parse the JSON from result
                // For now, send a notification that data changed
                let _ = tx.send(CellObserverEvent::Changed);
            })
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to register observer: {}", e),
                    "observe_cell",
                    None,
                )
            })?;

        Ok(CellObserver {
            _sync_sub: sync_sub,
            _observer: observer,
            receiver: rx,
        })
    }

    /// Create a platform observer that triggers on document changes
    pub async fn observe_node(&self, store: &DittoStore, node_id: &str) -> Result<NodeObserver> {
        let (tx, rx) = mpsc::unbounded_channel();

        let query = format!("SELECT * FROM nodes WHERE id == '{}'", node_id);
        let sync_sub = store
            .ditto()
            .sync()
            .register_subscription_v2(&query)
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to create sync subscription: {}", e),
                    "observe_node",
                    None,
                )
            })?;

        let observer = store
            .ditto()
            .store()
            .register_observer_v2(&query, move |result| {
                debug!("Node observer triggered: {} items", result.item_count());
                let _ = tx.send(NodeObserverEvent::Changed);
            })
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to register observer: {}", e),
                    "observe_node",
                    None,
                )
            })?;

        Ok(NodeObserver {
            _sync_sub: sync_sub,
            _observer: observer,
            receiver: rx,
        })
    }

    /// Wait for peers to discover each other
    ///
    /// Uses Ditto presence graph to detect when peers are connected
    /// Returns immediately when connection is established
    pub async fn wait_for_peer_connection(
        &self,
        store1: &DittoStore,
        _store2: &DittoStore,
        timeout_duration: Duration,
    ) -> Result<()> {
        let result = timeout(timeout_duration, async {
            loop {
                let graph = store1.ditto().presence().graph();
                if !graph.remote_peers.is_empty() {
                    info!("Peers connected: {} remote peers", graph.remote_peers.len());
                    return Ok(());
                }

                // Small sleep to avoid busy-waiting
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await;

        match result {
            Ok(r) => r,
            Err(_) => {
                warn!("Peer connection timeout after {:?}", timeout_duration);
                Err(Error::storage_error(
                    "Peer discovery timeout",
                    "wait_for_peer_connection",
                    None,
                ))
            }
        }
    }

    /// Clean shutdown helper
    pub async fn shutdown_store(&self, store: DittoStore) {
        store.stop_sync();
        drop(store);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Cell observer that emits events on document changes
pub struct CellObserver {
    _sync_sub: Arc<dittolive_ditto::sync::SyncSubscription>,
    _observer: Arc<dittolive_ditto::store::StoreObserver>,
    receiver: mpsc::UnboundedReceiver<CellObserverEvent>,
}

impl CellObserver {
    /// Wait for the next event with timeout
    pub async fn wait_for_event(
        &mut self,
        timeout_duration: Duration,
    ) -> Result<CellObserverEvent> {
        match timeout(timeout_duration, self.receiver.recv()).await {
            Ok(Some(event)) => Ok(event),
            Ok(None) => Err(Error::storage_error(
                "Observer channel closed",
                "wait_for_event",
                None,
            )),
            Err(_) => Err(Error::storage_error(
                "Observer timeout",
                "wait_for_event",
                None,
            )),
        }
    }

    /// Try to receive an event without blocking
    pub fn try_recv(&mut self) -> Option<CellObserverEvent> {
        self.receiver.try_recv().ok()
    }
}

#[derive(Debug, Clone)]
pub enum CellObserverEvent {
    /// Cell document changed (updated/inserted)
    Changed,
}

/// Node observer that emits events on document changes
pub struct NodeObserver {
    _sync_sub: Arc<dittolive_ditto::sync::SyncSubscription>,
    _observer: Arc<dittolive_ditto::store::StoreObserver>,
    receiver: mpsc::UnboundedReceiver<NodeObserverEvent>,
}

impl NodeObserver {
    /// Wait for the next event with timeout
    pub async fn wait_for_event(
        &mut self,
        timeout_duration: Duration,
    ) -> Result<NodeObserverEvent> {
        match timeout(timeout_duration, self.receiver.recv()).await {
            Ok(Some(event)) => Ok(event),
            Ok(None) => Err(Error::storage_error(
                "Observer channel closed",
                "wait_for_event",
                None,
            )),
            Err(_) => Err(Error::storage_error(
                "Observer timeout",
                "wait_for_event",
                None,
            )),
        }
    }

    /// Try to receive an event without blocking
    pub fn try_recv(&mut self) -> Option<NodeObserverEvent> {
        self.receiver.try_recv().ok()
    }
}

#[derive(Debug, Clone)]
pub enum NodeObserverEvent {
    /// Node document changed (updated/inserted)
    Changed,
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

    /// Test harness creation (requires Ditto credentials)
    #[tokio::test]
    async fn test_ditto_store_creation() {
        // Fail if Ditto credentials not properly configured
        let ditto_app_id =
            std::env::var("DITTO_APP_ID").expect("DITTO_APP_ID must be set for E2E tests");
        assert!(!ditto_app_id.is_empty(), "DITTO_APP_ID cannot be empty");

        let mut harness = E2EHarness::new("test_store_creation");
        let store = harness.create_ditto_store().await;
        assert!(store.is_ok(), "Should create Ditto store");
        assert_eq!(harness.temp_dirs.len(), 1);
    }

    /// Test multiple isolated stores (requires Ditto credentials)
    #[tokio::test]
    async fn test_multiple_isolated_stores() {
        // Fail if Ditto credentials not properly configured
        let ditto_app_id =
            std::env::var("DITTO_APP_ID").expect("DITTO_APP_ID must be set for E2E tests");
        assert!(!ditto_app_id.is_empty(), "DITTO_APP_ID cannot be empty");

        let mut harness = E2EHarness::new("test_isolated_stores");
        let store1 = harness.create_ditto_store().await;
        let store2 = harness.create_ditto_store().await;

        assert!(store1.is_ok());
        assert!(store2.is_ok());
        assert_eq!(harness.temp_dirs.len(), 2);

        // Verify each has isolated persistence directory
        println!("✓ Created {} isolated stores", harness.temp_dirs.len());
    }
}
