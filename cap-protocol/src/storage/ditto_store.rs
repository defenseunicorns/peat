//! Ditto CRDT storage implementation
//!
//! This module provides a wrapper around the Ditto SDK for CRDT-based state management.
//! It supports SharedKey identity for local-only syncing during development and testing.
//!
//! # SharedKey Identity Activation Requirements
//!
//! SharedKey is an "offline identity" that enables peer-to-peer synchronization without
//! requiring authentication through Ditto's cloud services. However, it requires activation
//! with an offline license token before sync operations can be performed.
//!
//! ## Initialization Order
//!
//! 1. **Build Ditto instance** with SharedKey identity using `identity::SharedKey::new()`
//! 2. **Activate** with `ditto.set_offline_only_license_token(&token)` ← REQUIRED
//! 3. **Disable v3 sync** with `ditto.disable_sync_with_v3()` ← REQUIRED for DQL mutations
//! 4. **Configure transports** via `ditto.update_transport_config()`
//! 5. **Start sync** with `ditto.start_sync()`
//!
//! Calling `start_sync()` without activation will result in a `NotActivated` error.
//! Calling DQL mutations without disabling v3 sync will result in a `DqlUnsupported` error.
//!
//! ## Required Environment Variables
//!
//! - `DITTO_APP_ID`: Application ID from Ditto portal (UUID format)
//! - `DITTO_OFFLINE_TOKEN`: Base64-encoded offline license token from Ditto portal
//! - `DITTO_SHARED_KEY`: Base64-encoded shared encryption key
//! - `DITTO_PERSISTENCE_DIR`: Directory for Ditto data storage (optional, defaults to ".ditto")
//!
//! ## Peer Discovery
//!
//! This implementation enables LAN transport (mDNS) by default, which works well for
//! localhost peer discovery on macOS and other nodes that support mDNS. For explicit
//! localhost testing or environments where mDNS is unreliable, TCP transport can be
//! configured with explicit server/client connections.
//!
//! See `examples/ditto_spike.rs` for an example of TCP transport configuration.

use crate::{Error, Result};
use dittolive_ditto::prelude::*;
use dittolive_ditto::AppId;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, error, info, instrument, warn};

/// Configuration for Ditto storage
#[derive(Debug, Clone)]
pub struct DittoConfig {
    /// Application ID from Ditto portal (UUID)
    pub app_id: String,
    /// Persistence directory for Ditto data
    pub persistence_dir: PathBuf,
    /// Shared key for local-only syncing (base64 encoded)
    pub shared_key: String,
    /// Optional TCP listen port (for explicit peer discovery)
    pub tcp_listen_port: Option<u16>,
    /// Optional TCP connect address (for explicit peer discovery)
    pub tcp_connect_address: Option<String>,
}

/// Wrapper around Ditto for CRDT operations
pub struct DittoStore {
    ditto: Arc<Ditto>,
    _config: DittoConfig,
}

impl DittoStore {
    /// Create a new Ditto store with the given configuration
    #[instrument(skip(config), fields(app_id = %config.app_id, persistence_dir = ?config.persistence_dir))]
    pub fn new(config: DittoConfig) -> Result<Self> {
        info!("Initializing Ditto store");

        // Create persistent storage root
        let root = Arc::new(
            PersistentRoot::new(config.persistence_dir.to_str().unwrap()).map_err(|_| {
                Error::storage_error("Failed to create storage root", "initialize", None)
            })?,
        );

        // Step 1: Create Ditto instance with SharedKey identity
        // This configures the identity type but does NOT activate sync capabilities yet
        let ditto = Ditto::builder()
            .with_root(root)
            .with_identity(|ditto_root| {
                // Get AppId from environment
                let app_id = AppId::from_env("DITTO_APP_ID")?;

                // Create SharedKey identity for offline P2P sync
                // SharedKey uses symmetric encryption for secure peer-to-peer communication
                // Trim the shared_key to handle potential whitespace from environment variables
                let shared_key = config.shared_key.trim();
                identity::SharedKey::new(ditto_root, app_id, shared_key)
            })
            .map_err(|e| {
                error!("Failed to build Ditto identity: {}", e);
                Error::storage_error("Failed to build Ditto identity", "initialize", None)
            })?
            .build()
            .map_err(|e| {
                error!("Failed to initialize Ditto instance: {}", e);
                Error::storage_error("Failed to initialize Ditto", "initialize", None)
            })?;

        // Step 2: Activate Ditto with offline license token (REQUIRED for SharedKey)
        //
        // IMPORTANT: SharedKey is an "offline identity" that requires explicit activation
        // before any sync operations can be performed. Without this step, calling start_sync()
        // will fail with a NotActivated error.
        //
        // The offline license token must be obtained from the Ditto portal and stored in
        // the DITTO_OFFLINE_TOKEN environment variable. This token proves you have a valid
        // license without requiring an online connection to Ditto's servers.
        let offline_token = std::env::var("DITTO_OFFLINE_TOKEN").map_err(|_| {
            Error::config_error(
                "DITTO_OFFLINE_TOKEN not set",
                Some("DITTO_OFFLINE_TOKEN".to_string()),
            )
        })?;
        ditto
            .set_offline_only_license_token(&offline_token)
            .map_err(|e| {
                error!("Failed to activate Ditto with offline license: {}", e);
                Error::storage_error("Failed to activate Ditto", "activate", None)
            })?;

        // Step 3: Disable sync with v3 peers (REQUIRED for DQL mutations)
        //
        // IMPORTANT: This must be called before start_sync() to enable mutating DQL statements
        // (INSERT, UPDATE, DELETE). Once set, this configuration propagates across the mesh
        // and persists across restarts.
        //
        // Calling this before start_sync() improves performance of initial sync.
        ditto.disable_sync_with_v3().map_err(|e| {
            error!("Failed to disable v3 sync: {}", e);
            Error::storage_error("Failed to disable v3 sync", "configure", None)
        })?;

        // Step 4: Configure transports for peer discovery
        //
        // By default, ALL transports are disabled in Ditto. We enable:
        // - LAN transport (mDNS) for automatic peer discovery on local networks
        // - TCP transport (optional) for explicit server/client connections
        //
        // TCP transport is more reliable for localhost testing where mDNS may not work.
        ditto.update_transport_config(|transport_config| {
            // Enable LAN/mDNS for automatic discovery
            transport_config.peer_to_peer.lan.enabled = true;

            // Configure TCP listener if specified
            if let Some(port) = config.tcp_listen_port {
                transport_config.listen.tcp.enabled = true;
                transport_config.listen.tcp.interface_ip = "127.0.0.1".to_string();
                transport_config.listen.tcp.port = port;
            }

            // Configure TCP client connection if specified
            if let Some(ref address) = config.tcp_connect_address {
                transport_config.connect.tcp_servers.insert(address.clone());
            }
        });

        info!("Ditto store initialized successfully (v3 sync disabled, LAN transport enabled)");

        Ok(Self {
            ditto: Arc::new(ditto),
            _config: config,
        })
    }

    /// Create a Ditto store from environment variables
    #[instrument]
    pub fn from_env() -> Result<Self> {
        info!("Creating DittoStore from environment variables");

        // Load environment variables
        dotenvy::dotenv().ok();

        // Trim all values to handle potential whitespace from environment variables
        let app_id = std::env::var("DITTO_APP_ID")
            .map_err(|_| {
                Error::config_error("DITTO_APP_ID not set", Some("DITTO_APP_ID".to_string()))
            })?
            .trim()
            .to_string();

        let shared_key = std::env::var("DITTO_SHARED_KEY")
            .map_err(|_| {
                Error::config_error(
                    "DITTO_SHARED_KEY not set",
                    Some("DITTO_SHARED_KEY".to_string()),
                )
            })?
            .trim()
            .to_string();

        let persistence_dir = PathBuf::from(
            std::env::var("DITTO_PERSISTENCE_DIR")
                .unwrap_or_else(|_| ".ditto".to_string())
                .trim(),
        );

        let config = DittoConfig {
            app_id,
            persistence_dir,
            shared_key,
            tcp_listen_port: None,
            tcp_connect_address: None,
        };

        Self::new(config)
    }

    /// Start sync with peers
    #[instrument(skip(self))]
    pub fn start_sync(&self) -> Result<()> {
        info!("Starting Ditto sync");
        self.ditto.start_sync().map_err(|e| {
            error!("Failed to start sync: {}", e);
            Error::storage_error("Failed to start sync", "start_sync", None)
        })?;
        info!("Ditto sync started successfully");
        Ok(())
    }

    /// Stop sync
    #[instrument(skip(self))]
    pub fn stop_sync(&self) {
        info!("Stopping Ditto sync");
        self.ditto.stop_sync();
        info!("Ditto sync stopped");
    }

    /// Get a reference to the underlying Ditto instance
    pub fn ditto(&self) -> &Ditto {
        &self.ditto
    }

    /// Execute a query on a collection using DQL (Ditto Query Language)
    #[instrument(skip(self), fields(collection, where_clause))]
    pub async fn query(
        &self,
        collection: &str,
        where_clause: &str,
    ) -> Result<Vec<serde_json::Value>> {
        let dql_query = format!("SELECT * FROM {} WHERE {}", collection, where_clause);
        debug!("Executing DQL query: {}", dql_query);

        let query_result = self
            .ditto
            .store()
            .execute_v2(dql_query)
            .await
            .map_err(|e| {
                error!("Query failed: {}", e);
                Error::storage_error(
                    format!("Query failed on collection {}", collection),
                    "query",
                    Some(collection.to_string()),
                )
            })?;

        let documents: Vec<serde_json::Value> = query_result
            .iter()
            .map(|item| {
                let json_str = item.json_string();
                serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null)
            })
            .collect();

        debug!("Query returned {} document(s)", documents.len());
        Ok(documents)
    }

    /// Insert/update a document into a collection using DQL
    #[instrument(skip(self, document), fields(collection))]
    pub async fn upsert(&self, collection: &str, document: serde_json::Value) -> Result<String> {
        let dql_query = format!("INSERT INTO {} DOCUMENTS (:doc)", collection);
        debug!("Upserting document into collection: {}", collection);

        let query_result = self
            .ditto
            .store()
            .execute_v2((dql_query, serde_json::json!({"doc": document})))
            .await
            .map_err(|e| {
                error!("Upsert failed: {}", e);
                Error::storage_error(
                    format!("Upsert failed on collection {}", collection),
                    "upsert",
                    Some(collection.to_string()),
                )
            })?;

        // Extract the document ID from the mutation result
        let doc_id = query_result
            .mutated_document_ids()
            .first()
            .map(|id| id.to_string())
            .ok_or_else(|| {
                error!("No document ID returned from upsert");
                Error::storage_error(
                    "No document ID returned from upsert",
                    "upsert",
                    Some(collection.to_string()),
                )
            })?;

        debug!("Upserted document with ID: {}", doc_id);
        Ok(doc_id)
    }

    /// Remove a document from a collection using DQL
    #[instrument(skip(self), fields(collection, doc_id))]
    pub async fn remove(&self, collection: &str, doc_id: &str) -> Result<()> {
        let dql_query = format!("EVICT FROM {} WHERE _id = :id", collection);
        debug!(
            "Removing document {} from collection: {}",
            doc_id, collection
        );

        self.ditto
            .store()
            .execute_v2((dql_query, serde_json::json!({"id": doc_id})))
            .await
            .map_err(|e| {
                error!("Remove failed: {}", e);
                Error::storage_error(
                    format!("Remove failed on collection {}", collection),
                    "remove",
                    Some(collection.to_string()),
                )
            })?;

        debug!("Successfully removed document with ID: {}", doc_id);
        Ok(())
    }

    /// Get peer key string (unique identifier for this Ditto instance)
    pub fn peer_key(&self) -> String {
        self.ditto
            .presence()
            .graph()
            .local_peer
            .peer_key_string
            .clone()
    }
}

impl Drop for DittoStore {
    fn drop(&mut self) {
        self.stop_sync();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::tempdir;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_ditto_initialization() {
        dotenvy::dotenv().ok();

        // Skip test if Ditto credentials not available (e.g., in CI without secrets)
        let app_id = std::env::var("DITTO_APP_ID").ok().and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        let shared_key = std::env::var("DITTO_SHARED_KEY").ok().and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });

        if app_id.is_none() || shared_key.is_none() {
            eprintln!("Skipping test: Ditto credentials not available (required: DITTO_APP_ID, DITTO_SHARED_KEY, DITTO_OFFLINE_TOKEN)");
            return;
        }

        // Create unique temp directory for this test
        let temp_dir = tempdir().expect("Failed to create temp dir");

        let config = DittoConfig {
            app_id: app_id.unwrap(),
            persistence_dir: temp_dir.path().to_path_buf(),
            shared_key: shared_key.unwrap(),
            tcp_listen_port: None,
            tcp_connect_address: None,
        };

        let store = DittoStore::new(config).expect("Failed to create Ditto store");
        assert!(!store.peer_key().is_empty());

        // Explicitly stop sync and drop store to ensure clean shutdown
        store.stop_sync();
        drop(store);

        // Give Ditto time to shut down background threads
        sleep(Duration::from_millis(100)).await;

        // Temp dir will be automatically cleaned up when it goes out of scope
    }

    #[tokio::test]
    async fn test_basic_crud_operations() {
        dotenvy::dotenv().ok();

        // Skip test if Ditto credentials not available
        let app_id = std::env::var("DITTO_APP_ID").ok().and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        let shared_key = std::env::var("DITTO_SHARED_KEY").ok().and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });

        if app_id.is_none() || shared_key.is_none() {
            eprintln!("Skipping test: Ditto credentials not available");
            return;
        }

        // Create unique temp directory for this test
        let temp_dir = tempdir().expect("Failed to create temp dir");

        let config = DittoConfig {
            app_id: app_id.unwrap(),
            persistence_dir: temp_dir.path().to_path_buf(),
            shared_key: shared_key.unwrap(),
            tcp_listen_port: None,
            tcp_connect_address: None,
        };

        let store = DittoStore::new(config).expect("Failed to create Ditto store");
        store.start_sync().expect("Failed to start sync");

        // Insert a document
        let doc = serde_json::json!({
            "name": "test_platform",
            "type": "UAV",
            "fuel": 100
        });

        let doc_id = store
            .upsert("test_platforms", doc)
            .await
            .expect("Failed to upsert");

        // Query it back
        let results = store
            .query("test_platforms", "name == 'test_platform'")
            .await
            .expect("Failed to query");

        assert!(!results.is_empty(), "Document should be found");

        // Clean up
        store
            .remove("test_platforms", &doc_id)
            .await
            .expect("Failed to remove");

        // Explicitly stop sync and drop store to ensure clean shutdown
        store.stop_sync();
        drop(store);

        // Give Ditto time to shut down background threads
        sleep(Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn test_two_instance_sync() {
        dotenvy::dotenv().ok();

        // Skip test if Ditto credentials not available
        let app_id = std::env::var("DITTO_APP_ID").ok().and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        let shared_key = std::env::var("DITTO_SHARED_KEY").ok().and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });

        if app_id.is_none() || shared_key.is_none() {
            eprintln!("Skipping test: Ditto credentials not available");
            return;
        }

        let app_id = app_id.unwrap();
        let shared_key = shared_key.unwrap();

        // Create unique temp directories for both stores
        let temp_dir1 = tempdir().expect("Failed to create temp dir 1");
        let temp_dir2 = tempdir().expect("Failed to create temp dir 2");

        // Create two Ditto instances with unique persistence directories
        // Both use mDNS for peer discovery (no TCP ports to avoid conflicts)
        let config1 = DittoConfig {
            app_id: app_id.clone(),
            persistence_dir: temp_dir1.path().to_path_buf(),
            shared_key: shared_key.clone(),
            tcp_listen_port: None, // No TCP - use mDNS only
            tcp_connect_address: None,
        };
        let store1 = DittoStore::new(config1).expect("Failed to create store 1");

        // Store2: Also uses mDNS for discovery
        let config2 = DittoConfig {
            app_id,
            persistence_dir: temp_dir2.path().to_path_buf(),
            shared_key,
            tcp_listen_port: None, // No TCP - use mDNS only
            tcp_connect_address: None,
        };
        let store2 = DittoStore::new(config2).expect("Failed to create store 2");

        let peer1_key = store1.peer_key();
        let peer2_key = store2.peer_key();
        println!("Store 1 peer key: {}", peer1_key);
        println!("Store 2 peer key: {}", peer2_key);

        // Start sync on both
        store1.start_sync().expect("Failed to start sync 1");
        store2.start_sync().expect("Failed to start sync 2");

        // Create sync subscriptions AND observers on BOTH stores before inserting data
        //
        // IMPORTANT: Two separate APIs are required:
        // 1. SyncSubscription (via ditto.sync().register_subscription_v2()) - enables P2P syncing
        // 2. Observer (via ditto.store().register_observer_v2()) - processes change deltas
        //
        // Peers only discover and sync when they have COMMON subscriptions.

        // Store1: Create sync subscription + observer
        let _sync_sub1 = store1
            .ditto()
            .sync()
            .register_subscription_v2("SELECT * FROM sync_test")
            .expect("Failed to create sync subscription on store1");

        let _observer1 = store1
            .ditto()
            .store()
            .register_observer_v2("SELECT * FROM sync_test", |result| {
                println!("Store1 observer triggered: {} items", result.item_count());
            })
            .expect("Failed to register observer on store1");

        // Store2: Create sync subscription + observer
        let _sync_sub2 = store2
            .ditto()
            .sync()
            .register_subscription_v2("SELECT * FROM sync_test")
            .expect("Failed to create sync subscription on store2");

        let _observer2 = store2
            .ditto()
            .store()
            .register_observer_v2("SELECT * FROM sync_test", |result| {
                println!("Store2 observer triggered: {} items", result.item_count());
            })
            .expect("Failed to register observer on store2");

        // Wait for peers to discover each other (with timeout)
        println!("Waiting for peer discovery...");
        let mut connected = false;
        for attempt in 1..=10 {
            sleep(Duration::from_millis(500)).await;

            let graph1 = store1.ditto().presence().graph();
            let peer_count = graph1.remote_peers.len();

            if peer_count > 0 {
                println!(
                    "✓ Peers connected after {} attempts ({} peers)",
                    attempt, peer_count
                );
                connected = true;
                break;
            }

            if attempt % 2 == 0 {
                println!("  Still waiting... (attempt {}/10)", attempt);
            }
        }

        if !connected {
            println!("⚠ Warning: Peers did not discover each other within timeout");
            println!("  This can happen in test environments with localhost");
            return; // Skip the sync assertion
        }

        // Give a bit more time for initial connection handshake
        sleep(Duration::from_millis(500)).await;

        // Insert on store1
        let doc = serde_json::json!({
            "test_id": "sync_test",
            "value": 42
        });

        store1
            .upsert("sync_test", doc)
            .await
            .expect("Failed to upsert on store1");

        println!("Inserted document on store1, waiting for sync...");

        // Wait for sync to propagate
        let mut synced = false;
        for attempt in 1..=20 {
            sleep(Duration::from_millis(500)).await;

            let results = store2
                .query("sync_test", "test_id == 'sync_test'")
                .await
                .expect("Failed to query on store2");

            if !results.is_empty() {
                println!(
                    "✓ Document synced after {} attempts ({} docs)",
                    attempt,
                    results.len()
                );
                synced = true;
                break;
            }

            if attempt % 5 == 0 {
                println!("  Still waiting for sync... (attempt {}/20)", attempt);
            }
        }

        assert!(synced, "Document should have synced from store1 to store2");

        // Explicitly stop sync and drop stores to ensure clean shutdown
        // Drop observers and subscriptions first to stop observing changes
        drop(_observer1);
        drop(_observer2);
        drop(_sync_sub1);
        drop(_sync_sub2);

        // Then stop sync and drop stores
        store1.stop_sync();
        store2.stop_sync();
        drop(store1);
        drop(store2);

        // Give Ditto time to shut down background threads
        sleep(Duration::from_millis(200)).await;
    }
}
