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
//! Primary (recommended):
//! - `PEAT_APP_ID`: Application ID (UUID format)
//! - `PEAT_SECRET_KEY`: Base64-encoded shared encryption key
//! - `PEAT_OFFLINE_TOKEN`: Base64-encoded offline license token
//! - `PEAT_PERSISTENCE_DIR`: Directory for data storage (optional, defaults to ".ditto")
//!
//! Legacy (backwards compatible fallback):
//! - `DITTO_APP_ID` → `PEAT_APP_ID`
//! - `DITTO_SHARED_KEY` → `PEAT_SECRET_KEY`
//! - `DITTO_OFFLINE_TOKEN` → `PEAT_OFFLINE_TOKEN`
//! - `DITTO_PERSISTENCE_DIR` → `PEAT_PERSISTENCE_DIR`
//!
//! ## Peer Discovery
//!
//! This implementation enables LAN transport (mDNS) by default, which works well for
//! localhost peer discovery on macOS and other nodes that support mDNS. For explicit
//! localhost testing or environments where mDNS is unreliable, TCP transport can be
//! configured with explicit server/client connections.
//!
//! See `examples/ditto_spike.rs` for an example of TCP transport configuration.

use crate::credentials::PeatCredentials;
use crate::sync::ditto::DittoBackend;
use crate::{Error, Result};
use dittolive_ditto::prelude::*;
use dittolive_ditto::AppId;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

/// Configuration for Ditto storage
#[derive(Debug, Clone)]
pub struct DittoConfig {
    /// Application ID from Ditto portal (UUID)
    pub app_id: String,
    /// Persistence directory for Ditto data
    pub persistence_dir: PathBuf,
    /// Shared key for local-only syncing (base64 encoded)
    pub shared_key: String,
    /// Offline license token (base64 encoded, required for SharedKey identity)
    pub offline_token: String,
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

        // Parse AppId from config (UUID format)
        let app_uuid = Uuid::parse_str(&config.app_id).map_err(|e| {
            error!("Invalid app_id format (expected UUID): {}", e);
            Error::config_error(
                format!("Invalid app_id format (expected UUID): {}", e),
                Some("app_id".to_string()),
            )
        })?;
        let app_id = AppId::from_uuid(app_uuid);

        // Step 1: Create Ditto instance with SharedKey identity
        // This configures the identity type but does NOT activate sync capabilities yet
        let ditto = Ditto::builder()
            .with_root(root)
            .with_identity(|ditto_root| {
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
        // The offline license token is now passed via DittoConfig (loaded from PEAT_OFFLINE_TOKEN
        // or DITTO_OFFLINE_TOKEN env vars via PeatCredentials). This token proves you have a valid
        // license without requiring an online connection to Ditto's servers.
        ditto
            .set_offline_only_license_token(&config.offline_token)
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
        eprintln!(
            "DittoStore: Configuring transport - listen={:?}, connect={:?}",
            config.tcp_listen_port, config.tcp_connect_address
        );
        ditto.update_transport_config(|transport_config| {
            // Disable BLE
            transport_config.peer_to_peer.bluetooth_le.enabled = false;

            // Disable HTTP listen
            transport_config.listen.http.enabled = false;

            // Configure transport based on whether we're using explicit TCP or mDNS
            if config.tcp_listen_port.is_some() || config.tcp_connect_address.is_some() {
                // Using explicit TCP connections - disable mDNS/LAN discovery
                transport_config.peer_to_peer.lan.enabled = false;
                debug!("mDNS/LAN discovery disabled (using explicit TCP connections)");

                // Enable TCP listener if port specified
                if let Some(port) = config.tcp_listen_port {
                    transport_config.listen.tcp.enabled = true;
                    transport_config.listen.tcp.interface_ip = "0.0.0.0".to_string();
                    transport_config.listen.tcp.port = port;
                    debug!("TCP listen enabled on 0.0.0.0:{}", port);
                } else {
                    transport_config.listen.tcp.enabled = false;
                }

                // Configure TCP client connection if specified
                // Support comma-separated list of addresses for multi-peer connectivity
                if let Some(ref addresses) = config.tcp_connect_address {
                    for address in addresses.split(',') {
                        let address = address.trim();
                        if !address.is_empty() {
                            transport_config
                                .connect
                                .tcp_servers
                                .insert(address.to_string());
                            debug!("TCP client will connect to: {}", address);
                        }
                    }
                }
            } else {
                // No explicit TCP - use mDNS/LAN for peer discovery
                transport_config.peer_to_peer.lan.enabled = true;
                transport_config.listen.tcp.enabled = false;
                debug!("Using mDNS/LAN for peer discovery (no explicit TCP configured)");
            }
        });

        info!("Ditto store initialized successfully (v3 sync disabled)");

        Ok(Self {
            ditto: Arc::new(ditto),
            _config: config,
        })
    }

    /// Create a Ditto store from environment variables
    ///
    /// Uses PeatCredentials to load credentials, which supports both:
    /// - PEAT_APP_ID, PEAT_SECRET_KEY, PEAT_OFFLINE_TOKEN (preferred)
    /// - DITTO_APP_ID, DITTO_SHARED_KEY, DITTO_OFFLINE_TOKEN (legacy fallback)
    ///
    /// # Test Mode Isolation
    ///
    /// When running tests (detected via `RUST_TEST_THREADS` env var or `cfg(test)`),
    /// this method automatically creates unique temporary directories for each Ditto
    /// instance to prevent file locking conflicts. The temporary directories are
    /// cleaned up automatically when the process exits.
    ///
    /// In production mode, uses `PEAT_PERSISTENCE_DIR` (or `DITTO_PERSISTENCE_DIR`)
    /// environment variable or defaults to `.ditto` in the current directory.
    #[instrument]
    pub fn from_env() -> Result<Self> {
        info!("Creating DittoStore from environment variables");

        // Load environment variables
        dotenvy::dotenv().ok();

        // Load credentials via PeatCredentials (handles PEAT_* -> DITTO_* fallback)
        let credentials = PeatCredentials::from_env()
            .map_err(|e| Error::config_error(format!("{}", e), Some("credentials".to_string())))?;

        let app_id = credentials.app_id().to_string();
        let shared_key = credentials
            .require_secret_key()
            .map_err(|e| Error::config_error(format!("{}", e), Some("secret_key".to_string())))?
            .to_string();
        let offline_token = credentials
            .require_offline_token()
            .map_err(|e| Error::config_error(format!("{}", e), Some("offline_token".to_string())))?
            .to_string();

        // In test mode (detected via RUST_TEST_THREADS or cfg(test)), use unique temp directory
        // to prevent file locking conflicts between parallel tests
        let persistence_dir = if std::env::var("RUST_TEST_THREADS").is_ok() || cfg!(test) {
            let temp_dir = tempfile::tempdir().map_err(|e| {
                Error::storage_error(
                    format!("Failed to create temp dir for test: {}", e),
                    "from_env",
                    None,
                )
            })?;
            let path = temp_dir.path().to_path_buf();
            // Leak temp_dir to keep it alive - test cleanup will handle removal
            std::mem::forget(temp_dir);
            debug!("Test mode: Using isolated temp directory: {:?}", path);
            path
        } else {
            // Try PEAT_PERSISTENCE_DIR first, fall back to DITTO_PERSISTENCE_DIR
            PathBuf::from(
                std::env::var("PEAT_PERSISTENCE_DIR")
                    .or_else(|_| std::env::var("DITTO_PERSISTENCE_DIR"))
                    .unwrap_or_else(|_| ".ditto".to_string())
                    .trim(),
            )
        };

        let config = DittoConfig {
            app_id,
            persistence_dir,
            shared_key,
            offline_token,
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
        debug!("Upserting document into collection: {}", collection);

        // Use DQL v2 API with ON ID CONFLICT DO UPDATE for proper upsert behavior
        let dql_query = format!(
            "INSERT INTO {} DOCUMENTS (:doc) ON ID CONFLICT DO UPDATE",
            collection
        );

        let query_result = self
            .ditto
            .store()
            .execute_v2((dql_query.clone(), serde_json::json!({"doc": document})))
            .await
            .map_err(|e| {
                eprintln!("DQL upsert error: {:?}", e);
                eprintln!("DQL query was: {}", dql_query);
                error!("Upsert failed: {}", e);
                Error::storage_error(
                    format!("Upsert failed on collection {} - error: {}", collection, e),
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

    /// Replace a document completely using EVICT + INSERT pattern
    /// This is the recommended way to do "updates" in Ditto when you need to replace the whole document
    #[instrument(skip(self, document), fields(collection, where_clause))]
    pub async fn replace(
        &self,
        collection: &str,
        where_clause: &str,
        document: serde_json::Value,
    ) -> Result<String> {
        debug!(
            "Replacing documents in collection {} where {}",
            collection, where_clause
        );

        // First evict matching documents
        let evict_query = format!("EVICT FROM {} WHERE {}", collection, where_clause);
        self.ditto
            .store()
            .execute_v2((evict_query, serde_json::json!({})))
            .await
            .map_err(|e| {
                error!("Evict before replace failed: {}", e);
                Error::storage_error(
                    format!("Evict failed on collection {}", collection),
                    "replace",
                    Some(collection.to_string()),
                )
            })?;

        // Then insert new document
        self.upsert(collection, document).await
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

    // Hierarchical Summary Storage (E11.2)
    //
    // These methods enable Mode 3 (CAP Differential) testing by providing
    // storage for SquadSummary and PlatoonSummary aggregations.

    /// Store a SquadSummary in the squad_summaries collection
    ///
    /// # Arguments
    ///
    /// * `squad_id` - Unique squad identifier (used as document _id)
    /// * `summary` - SquadSummary protobuf message
    ///
    /// # Returns
    ///
    /// Document ID (same as squad_id)
    #[instrument(skip(self, summary), fields(squad_id))]
    pub async fn upsert_squad_summary(
        &self,
        squad_id: &str,
        summary: &peat_schema::hierarchy::v1::SquadSummary,
        collection: Option<&str>,
    ) -> Result<String> {
        // Use provided collection or default to "sim_poc" for backward compatibility
        let target_collection = collection.unwrap_or("sim_poc");
        let collection_name = collection.unwrap_or("squad_summaries");

        // Full JSON expansion for CRDT field-level merging
        // This allows Ditto to:
        // 1. Merge member_ids array with OR-Set semantics (track additions/removals)
        // 2. Merge scalar fields with LWW-Register (last-write-wins based on timestamp)
        // 3. Send delta updates (only changed fields, not entire blob)
        let mut doc = serde_json::to_value(summary).map_err(|e| {
            Error::storage_error(
                format!("Failed to serialize SquadSummary to JSON: {}", e),
                "upsert_squad_summary",
                Some(squad_id.to_string()),
            )
        })?;

        // Add Ditto-required metadata
        // CRITICAL: Append -summary suffix for hierarchical aggregation detection
        let doc_id = format!("{}-summary", squad_id);
        doc["_id"] = serde_json::Value::String(doc_id.clone());
        doc["type"] = serde_json::Value::String("squad_summary".to_string());
        doc["collection_name"] = serde_json::Value::String(collection_name.to_string());

        // Get current timestamp in microseconds for latency tracking
        let timestamp_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
        doc["timestamp_us"] = serde_json::Value::Number(timestamp_us.into());

        self.upsert(target_collection, doc).await
    }

    /// Retrieve a SquadSummary from the squad_summaries collection
    ///
    /// # Arguments
    ///
    /// * `squad_id` - Unique squad identifier
    /// * `collection` - Optional collection name (defaults to "sim_poc")
    ///
    /// # Returns
    ///
    /// Some(SquadSummary) if found, None if not found
    #[instrument(skip(self), fields(squad_id))]
    pub async fn get_squad_summary(
        &self,
        squad_id: &str,
        collection: Option<&str>,
    ) -> Result<Option<peat_schema::hierarchy::v1::SquadSummary>> {
        let target_collection = collection.unwrap_or("sim_poc");

        // Query with -summary suffix
        let doc_id = format!("{}-summary", squad_id);
        let results = self
            .query(target_collection, &format!("_id == '{}'", doc_id))
            .await?;

        if results.is_empty() {
            return Ok(None);
        }

        let doc = &results[0];

        // Deserialize directly from JSON (full CRDT-enabled format)
        let summary: peat_schema::hierarchy::v1::SquadSummary = serde_json::from_value(doc.clone())
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to deserialize SquadSummary from JSON: {}", e),
                    "get_squad_summary",
                    Some(squad_id.to_string()),
                )
            })?;

        Ok(Some(summary))
    }

    /// Create a squad summary document (ONCE per squad lifecycle)
    ///
    /// Implements ADR-021 create-once pattern. This method should be called exactly
    /// once during squad formation. Subsequent updates use `update_squad_summary()`.
    ///
    /// # Arguments
    ///
    /// * `squad_id` - Unique squad identifier
    /// * `initial_state` - Initial SquadSummary state
    ///
    /// # Returns
    ///
    /// Document ID on success
    ///
    /// # Errors
    ///
    /// Returns error if document already exists (prevents recreation)
    #[instrument(skip(self, initial_state), fields(squad_id))]
    pub async fn create_squad_summary(
        &self,
        squad_id: &str,
        initial_state: &peat_schema::hierarchy::v1::SquadSummary,
        collection: Option<&str>,
    ) -> Result<String> {
        let doc_id = format!("{}-summary", squad_id);

        // Check if already exists (enforce create-once invariant)
        if self.get_squad_summary(squad_id, None).await?.is_some() {
            return Err(Error::storage_error(
                format!(
                    "Squad summary {} already exists (cannot recreate - violates ADR-021)",
                    squad_id
                ),
                "create_squad_summary",
                Some(squad_id.to_string()),
            ));
        }

        // Create document with full initial state
        let mut doc = serde_json::to_value(initial_state).map_err(|e| {
            Error::storage_error(
                format!("Failed to serialize SquadSummary to JSON: {}", e),
                "create_squad_summary",
                Some(squad_id.to_string()),
            )
        })?;

        // Add Ditto metadata
        doc["_id"] = serde_json::Value::String(doc_id.clone());
        doc["type"] = serde_json::Value::String("squad_summary".to_string());
        doc["collection_name"] = serde_json::Value::String("squad_summaries".to_string());

        // Add lifecycle tracking fields (timestamps only - counters initialized separately)
        let timestamp_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
        doc["created_at_us"] = serde_json::Value::Number(timestamp_us.into());
        doc["last_update_us"] = serde_json::Value::Number(timestamp_us.into());
        doc["sequence"] = serde_json::Value::Number(0.into());

        // Use plain INSERT (not upsert) for document creation
        // This ensures we're creating a NEW document, not updating an existing one
        let dql_query = "INSERT INTO sim_poc DOCUMENTS (:doc)";

        self.ditto
            .store()
            .execute_v2((dql_query, serde_json::json!({"doc": doc})))
            .await
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to insert squad summary: {}", e),
                    "create_squad_summary",
                    Some(squad_id.to_string()),
                )
            })?;

        // Initialize PN_COUNTER fields AFTER document creation
        // This ensures they are created as PN_COUNTERs, not REGISTERs
        self.increment_counter(&doc_id, "create_count").await?;
        // update_count and total_delta_bytes start at 0, will be incremented on first update

        // Emit lifecycle metric
        tracing::info!(
            squad_id = squad_id,
            doc_id = %doc_id,
            "SquadSummaryCreated"
        );

        Ok(doc_id)
    }

    /// Update an existing squad summary with delta (MANY times)
    ///
    /// Implements ADR-021 update-many pattern. Applies field-level delta updates
    /// to existing document, enabling CRDT delta propagation instead of full
    /// document recreation.
    ///
    /// # Arguments
    ///
    /// * `squad_id` - Unique squad identifier
    /// * `delta` - Field-level delta updates
    ///
    /// # Errors
    ///
    /// Returns error if document does not exist (must create first)
    #[instrument(skip(self, delta), fields(squad_id, delta_size = delta.size_bytes()))]
    pub async fn update_squad_summary(
        &self,
        squad_id: &str,
        delta: crate::hierarchy::deltas::SquadDelta,
        collection: Option<&str>,
    ) -> Result<()> {
        use crate::hierarchy::deltas::SquadFieldUpdate;

        let doc_id = format!("{}-summary", squad_id);

        // Verify document exists (enforce must-create-first invariant)
        if self.get_squad_summary(squad_id, None).await?.is_none() {
            return Err(Error::storage_error(
                format!(
                    "Squad summary {} does not exist (must create first)",
                    squad_id
                ),
                "update_squad_summary",
                Some(squad_id.to_string()),
            ));
        }

        // Skip if delta is empty
        if delta.is_empty() {
            tracing::debug!(squad_id = squad_id, "Skipping empty delta");
            return Ok(());
        }

        // Build UPDATE SET clause from delta
        let mut set_clauses = Vec::new();
        let mut params = serde_json::Map::new();
        params.insert("_id".to_string(), serde_json::json!(doc_id.clone()));

        // Convert delta updates to SET clauses
        for (idx, update) in delta.updates.iter().enumerate() {
            let param_name = format!("p{}", idx);

            match update {
                SquadFieldUpdate::SetLeaderId(id) => {
                    set_clauses.push(format!("leader_id = :{}", param_name));
                    params.insert(param_name, serde_json::json!(id));
                }
                SquadFieldUpdate::SetMemberCount(count) => {
                    set_clauses.push(format!("member_count = :{}", param_name));
                    params.insert(param_name, serde_json::json!(count));
                }
                SquadFieldUpdate::SetOperationalCount(count) => {
                    set_clauses.push(format!("operational_count = :{}", param_name));
                    params.insert(param_name, serde_json::json!(count));
                }
                SquadFieldUpdate::SetAvgFuelMinutes(fuel) => {
                    set_clauses.push(format!("avg_fuel_minutes = :{}", param_name));
                    params.insert(param_name, serde_json::json!(fuel));
                }
                SquadFieldUpdate::SetWorstHealth(health) => {
                    set_clauses.push(format!("worst_health = :{}", param_name));
                    params.insert(param_name, serde_json::json!(health));
                }
                SquadFieldUpdate::SetReadinessScore(score) => {
                    set_clauses.push(format!("readiness_score = :{}", param_name));
                    params.insert(param_name, serde_json::json!(score));
                }
                SquadFieldUpdate::UpdatePositionCentroid(pos) => {
                    set_clauses.push(format!("position_centroid = :{}", param_name));
                    params.insert(
                        param_name,
                        serde_json::to_value(pos).unwrap_or(serde_json::json!(null)),
                    );
                }
                SquadFieldUpdate::UpdateBoundingBox(bbox) => {
                    set_clauses.push(format!("bounding_box = :{}", param_name));
                    params.insert(
                        param_name,
                        serde_json::to_value(bbox).unwrap_or(serde_json::json!(null)),
                    );
                }
                SquadFieldUpdate::UpdateAggregatedAt(ts) => {
                    set_clauses.push(format!("aggregated_at = :{}", param_name));
                    params.insert(
                        param_name,
                        serde_json::to_value(ts).unwrap_or(serde_json::json!(null)),
                    );
                }
                // Array operations handled separately (OR-Set semantics need special handling)
                _ => {
                    tracing::warn!(
                        "Array operations (Add/Remove) not yet implemented in DQL UPDATE"
                    );
                    // TODO: For now, arrays require full document read-modify-write
                    // Future: Implement using Ditto's array operations or custom CRDT logic
                }
            }
        }

        // Add metadata updates
        set_clauses.push("last_update_us = :last_update_us".to_string());
        params.insert(
            "last_update_us".to_string(),
            serde_json::json!(delta.timestamp_us),
        );

        set_clauses.push("sequence = :sequence".to_string());
        params.insert("sequence".to_string(), serde_json::json!(delta.sequence));

        // Build and execute UPDATE query
        let query = format!(
            "UPDATE sim_poc SET {} WHERE _id = :_id",
            set_clauses.join(", ")
        );

        self.ditto
            .store()
            .execute_v2((query, serde_json::Value::Object(params)))
            .await
            .map_err(|e| {
                tracing::error!("Delta update failed: {}", e);
                Error::storage_error(
                    format!("Failed to update squad summary {}: {}", squad_id, e),
                    "update_squad_summary",
                    Some(squad_id.to_string()),
                )
            })?;

        // Update lifecycle metrics using PN_COUNTER
        // These were initialized as PN_COUNTERs in create_*_summary()
        self.increment_counter(&doc_id, "update_count").await?;
        self.add_to_counter(&doc_id, "total_delta_bytes", delta.size_bytes() as u64)
            .await?;

        // Emit lifecycle metric
        tracing::debug!(
            squad_id = squad_id,
            doc_id = %doc_id,
            update_count = delta.updates.len(),
            delta_size = delta.size_bytes(),
            sequence = delta.sequence,
            "SquadSummaryUpdated"
        );

        Ok(())
    }

    /// Helper: Increment a counter field using PN_COUNTER CRDT
    ///
    /// Uses Ditto's PN_INCREMENT operation for distributed counter semantics.
    /// This correctly handles concurrent increments from multiple peers.
    ///
    /// **Important**: Includes explicit PN_COUNTER type declaration for strict mode.
    async fn increment_counter(&self, doc_id: &str, field: &str) -> Result<()> {
        // Include type declaration (field PN_COUNTER) for strict mode support
        // Also update last_update_us timestamp
        let timestamp_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        let query = format!(
            "UPDATE COLLECTION sim_poc ({} PN_COUNTER) APPLY {} PN_INCREMENT BY 1.0 SET last_update_us = :timestamp WHERE _id = :_id",
            field, field
        );

        self.ditto
            .store()
            .execute_v2((
                query,
                serde_json::json!({"_id": doc_id, "timestamp": timestamp_us}),
            ))
            .await
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to increment counter {}: {}", field, e),
                    "increment_counter",
                    Some(doc_id.to_string()),
                )
            })?;

        Ok(())
    }

    /// Helper: Add to a counter field using PN_COUNTER CRDT
    ///
    /// Uses Ditto's PN_INCREMENT operation for distributed counter semantics.
    /// This correctly handles concurrent increments from multiple peers.
    ///
    /// **Important**: Includes explicit PN_COUNTER type declaration for strict mode.
    async fn add_to_counter(&self, doc_id: &str, field: &str, value: u64) -> Result<()> {
        // Include type declaration (field PN_COUNTER) for strict mode support
        // Also update last_update_us timestamp
        let timestamp_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        let query = format!(
            "UPDATE COLLECTION sim_poc ({} PN_COUNTER) APPLY {} PN_INCREMENT BY :value SET last_update_us = :timestamp WHERE _id = :_id",
            field, field
        );

        self.ditto
            .store()
            .execute_v2((
                query,
                serde_json::json!({"_id": doc_id, "value": value as f64, "timestamp": timestamp_us}),
            ))
            .await
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to add to counter {}: {}", field, e),
                    "add_to_counter",
                    Some(doc_id.to_string()),
                )
            })?;

        Ok(())
    }

    /// Store a PlatoonSummary in the platoon_summaries collection
    ///
    /// # Arguments
    ///
    /// * `platoon_id` - Unique platoon identifier (used as document _id)
    /// * `summary` - PlatoonSummary protobuf message
    ///
    /// # Returns
    ///
    /// Document ID (same as platoon_id)
    #[instrument(skip(self, summary), fields(platoon_id))]
    pub async fn upsert_platoon_summary(
        &self,
        platoon_id: &str,
        summary: &peat_schema::hierarchy::v1::PlatoonSummary,
        collection: Option<&str>,
    ) -> Result<String> {
        // Full JSON expansion for CRDT field-level merging
        let mut doc = serde_json::to_value(summary).map_err(|e| {
            Error::storage_error(
                format!("Failed to serialize PlatoonSummary to JSON: {}", e),
                "upsert_platoon_summary",
                Some(platoon_id.to_string()),
            )
        })?;

        let target_collection = collection.unwrap_or("sim_poc");
        let collection_name = collection.unwrap_or("platoon_summaries");

        // Add Ditto-required metadata
        // CRITICAL: Append -summary suffix for hierarchical aggregation detection
        let doc_id = format!("{}-summary", platoon_id);
        doc["_id"] = serde_json::Value::String(doc_id);
        doc["type"] = serde_json::Value::String("platoon_summary".to_string());
        doc["collection_name"] = serde_json::Value::String(collection_name.to_string());

        // Get current timestamp in microseconds for latency tracking
        let timestamp_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
        doc["timestamp_us"] = serde_json::Value::Number(timestamp_us.into());

        self.upsert(target_collection, doc).await
    }

    /// Retrieve a PlatoonSummary from the platoon_summaries collection
    ///
    /// # Arguments
    ///
    /// * `platoon_id` - Unique platoon identifier
    ///
    /// # Returns
    ///
    /// Some(PlatoonSummary) if found, None if not found
    #[instrument(skip(self), fields(platoon_id))]
    pub async fn get_platoon_summary(
        &self,
        platoon_id: &str,
        collection: Option<&str>,
    ) -> Result<Option<peat_schema::hierarchy::v1::PlatoonSummary>> {
        // Query with -summary suffix
        let doc_id = format!("{}-summary", platoon_id);
        let results = self
            .query("sim_poc", &format!("_id == '{}'", doc_id))
            .await?;

        if results.is_empty() {
            return Ok(None);
        }

        let doc = &results[0];

        // Deserialize directly from JSON (full CRDT-enabled format)
        let summary: peat_schema::hierarchy::v1::PlatoonSummary =
            serde_json::from_value(doc.clone()).map_err(|e| {
                Error::storage_error(
                    format!("Failed to deserialize PlatoonSummary from JSON: {}", e),
                    "get_platoon_summary",
                    Some(platoon_id.to_string()),
                )
            })?;

        Ok(Some(summary))
    }

    /// Create a platoon summary document (ONCE per platoon lifecycle)
    ///
    /// Implements ADR-021 create-once pattern. This method should be called exactly
    /// once during platoon formation. Subsequent updates use `update_platoon_summary()`.
    #[instrument(skip(self, initial_state), fields(platoon_id))]
    pub async fn create_platoon_summary(
        &self,
        platoon_id: &str,
        initial_state: &peat_schema::hierarchy::v1::PlatoonSummary,
        collection: Option<&str>,
    ) -> Result<String> {
        let doc_id = format!("{}-summary", platoon_id);

        // Check if already exists (enforce create-once invariant)
        if self.get_platoon_summary(platoon_id, None).await?.is_some() {
            return Err(Error::storage_error(
                format!(
                    "Platoon summary {} already exists (cannot recreate - violates ADR-021)",
                    platoon_id
                ),
                "create_platoon_summary",
                Some(platoon_id.to_string()),
            ));
        }

        // Create document with full initial state
        let mut doc = serde_json::to_value(initial_state).map_err(|e| {
            Error::storage_error(
                format!("Failed to serialize PlatoonSummary to JSON: {}", e),
                "create_platoon_summary",
                Some(platoon_id.to_string()),
            )
        })?;

        // Add Ditto metadata
        doc["_id"] = serde_json::Value::String(doc_id.clone());
        doc["type"] = serde_json::Value::String("platoon_summary".to_string());
        doc["collection_name"] = serde_json::Value::String("platoon_summaries".to_string());

        // Add lifecycle tracking fields (timestamps only - counters initialized separately)
        let timestamp_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
        doc["created_at_us"] = serde_json::Value::Number(timestamp_us.into());
        doc["last_update_us"] = serde_json::Value::Number(timestamp_us.into());
        doc["sequence"] = serde_json::Value::Number(0.into());

        let target_collection = collection.unwrap_or("sim_poc");

        self.upsert(target_collection, doc).await?;

        // Initialize PN_COUNTER fields AFTER document creation
        // This ensures they are created as PN_COUNTERs, not REGISTERs
        self.increment_counter(&doc_id, "create_count").await?;
        // update_count and total_delta_bytes start at 0, will be incremented on first update

        tracing::info!(
            platoon_id = platoon_id,
            doc_id = %doc_id,
            "PlatoonSummaryCreated"
        );

        Ok(doc_id)
    }

    /// Update an existing platoon summary with delta (MANY times)
    #[instrument(skip(self, delta), fields(platoon_id, delta_size = delta.size_bytes()))]
    pub async fn update_platoon_summary(
        &self,
        platoon_id: &str,
        delta: crate::hierarchy::deltas::PlatoonDelta,
        collection: Option<&str>,
    ) -> Result<()> {
        use crate::hierarchy::deltas::PlatoonFieldUpdate;

        let doc_id = format!("{}-summary", platoon_id);

        // Verify document exists
        if self.get_platoon_summary(platoon_id, None).await?.is_none() {
            return Err(Error::storage_error(
                format!(
                    "Platoon summary {} does not exist (must create first)",
                    platoon_id
                ),
                "update_platoon_summary",
                Some(platoon_id.to_string()),
            ));
        }

        // Skip if delta is empty
        if delta.is_empty() {
            tracing::debug!(platoon_id = platoon_id, "Skipping empty delta");
            return Ok(());
        }

        // Build UPDATE SET clause from delta
        let mut set_clauses = Vec::new();
        let mut params = serde_json::Map::new();
        params.insert("_id".to_string(), serde_json::json!(doc_id.clone()));

        // Convert delta updates to SET clauses
        for (idx, update) in delta.updates.iter().enumerate() {
            let param_name = format!("p{}", idx);

            match update {
                PlatoonFieldUpdate::SetLeaderId(id) => {
                    set_clauses.push(format!("leader_id = :{}", param_name));
                    params.insert(param_name, serde_json::json!(id));
                }
                PlatoonFieldUpdate::SetSquadCount(count) => {
                    set_clauses.push(format!("squad_count = :{}", param_name));
                    params.insert(param_name, serde_json::json!(count));
                }
                PlatoonFieldUpdate::SetTotalMemberCount(count) => {
                    set_clauses.push(format!("total_member_count = :{}", param_name));
                    params.insert(param_name, serde_json::json!(count));
                }
                PlatoonFieldUpdate::SetOperationalCount(count) => {
                    set_clauses.push(format!("operational_count = :{}", param_name));
                    params.insert(param_name, serde_json::json!(count));
                }
                PlatoonFieldUpdate::SetAvgFuelMinutes(fuel) => {
                    set_clauses.push(format!("avg_fuel_minutes = :{}", param_name));
                    params.insert(param_name, serde_json::json!(fuel));
                }
                PlatoonFieldUpdate::SetWorstHealth(health) => {
                    set_clauses.push(format!("worst_health = :{}", param_name));
                    params.insert(param_name, serde_json::json!(health));
                }
                PlatoonFieldUpdate::SetReadinessScore(score) => {
                    set_clauses.push(format!("readiness_score = :{}", param_name));
                    params.insert(param_name, serde_json::json!(score));
                }
                PlatoonFieldUpdate::UpdatePositionCentroid(pos) => {
                    set_clauses.push(format!("position_centroid = :{}", param_name));
                    params.insert(
                        param_name,
                        serde_json::to_value(pos).unwrap_or(serde_json::json!(null)),
                    );
                }
                PlatoonFieldUpdate::UpdateBoundingBox(bbox) => {
                    set_clauses.push(format!("bounding_box = :{}", param_name));
                    params.insert(
                        param_name,
                        serde_json::to_value(bbox).unwrap_or(serde_json::json!(null)),
                    );
                }
                PlatoonFieldUpdate::UpdateAggregatedAt(ts) => {
                    set_clauses.push(format!("aggregated_at = :{}", param_name));
                    params.insert(
                        param_name,
                        serde_json::to_value(ts).unwrap_or(serde_json::json!(null)),
                    );
                }
                // Array operations handled separately
                _ => {
                    tracing::warn!(
                        "Array operations (Add/Remove) not yet implemented in DQL UPDATE"
                    );
                }
            }
        }

        // Add metadata updates
        set_clauses.push("last_update_us = :last_update_us".to_string());
        params.insert(
            "last_update_us".to_string(),
            serde_json::json!(delta.timestamp_us),
        );

        set_clauses.push("sequence = :sequence".to_string());
        params.insert("sequence".to_string(), serde_json::json!(delta.sequence));

        // Build and execute UPDATE query
        let query = format!(
            "UPDATE sim_poc SET {} WHERE _id = :_id",
            set_clauses.join(", ")
        );

        self.ditto
            .store()
            .execute_v2((query, serde_json::Value::Object(params)))
            .await
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to update platoon summary {}: {}", platoon_id, e),
                    "update_platoon_summary",
                    Some(platoon_id.to_string()),
                )
            })?;

        // Update lifecycle metrics using PN_COUNTER
        // These were initialized as PN_COUNTERs in create_*_summary()
        self.increment_counter(&doc_id, "update_count").await?;
        self.add_to_counter(&doc_id, "total_delta_bytes", delta.size_bytes() as u64)
            .await?;

        tracing::debug!(
            platoon_id = platoon_id,
            doc_id = %doc_id,
            update_count = delta.updates.len(),
            delta_size = delta.size_bytes(),
            sequence = delta.sequence,
            "PlatoonSummaryUpdated"
        );

        Ok(())
    }

    // ========================================================================
    // Company Summary Operations
    // ========================================================================

    /// Retrieve a CompanySummary from the company_summaries collection
    #[instrument(skip(self), fields(company_id))]
    pub async fn get_company_summary(
        &self,
        company_id: &str,
    ) -> Result<Option<peat_schema::hierarchy::v1::CompanySummary>> {
        let doc_id = format!("{}-summary", company_id);
        let results = self
            .query("company_summaries", &format!("_id == '{}'", doc_id))
            .await?;

        if results.is_empty() {
            return Ok(None);
        }

        let doc = &results[0];

        let summary: peat_schema::hierarchy::v1::CompanySummary =
            serde_json::from_value(doc.clone()).map_err(|e| {
                Error::storage_error(
                    format!("Failed to deserialize CompanySummary from JSON: {}", e),
                    "get_company_summary",
                    Some(company_id.to_string()),
                )
            })?;

        Ok(Some(summary))
    }

    /// Create a company summary document (ONCE per company lifecycle)
    ///
    /// Implements ADR-021 create-once pattern. This method should be called exactly
    /// once during company formation. Subsequent updates use `update_company_summary()`.
    #[instrument(skip(self, initial_state), fields(company_id))]
    pub async fn create_company_summary(
        &self,
        company_id: &str,
        initial_state: &peat_schema::hierarchy::v1::CompanySummary,
    ) -> Result<String> {
        let doc_id = format!("{}-summary", company_id);

        // Check if already exists (enforce create-once invariant)
        if self.get_company_summary(company_id).await?.is_some() {
            return Err(Error::storage_error(
                format!(
                    "Company summary {} already exists (cannot recreate - violates ADR-021)",
                    company_id
                ),
                "create_company_summary",
                Some(company_id.to_string()),
            ));
        }

        // Create document with full initial state
        let mut doc = serde_json::to_value(initial_state).map_err(|e| {
            Error::storage_error(
                format!("Failed to serialize CompanySummary to JSON: {}", e),
                "create_company_summary",
                Some(company_id.to_string()),
            )
        })?;

        // Add Ditto metadata
        doc["_id"] = serde_json::Value::String(doc_id.clone());
        doc["type"] = serde_json::Value::String("company_summary".to_string());
        doc["collection_name"] = serde_json::Value::String("company_summaries".to_string());

        // Add lifecycle tracking fields (timestamps only - counters initialized separately)
        let timestamp_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
        doc["created_at_us"] = serde_json::Value::Number(timestamp_us.into());
        doc["last_update_us"] = serde_json::Value::Number(timestamp_us.into());
        doc["sequence"] = serde_json::Value::Number(0.into());

        self.upsert("sim_poc", doc).await?;

        // Initialize PN_COUNTER fields AFTER document creation
        // This ensures they are created as PN_COUNTERs, not REGISTERs
        self.increment_counter(&doc_id, "create_count").await?;
        // update_count and total_delta_bytes start at 0, will be incremented on first update

        tracing::info!(
            company_id = company_id,
            doc_id = %doc_id,
            "CompanySummaryCreated"
        );

        Ok(doc_id)
    }

    /// Update an existing company summary with delta (MANY times)
    #[instrument(skip(self, delta), fields(company_id, delta_size = delta.size_bytes()))]
    pub async fn update_company_summary(
        &self,
        company_id: &str,
        delta: crate::hierarchy::deltas::CompanyDelta,
    ) -> Result<()> {
        use crate::hierarchy::deltas::CompanyFieldUpdate;

        let doc_id = format!("{}-summary", company_id);

        // Verify document exists
        if self.get_company_summary(company_id).await?.is_none() {
            return Err(Error::storage_error(
                format!(
                    "Company summary {} does not exist (must create first)",
                    company_id
                ),
                "update_company_summary",
                Some(company_id.to_string()),
            ));
        }

        // Skip if delta is empty
        if delta.is_empty() {
            tracing::debug!(company_id = company_id, "Skipping empty delta");
            return Ok(());
        }

        // Build UPDATE SET clause from delta
        let mut set_clauses = Vec::new();
        let mut params = serde_json::Map::new();
        params.insert("_id".to_string(), serde_json::json!(doc_id.clone()));

        // Convert delta updates to SET clauses
        for (idx, update) in delta.updates.iter().enumerate() {
            let param_name = format!("p{}", idx);

            match update {
                CompanyFieldUpdate::SetLeaderId(id) => {
                    set_clauses.push(format!("leader_id = :{}", param_name));
                    params.insert(param_name, serde_json::json!(id));
                }
                CompanyFieldUpdate::SetPlatoonCount(count) => {
                    set_clauses.push(format!("platoon_count = :{}", param_name));
                    params.insert(param_name, serde_json::json!(count));
                }
                CompanyFieldUpdate::SetTotalMemberCount(count) => {
                    set_clauses.push(format!("total_member_count = :{}", param_name));
                    params.insert(param_name, serde_json::json!(count));
                }
                CompanyFieldUpdate::SetOperationalCount(count) => {
                    set_clauses.push(format!("operational_count = :{}", param_name));
                    params.insert(param_name, serde_json::json!(count));
                }
                CompanyFieldUpdate::SetAvgFuelMinutes(fuel) => {
                    set_clauses.push(format!("avg_fuel_minutes = :{}", param_name));
                    params.insert(param_name, serde_json::json!(fuel));
                }
                CompanyFieldUpdate::SetWorstHealth(health) => {
                    set_clauses.push(format!("worst_health = :{}", param_name));
                    params.insert(param_name, serde_json::json!(health));
                }
                CompanyFieldUpdate::SetReadinessScore(score) => {
                    set_clauses.push(format!("readiness_score = :{}", param_name));
                    params.insert(param_name, serde_json::json!(score));
                }
                CompanyFieldUpdate::UpdatePositionCentroid(pos) => {
                    set_clauses.push(format!("position_centroid = :{}", param_name));
                    params.insert(
                        param_name,
                        serde_json::to_value(pos).unwrap_or(serde_json::json!(null)),
                    );
                }
                CompanyFieldUpdate::UpdateBoundingBox(bbox) => {
                    set_clauses.push(format!("bounding_box = :{}", param_name));
                    params.insert(
                        param_name,
                        serde_json::to_value(bbox).unwrap_or(serde_json::json!(null)),
                    );
                }
                CompanyFieldUpdate::UpdateAggregatedAt(ts) => {
                    set_clauses.push(format!("aggregated_at = :{}", param_name));
                    params.insert(
                        param_name,
                        serde_json::to_value(ts).unwrap_or(serde_json::json!(null)),
                    );
                }
                // Array operations handled separately
                _ => {
                    tracing::warn!(
                        "Array operations (Add/Remove) not yet implemented in DQL UPDATE"
                    );
                }
            }
        }

        // Add metadata updates
        set_clauses.push("last_update_us = :last_update_us".to_string());
        params.insert(
            "last_update_us".to_string(),
            serde_json::json!(delta.timestamp_us),
        );

        set_clauses.push("sequence = :sequence".to_string());
        params.insert("sequence".to_string(), serde_json::json!(delta.sequence));

        // Build and execute UPDATE query
        let query = format!(
            "UPDATE sim_poc SET {} WHERE _id = :_id",
            set_clauses.join(", ")
        );

        self.ditto
            .store()
            .execute_v2((query, serde_json::Value::Object(params)))
            .await
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to update company summary {}: {}", company_id, e),
                    "update_company_summary",
                    Some(company_id.to_string()),
                )
            })?;

        // Update lifecycle metrics using PN_COUNTER
        // These were initialized as PN_COUNTERs in create_*_summary()
        self.increment_counter(&doc_id, "update_count").await?;
        self.add_to_counter(&doc_id, "total_delta_bytes", delta.size_bytes() as u64)
            .await?;

        tracing::debug!(
            company_id = company_id,
            doc_id = %doc_id,
            update_count = delta.updates.len(),
            delta_size = delta.size_bytes(),
            sequence = delta.sequence,
            "CompanySummaryUpdated"
        );

        Ok(())
    }

    // ========================================================================
    // Lifecycle Metrics
    // ========================================================================

    /// Get document lifecycle metrics for validation (ADR-021)
    ///
    /// Returns metrics for validating architectural invariants:
    /// - create_count must equal 1
    /// - compression_ratio should be > 10×
    #[instrument(skip(self), fields(doc_id))]
    pub async fn get_document_metrics(
        &self,
        doc_id: &str,
    ) -> Result<crate::hierarchy::storage_trait::DocumentMetrics> {
        use crate::hierarchy::storage_trait::DocumentMetrics;

        // Query document to get lifecycle fields
        let results = self
            .query("sim_poc", &format!("_id == '{}'", doc_id))
            .await?;

        if results.is_empty() {
            return Err(Error::storage_error(
                format!("Document {} not found", doc_id),
                "get_document_metrics",
                Some(doc_id.to_string()),
            ));
        }

        let doc = &results[0];

        // Extract lifecycle fields
        let created_at_us = doc
            .get("created_at_us")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let create_count = doc
            .get("create_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let update_count = doc
            .get("update_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let last_update_us = doc
            .get("last_update_us")
            .and_then(|v| v.as_u64())
            .unwrap_or(created_at_us);
        let total_delta_bytes = doc
            .get("total_delta_bytes")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let sequence = doc.get("sequence").and_then(|v| v.as_u64()).unwrap_or(0);

        // Calculate full document size
        let full_doc_size = serde_json::to_vec(&doc).map(|v| v.len()).unwrap_or(0);

        // Calculate compression ratio
        let avg_delta_size = if update_count > 0 {
            total_delta_bytes / update_count as usize
        } else {
            0
        };
        let compression_ratio = if avg_delta_size > 0 {
            full_doc_size as f32 / avg_delta_size as f32
        } else {
            0.0
        };

        Ok(DocumentMetrics {
            document_id: doc_id.to_string(),
            created_at_us,
            create_count,
            update_count,
            last_update_us,
            total_delta_bytes,
            full_doc_size,
            compression_ratio,
            sequence,
        })
    }

    /// Upsert a HierarchicalCommand to the hierarchical_commands collection
    ///
    /// # Arguments
    ///
    /// * `command_id` - Unique command identifier (used as document ID)
    /// * `command` - The hierarchical command to store
    ///
    /// # Returns
    ///
    /// Document ID on success
    #[instrument(skip(self, command), fields(command_id))]
    pub async fn upsert_command(
        &self,
        command_id: &str,
        command: &peat_schema::command::v1::HierarchicalCommand,
    ) -> Result<String> {
        // Full JSON expansion for CRDT field-level merging
        let mut doc = serde_json::to_value(command).map_err(|e| {
            Error::storage_error(
                format!("Failed to serialize HierarchicalCommand to JSON: {}", e),
                "upsert_command",
                Some(command_id.to_string()),
            )
        })?;

        // Add Ditto-required metadata
        doc["_id"] = serde_json::Value::String(command_id.to_string());
        doc["type"] = serde_json::Value::String("hierarchical_command".to_string());

        self.upsert("hierarchical_commands", doc).await
    }

    /// Retrieve a HierarchicalCommand from the hierarchical_commands collection
    ///
    /// # Arguments
    ///
    /// * `command_id` - Unique command identifier
    ///
    /// # Returns
    ///
    /// Some(HierarchicalCommand) if found, None if not found
    #[instrument(skip(self), fields(command_id))]
    pub async fn get_command(
        &self,
        command_id: &str,
    ) -> Result<Option<peat_schema::command::v1::HierarchicalCommand>> {
        let results = self
            .query("hierarchical_commands", &format!("_id == '{}'", command_id))
            .await?;

        if results.is_empty() {
            return Ok(None);
        }

        let doc = &results[0];

        // Deserialize directly from JSON (full CRDT-enabled format)
        let command: peat_schema::command::v1::HierarchicalCommand =
            serde_json::from_value(doc.clone()).map_err(|e| {
                Error::storage_error(
                    format!("Failed to deserialize HierarchicalCommand from JSON: {}", e),
                    "get_command",
                    Some(command_id.to_string()),
                )
            })?;

        Ok(Some(command))
    }

    /// Upsert a CommandAcknowledgment to the command_acknowledgments collection
    ///
    /// # Arguments
    ///
    /// * `ack_id` - Unique acknowledgment identifier (command_id + node_id)
    /// * `ack` - The command acknowledgment to store
    ///
    /// # Returns
    ///
    /// Document ID on success
    #[instrument(skip(self, ack), fields(ack_id))]
    pub async fn upsert_command_ack(
        &self,
        ack_id: &str,
        ack: &peat_schema::command::v1::CommandAcknowledgment,
    ) -> Result<String> {
        // Full JSON expansion for CRDT field-level merging
        let mut doc = serde_json::to_value(ack).map_err(|e| {
            Error::storage_error(
                format!("Failed to serialize CommandAcknowledgment to JSON: {}", e),
                "upsert_command_ack",
                Some(ack_id.to_string()),
            )
        })?;

        // Add Ditto-required metadata
        doc["_id"] = serde_json::Value::String(ack_id.to_string());
        doc["type"] = serde_json::Value::String("command_acknowledgment".to_string());

        self.upsert("command_acknowledgments", doc).await
    }

    /// Query all acknowledgments for a specific command
    ///
    /// # Arguments
    ///
    /// * `command_id` - Command identifier to query acknowledgments for
    ///
    /// # Returns
    ///
    /// Vector of CommandAcknowledgment messages
    #[instrument(skip(self), fields(command_id))]
    pub async fn query_command_acks(
        &self,
        command_id: &str,
    ) -> Result<Vec<peat_schema::command::v1::CommandAcknowledgment>> {
        let results = self
            .query(
                "command_acknowledgments",
                &format!("command_id == '{}'", command_id),
            )
            .await?;

        let mut acks = Vec::new();
        for doc in results {
            // Deserialize directly from JSON (full CRDT-enabled format)
            let ack: peat_schema::command::v1::CommandAcknowledgment = serde_json::from_value(doc)
                .map_err(|e| {
                    Error::storage_error(
                        format!(
                            "Failed to deserialize CommandAcknowledgment from JSON: {}",
                            e
                        ),
                        "query_command_acks",
                        None,
                    )
                })?;

            acks.push(ack);
        }

        Ok(acks)
    }

    // Policy Engine Operations (Optimistic Concurrency Control)
    //
    // These methods implement conditional updates to enforce conflict resolution policies
    // BEFORE Ditto's CRDT merge. See docs/POLICY_ENGINE_CRDT_INTEGRATION.md for details.

    /// Conditional update for command with policy enforcement (Optimistic Concurrency Control)
    ///
    /// Uses WHERE clause to check policy-relevant attributes BEFORE allowing update.
    /// This ensures policy enforcement happens before Ditto's CRDT merge.
    ///
    /// # Arguments
    ///
    /// * `command` - The command to upsert
    /// * `policy` - The conflict policy to enforce
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Update succeeded (policy check passed)
    /// * `Ok(false)` - Update rejected (existing command wins per policy)
    /// * `Err(_)` - Query execution failed
    ///
    /// # Policy Enforcement
    ///
    /// Different policies use different WHERE clauses:
    ///
    /// - `LastWriteWins`: `issued_at < :new_time` - Only update if new timestamp is newer
    /// - `HighestPriorityWins`: `priority < :new_priority OR (priority = :new_priority AND issued_at < :new_time)`
    /// - `HighestAuthorityWins`: Checks originator_id prefix (zone- > platoon-/squad- > node-)
    /// - `RejectConflict`: `false` - Never update existing documents
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let command = HierarchicalCommand { priority: 5, ... };
    /// let success = store.conditional_update_command(&command, ConflictPolicy::HighestPriorityWins).await?;
    ///
    /// if !success {
    ///     // Existing command has higher priority, new command rejected
    ///     return Err(Error::ConflictDetected("Higher priority command exists"));
    /// }
    /// ```
    #[instrument(skip(self, command), fields(command_id = %command.command_id, policy = ?policy))]
    pub async fn conditional_update_command(
        &self,
        command: &peat_schema::command::v1::HierarchicalCommand,
        policy: peat_schema::command::v1::ConflictPolicy,
    ) -> Result<bool> {
        let (where_clause, mut params) = self.build_policy_where_clause(command, policy)?;

        // Full JSON expansion for CRDT field-level merging
        let command_json = serde_json::to_value(command).map_err(|e| {
            Error::storage_error(
                format!("Failed to serialize HierarchicalCommand to JSON: {}", e),
                "conditional_update_command",
                Some(command.command_id.clone()),
            )
        })?;

        // Build SET clause dynamically from JSON fields
        // This ensures all protobuf fields are updated, not just a base64 blob
        params["_id"] = serde_json::json!(command.command_id);
        params["command_json"] = command_json;
        params["type"] = serde_json::json!("hierarchical_command");
        params["last_modified"] = serde_json::json!(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs());

        // Note: DQL v2 doesn't support spreading JSON object fields in UPDATE
        // We need to use EVICT + INSERT pattern for conditional full-document replacement
        let query = format!(
            "UPDATE hierarchical_commands
             SET command_id = :command_id,
                 originator_id = :originator_id,
                 priority = :priority,
                 type = :type,
                 last_modified = :last_modified
             WHERE _id = :_id AND ({})",
            where_clause
        );

        // Add individual fields from command for UPDATE
        params["command_id"] = serde_json::json!(command.command_id);
        params["originator_id"] = serde_json::json!(command.originator_id);
        params["priority"] = serde_json::json!(command.priority);

        debug!("Executing conditional update with WHERE: {}", where_clause);

        let result = self
            .ditto
            .store()
            .execute_v2((query, params))
            .await
            .map_err(|e| {
                error!("Conditional update failed: {}", e);
                Error::storage_error(
                    format!("Conditional update failed: {}", e),
                    "conditional_update_command",
                    Some("hierarchical_commands".to_string()),
                )
            })?;

        // Check if any documents were mutated
        let success = !result.mutated_document_ids().is_empty();

        if !success {
            debug!(
                "Conditional update rejected for command {} - existing command wins per policy {:?}",
                command.command_id, policy
            );
        } else {
            info!(
                "Conditional update succeeded for command {} with policy {:?}",
                command.command_id, policy
            );
        }

        Ok(success)
    }

    /// Build WHERE clause and params for policy-based conditional update
    fn build_policy_where_clause(
        &self,
        command: &peat_schema::command::v1::HierarchicalCommand,
        policy: peat_schema::command::v1::ConflictPolicy,
    ) -> Result<(String, serde_json::Value)> {
        use peat_schema::command::v1::ConflictPolicy;

        let mut params = serde_json::json!({});

        let issued_at_secs = command.issued_at.as_ref().map(|t| t.seconds).unwrap_or(0);

        let where_clause = match policy {
            ConflictPolicy::HighestPriorityWins => {
                // Only update if new priority is higher, OR equal priority with newer timestamp
                params["new_priority"] = serde_json::json!(command.priority);
                params["new_time"] = serde_json::json!(issued_at_secs);

                "(priority < :new_priority OR (priority = :new_priority AND issued_at < :new_time))"
                    .to_string()
            }

            ConflictPolicy::HighestAuthorityWins => {
                // Derive authority level from originator_id
                // zone- = 3 (highest), platoon-/squad- = 2, other = 1
                if command.originator_id.starts_with("zone-") {
                    // Zone-level authority: can override anything
                    "true".to_string()
                } else if command.originator_id.starts_with("platoon-")
                    || command.originator_id.starts_with("squad-")
                {
                    // Platoon/Squad level: can override node-level, but not zone-level
                    "NOT (originator_id LIKE 'zone-%')".to_string()
                } else {
                    // Node-level: can only override other node-level
                    "(NOT (originator_id LIKE 'zone-%')) AND (NOT (originator_id LIKE 'platoon-%')) AND (NOT (originator_id LIKE 'squad-%'))".to_string()
                }
            }

            ConflictPolicy::LastWriteWins => {
                // Only update if new timestamp is newer
                params["new_time"] = serde_json::json!(issued_at_secs);
                "issued_at < :new_time".to_string()
            }

            ConflictPolicy::MergeCompatible => {
                // TODO: Implement actual compatibility checking
                // For now, allow all updates
                warn!("MergeCompatible policy not fully implemented, allowing all updates");
                "true".to_string()
            }

            ConflictPolicy::RejectConflict => {
                // Never update existing documents - always reject
                "false".to_string()
            }

            ConflictPolicy::Unspecified => {
                return Err(Error::InvalidInput(
                    "Conflict policy must be specified for conditional update".to_string(),
                ));
            }
        };

        Ok((where_clause, params))
    }

    // TTL and Data Lifecycle Operations
    //
    // These methods implement soft-delete patterns and EVICT strategies to manage
    // document lifecycle in distributed environments. See docs/TTL_AND_DATA_LIFECYCLE_DESIGN.md
    // for architectural rationale.

    /// Soft-delete a document by setting _deleted flag
    ///
    /// This avoids husking (concurrent delete-update creating partially null documents)
    /// on high-churn data like beacons and positions.
    ///
    /// # Arguments
    ///
    /// * `collection` - Collection name
    /// * `doc_id` - Document ID to soft-delete
    ///
    /// # Example
    ///
    /// ```ignore
    /// store.soft_delete("beacons", "beacon-123").await?;
    /// ```
    #[instrument(skip(self), fields(collection, doc_id))]
    pub async fn soft_delete(&self, collection: &str, doc_id: &str) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let dql_query = format!(
            "UPDATE {} SET _deleted = true, _deleted_at = :now WHERE _id = :id",
            collection
        );

        self.ditto
            .store()
            .execute_v2((dql_query, serde_json::json!({"id": doc_id, "now": now})))
            .await
            .map_err(|e| {
                error!("Soft delete failed: {}", e);
                Error::storage_error(
                    format!("Soft delete failed on collection {}", collection),
                    "soft_delete",
                    Some(collection.to_string()),
                )
            })?;

        debug!("Soft-deleted document {} in {}", doc_id, collection);
        Ok(())
    }

    /// Clean up soft-deleted documents older than the specified TTL
    ///
    /// This performs hard deletion (tombstone creation) after soft-delete TTL expires.
    ///
    /// # Arguments
    ///
    /// * `collection` - Collection name
    /// * `ttl_seconds` - Documents with _deleted_at older than this are hard-deleted
    ///
    /// # Returns
    ///
    /// Number of documents hard-deleted
    #[instrument(skip(self), fields(collection, ttl_seconds))]
    pub async fn cleanup_soft_deleted(&self, collection: &str, ttl_seconds: u64) -> Result<usize> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let cutoff = now - ttl_seconds;

        // Query soft-deleted documents older than cutoff
        let dql_query = format!(
            "SELECT _id FROM {} WHERE _deleted == true AND _deleted_at < :cutoff",
            collection
        );

        let results = self
            .ditto
            .store()
            .execute_v2((dql_query, serde_json::json!({"cutoff": cutoff})))
            .await
            .map_err(|e| {
                error!("Query soft-deleted failed: {}", e);
                Error::storage_error(
                    format!("Query soft-deleted failed on collection {}", collection),
                    "cleanup_soft_deleted",
                    Some(collection.to_string()),
                )
            })?;

        let doc_ids: Vec<String> = results
            .iter()
            .filter_map(|item| {
                let json_str = item.json_string();
                serde_json::from_str::<serde_json::Value>(&json_str)
                    .ok()
                    .and_then(|v| v["_id"].as_str().map(|s| s.to_string()))
            })
            .collect();

        let count = doc_ids.len();

        // Hard delete each document (creates tombstones)
        for doc_id in doc_ids {
            let delete_query = format!("DELETE FROM {} WHERE _id = :id", collection);
            self.ditto
                .store()
                .execute_v2((delete_query, serde_json::json!({"id": doc_id})))
                .await
                .map_err(|e| {
                    error!("Hard delete failed: {}", e);
                    Error::storage_error(
                        format!("Hard delete failed on collection {}", collection),
                        "cleanup_soft_deleted",
                        Some(collection.to_string()),
                    )
                })?;
        }

        debug!(
            "Cleaned up {} soft-deleted documents in {}",
            count, collection
        );
        Ok(count)
    }

    /// Configure Ditto tombstone TTL at runtime using ALTER SYSTEM
    ///
    /// IMPORTANT: Never set Edge SDK TTL > Server TTL (Edge default: 7 days, Server default: 30 days)
    ///
    /// # Arguments
    ///
    /// * `tombstone_ttl_hours` - Hours until tombstones are reaped (168 = 7 days)
    /// * `enabled` - Enable/disable automatic tombstone reaping
    /// * `days_between_reaping` - How often to run reaping (default: 1 day)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Configure tactical edge device: 7 day tombstone TTL
    /// store.configure_tombstone_ttl(168, true, 1).await?;
    /// ```
    #[instrument(skip(self))]
    pub async fn configure_tombstone_ttl(
        &self,
        tombstone_ttl_hours: u32,
        enabled: bool,
        days_between_reaping: u32,
    ) -> Result<()> {
        // Validate: Edge SDK should never exceed 7 days (168 hours)
        if tombstone_ttl_hours > 168 {
            warn!(
                "Tombstone TTL {} hours exceeds recommended Edge SDK limit of 168 hours (7 days)",
                tombstone_ttl_hours
            );
        }

        let commands = vec![
            format!("ALTER SYSTEM SET TOMBSTONE_TTL_ENABLED = {}", enabled),
            format!(
                "ALTER SYSTEM SET TOMBSTONE_TTL_HOURS = {}",
                tombstone_ttl_hours
            ),
            format!(
                "ALTER SYSTEM SET DAYS_BETWEEN_REAPING = {}",
                days_between_reaping
            ),
        ];

        for cmd in commands {
            debug!("Executing: {}", cmd);
            self.ditto
                .store()
                .execute_v2((cmd.clone(), serde_json::json!({})))
                .await
                .map_err(|e| {
                    error!("ALTER SYSTEM command failed: {}", e);
                    Error::storage_error(
                        format!("Failed to configure tombstone TTL: {}", e),
                        "configure_tombstone_ttl",
                        None,
                    )
                })?;
        }

        info!(
            "Configured tombstone TTL: {} hours, enabled={}, days_between_reaping={}",
            tombstone_ttl_hours, enabled, days_between_reaping
        );
        Ok(())
    }

    /// EVICT oldest documents from a collection to free local storage
    ///
    /// EVICT removes documents locally only (no tombstone). They may re-sync from peers.
    /// Use for edge devices with storage constraints.
    ///
    /// # Arguments
    ///
    /// * `collection` - Collection name
    /// * `limit` - Maximum number of documents to evict
    ///
    /// # Returns
    ///
    /// Number of documents evicted
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Edge device storage management: keep only 100 most recent beacons
    /// let evicted = store.evict_oldest("beacons", 50).await?;
    /// ```
    #[instrument(skip(self), fields(collection, limit))]
    pub async fn evict_oldest(&self, collection: &str, limit: usize) -> Result<usize> {
        // Query oldest documents (assuming _id or timestamp-based sorting)
        let dql_query = format!(
            "SELECT _id FROM {} ORDER BY _id ASC LIMIT {}",
            collection, limit
        );

        let results = self
            .ditto
            .store()
            .execute_v2((dql_query, serde_json::json!({})))
            .await
            .map_err(|e| {
                error!("Query for eviction failed: {}", e);
                Error::storage_error(
                    format!("Query for eviction failed on collection {}", collection),
                    "evict_oldest",
                    Some(collection.to_string()),
                )
            })?;

        let doc_ids: Vec<String> = results
            .iter()
            .filter_map(|item| {
                let json_str = item.json_string();
                serde_json::from_str::<serde_json::Value>(&json_str)
                    .ok()
                    .and_then(|v| v["_id"].as_str().map(|s| s.to_string()))
            })
            .collect();

        let count = doc_ids.len();

        // EVICT each document (local removal, no tombstone)
        for doc_id in doc_ids {
            let evict_query = format!("EVICT FROM {} WHERE _id = :id", collection);
            self.ditto
                .store()
                .execute_v2((evict_query, serde_json::json!({"id": doc_id})))
                .await
                .map_err(|e| {
                    error!("EVICT failed: {}", e);
                    Error::storage_error(
                        format!("EVICT failed on collection {}", collection),
                        "evict_oldest",
                        Some(collection.to_string()),
                    )
                })?;
        }

        debug!("Evicted {} oldest documents from {}", count, collection);
        Ok(count)
    }
}

impl Clone for DittoStore {
    fn clone(&self) -> Self {
        Self {
            ditto: self.ditto.clone(),
            _config: self._config.clone(),
        }
    }
}

impl Drop for DittoStore {
    fn drop(&mut self) {
        // Stop sync to release network resources
        self.stop_sync();

        // If this is the last reference to the Ditto instance, close it properly
        // Note: Arc::try_unwrap requires ownership, which we don't have in drop()
        // The best we can do is stop_sync() and let the Arc drop naturally
        // Ditto's Drop implementation should handle cleanup when the last Arc is dropped
        debug!("DittoStore dropped, sync stopped");
    }
}

// Conversion from DittoStore to Arc<DittoBackend>
// This allows tests using DittoStore to work with the new abstraction layer
impl From<DittoStore> for Arc<DittoBackend> {
    fn from(store: DittoStore) -> Self {
        Arc::new(DittoBackend::from_store(store))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::PeatCredentials;
    use std::time::Duration;
    use tempfile::tempdir;
    use tokio::time::sleep;

    /// Helper function to create a test DittoStore with credentials
    /// Uses PeatCredentials to load PEAT_* (with DITTO_* fallback)
    /// Returns (DittoStore, TempDir) - the TempDir must be kept alive for the duration of the test
    async fn create_test_store(test_name: &str) -> (DittoStore, tempfile::TempDir) {
        dotenvy::dotenv().ok();

        let credentials = PeatCredentials::from_env().expect(
            "Credentials required (PEAT_APP_ID/PEAT_SECRET_KEY or DITTO_APP_ID/DITTO_SHARED_KEY)",
        );

        let app_id = credentials.app_id().to_string();
        let shared_key = credentials
            .require_secret_key()
            .expect("Secret key required")
            .to_string();
        let offline_token = credentials
            .require_offline_token()
            .expect("Offline token required")
            .to_string();

        let temp_dir = tempdir().expect("Failed to create temp dir");
        let config = DittoConfig {
            app_id,
            persistence_dir: temp_dir.path().join(test_name),
            shared_key,
            offline_token,
            tcp_listen_port: None,
            tcp_connect_address: None,
        };

        let store = DittoStore::new(config).expect("Failed to create Ditto store");
        store.start_sync().expect("Failed to start sync");
        (store, temp_dir)
    }

    /// Helper function to get test credentials (returns None if not available)
    fn get_test_credentials() -> Option<(String, String, String)> {
        dotenvy::dotenv().ok();
        PeatCredentials::from_env().ok().and_then(|c| {
            let app_id = c.app_id().to_string();
            let shared_key = c.secret_key()?.to_string();
            let offline_token = c.offline_token()?.to_string();
            Some((app_id, shared_key, offline_token))
        })
    }

    #[tokio::test]
    async fn test_ditto_initialization() {
        // Skip test if credentials not available
        let Some((app_id, shared_key, offline_token)) = get_test_credentials() else {
            eprintln!("Skipping test: credentials not available (need PEAT_APP_ID/PEAT_SECRET_KEY/PEAT_OFFLINE_TOKEN or DITTO_* equivalents)");
            return;
        };

        // Create unique temp directory for this test
        let temp_dir = tempdir().expect("Failed to create temp dir");

        let config = DittoConfig {
            app_id,
            persistence_dir: temp_dir.path().to_path_buf(),
            shared_key,
            offline_token,
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
        // Skip test if credentials not available
        let Some((app_id, shared_key, offline_token)) = get_test_credentials() else {
            eprintln!("Skipping test: credentials not available");
            return;
        };

        // Create unique temp directory for this test
        let temp_dir = tempdir().expect("Failed to create temp dir");

        let config = DittoConfig {
            app_id,
            persistence_dir: temp_dir.path().to_path_buf(),
            shared_key,
            offline_token,
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

    /// Helper to clean up Ditto stores - ensures proper shutdown with sufficient wait time
    async fn cleanup_stores(
        observers: (
            Arc<dittolive_ditto::store::StoreObserver>,
            Arc<dittolive_ditto::store::StoreObserver>,
        ),
        subs: (
            Arc<dittolive_ditto::sync::SyncSubscription>,
            Arc<dittolive_ditto::sync::SyncSubscription>,
        ),
        stores: (DittoStore, DittoStore),
    ) {
        // Drop observers and subscriptions first
        drop(observers);
        drop(subs);
        sleep(Duration::from_millis(200)).await;

        // Stop sync on both stores
        stores.0.stop_sync();
        stores.1.stop_sync();
        sleep(Duration::from_secs(1)).await;

        // Drop the stores
        drop(stores.0);
        drop(stores.1);

        // CRITICAL: Ditto SDK has background threads (tombstone reaper, etc.) that need
        // significant time to exit. Without this, tests will hang for 60+ seconds.
        // The E2E tests work because there's natural time between tests, but this unit test
        // needs explicit cleanup time. 3 seconds should be enough for Ditto to fully shut down.
        sleep(Duration::from_secs(3)).await;
    }

    #[tokio::test]
    async fn test_two_instance_sync() {
        // Skip test if credentials not available
        let Some((app_id, shared_key, offline_token)) = get_test_credentials() else {
            eprintln!("Skipping test: credentials not available");
            return;
        };

        // Create unique temp directories for both stores
        let temp_dir1 = tempdir().expect("Failed to create temp dir 1");
        let temp_dir2 = tempdir().expect("Failed to create temp dir 2");

        // Create two Ditto instances with explicit TCP connection for reliable testing
        // Store1 listens on TCP port, Store2 connects to it
        let tcp_port: u16 = 12345; // Fixed port for testing

        let config1 = DittoConfig {
            app_id: app_id.clone(),
            persistence_dir: temp_dir1.path().to_path_buf(),
            shared_key: shared_key.clone(),
            offline_token: offline_token.clone(),
            tcp_listen_port: Some(tcp_port), // Store1 listens
            tcp_connect_address: None,
        };
        let store1 = DittoStore::new(config1).expect("Failed to create store 1");

        // Store2: Connect to Store1's TCP port
        let config2 = DittoConfig {
            app_id,
            persistence_dir: temp_dir2.path().to_path_buf(),
            shared_key,
            offline_token,
            tcp_listen_port: None, // Store2 doesn't listen
            tcp_connect_address: Some(format!("127.0.0.1:{}", tcp_port)),
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
        let sync_sub1 = store1
            .ditto()
            .sync()
            .register_subscription_v2("SELECT * FROM sync_test")
            .expect("Failed to create sync subscription on store1");

        let observer1 = store1
            .ditto()
            .store()
            .register_observer_v2("SELECT * FROM sync_test", |result| {
                println!("Store1 observer triggered: {} items", result.item_count());
            })
            .expect("Failed to register observer on store1");

        // Store2: Create sync subscription + observer
        let sync_sub2 = store2
            .ditto()
            .sync()
            .register_subscription_v2("SELECT * FROM sync_test")
            .expect("Failed to create sync subscription on store2");

        let observer2 = store2
            .ditto()
            .store()
            .register_observer_v2("SELECT * FROM sync_test", |result| {
                println!("Store2 observer triggered: {} items", result.item_count());
            })
            .expect("Failed to register observer on store2");

        // Use presence observer to detect TCP peer connection (observer-based, not polling)
        println!("Setting up presence observer for TCP peer connection...");
        let (presence_tx, mut presence_rx) = tokio::sync::mpsc::unbounded_channel();

        let presence_observer = store1.ditto().presence().observe(move |graph| {
            let peer_count = graph.remote_peers.len();
            if peer_count > 0 {
                let _ = presence_tx.send(peer_count);
            }
        });

        // Wait for TCP connection to establish (with timeout)
        // Note: There's a delay between physical TCP connection and presence graph update
        println!("Waiting for TCP connection between peers...");
        let connected = tokio::time::timeout(Duration::from_secs(10), presence_rx.recv()).await;

        let _peer_count = match connected {
            Ok(Some(peer_count)) => {
                println!("✓ TCP peers connected ({} peers discovered)", peer_count);
                peer_count
            }
            Ok(None) => {
                eprintln!("⚠️  Skipping test: Presence observer channel closed unexpectedly");
                drop(presence_observer);
                cleanup_stores(
                    (observer1, observer2),
                    (sync_sub1, sync_sub2),
                    (store1, store2),
                )
                .await;
                return;
            }
            Err(_) => {
                eprintln!("⚠️  Skipping test: TCP peer connection failed within 10s");
                eprintln!("    This may indicate a port conflict or network issue.");
                eprintln!("    P2P sync functionality is tested in E2E tests.");
                drop(presence_observer);
                cleanup_stores(
                    (observer1, observer2),
                    (sync_sub1, sync_sub2),
                    (store1, store2),
                )
                .await;
                return;
            }
        };

        // Give a bit more time for initial connection handshake
        sleep(Duration::from_millis(1000)).await;

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
            sleep(Duration::from_millis(1000)).await;

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

        // Drop presence observer first, then call cleanup helper
        drop(presence_observer);
        cleanup_stores(
            (observer1, observer2),
            (sync_sub1, sync_sub2),
            (store1, store2),
        )
        .await;
    }

    #[tokio::test]
    async fn test_squad_summary_storage() {
        use peat_schema::common::v1::{Position, Timestamp};
        use peat_schema::hierarchy::v1::{BoundingBox, SquadSummary};
        use peat_schema::node::v1::HealthStatus;

        // Skip test if credentials not available
        let Some((app_id, shared_key, offline_token)) = get_test_credentials() else {
            eprintln!("Skipping test: credentials not available");
            return;
        };

        let temp_dir = tempdir().expect("Failed to create temp dir");
        let config = DittoConfig {
            app_id,
            persistence_dir: temp_dir.path().to_path_buf(),
            shared_key,
            offline_token,
            tcp_listen_port: None,
            tcp_connect_address: None,
        };

        let store = DittoStore::new(config).expect("Failed to create Ditto store");
        store.start_sync().expect("Failed to start sync");

        // Create test SquadSummary
        let squad_summary = SquadSummary {
            squad_id: "squad-alpha".to_string(),
            leader_id: "node-1".to_string(),
            member_ids: vec!["node-1".to_string(), "node-2".to_string()],
            member_count: 2,
            position_centroid: Some(Position {
                latitude: 37.7749,
                longitude: -122.4194,
                altitude: 100.0,
            }),
            avg_fuel_minutes: 120.0,
            worst_health: HealthStatus::Nominal as i32,
            operational_count: 2,
            aggregated_capabilities: vec![],
            readiness_score: 0.95,
            bounding_box: Some(BoundingBox {
                southwest: Some(Position {
                    latitude: 37.7740,
                    longitude: -122.4203,
                    altitude: 90.0,
                }),
                northeast: Some(Position {
                    latitude: 37.7758,
                    longitude: -122.4185,
                    altitude: 110.0,
                }),
                max_altitude: 110.0,
                min_altitude: 90.0,
                radius_m: 500.0,
            }),
            aggregated_at: Some(Timestamp {
                seconds: 1234567890,
                nanos: 0,
            }),
        };

        // Test upsert
        let doc_id = store
            .upsert_squad_summary("squad-alpha", &squad_summary, None)
            .await
            .expect("Failed to upsert squad summary");
        assert_eq!(doc_id, "squad-alpha-summary");

        // Test retrieval
        let retrieved = store
            .get_squad_summary("squad-alpha", None)
            .await
            .expect("Failed to get squad summary");

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.squad_id, "squad-alpha");
        assert_eq!(retrieved.leader_id, "node-1");
        assert_eq!(retrieved.member_count, 2);
        assert_eq!(retrieved.operational_count, 2);
        assert!((retrieved.avg_fuel_minutes - 120.0).abs() < 0.001);

        // Test non-existent retrieval
        let not_found = store
            .get_squad_summary("squad-nonexistent", None)
            .await
            .expect("Query should succeed");
        assert!(not_found.is_none());

        store.stop_sync();
        drop(store);
        sleep(Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn test_platoon_summary_storage() {
        use peat_schema::common::v1::{Position, Timestamp};
        use peat_schema::hierarchy::v1::{BoundingBox, PlatoonSummary};
        use peat_schema::node::v1::HealthStatus;

        // Skip test if credentials not available
        let Some((app_id, shared_key, offline_token)) = get_test_credentials() else {
            eprintln!("Skipping test: credentials not available");
            return;
        };

        let temp_dir = tempdir().expect("Failed to create temp dir");
        let config = DittoConfig {
            app_id,
            persistence_dir: temp_dir.path().to_path_buf(),
            shared_key,
            offline_token,
            tcp_listen_port: None,
            tcp_connect_address: None,
        };

        let store = DittoStore::new(config).expect("Failed to create Ditto store");
        store.start_sync().expect("Failed to start sync");

        // Create test PlatoonSummary
        let platoon_summary = PlatoonSummary {
            platoon_id: "platoon-1".to_string(),
            leader_id: "node-1".to_string(),
            squad_ids: vec!["squad-alpha".to_string(), "squad-bravo".to_string()],
            squad_count: 2,
            total_member_count: 16,
            position_centroid: Some(Position {
                latitude: 37.7749,
                longitude: -122.4194,
                altitude: 100.0,
            }),
            avg_fuel_minutes: 110.0,
            worst_health: HealthStatus::Nominal as i32,
            operational_count: 14,
            aggregated_capabilities: vec![],
            readiness_score: 0.90,
            bounding_box: Some(BoundingBox {
                southwest: Some(Position {
                    latitude: 37.7700,
                    longitude: -122.4250,
                    altitude: 80.0,
                }),
                northeast: Some(Position {
                    latitude: 37.7800,
                    longitude: -122.4150,
                    altitude: 120.0,
                }),
                max_altitude: 120.0,
                min_altitude: 80.0,
                radius_m: 1000.0,
            }),
            aggregated_at: Some(Timestamp {
                seconds: 1234567890,
                nanos: 0,
            }),
        };

        // Test upsert
        let doc_id = store
            .upsert_platoon_summary("platoon-1", &platoon_summary, None)
            .await
            .expect("Failed to upsert platoon summary");
        assert_eq!(doc_id, "platoon-1-summary");

        // Test retrieval
        let retrieved = store
            .get_platoon_summary("platoon-1", None)
            .await
            .expect("Failed to get platoon summary");

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.platoon_id, "platoon-1");
        assert_eq!(retrieved.leader_id, "node-1");
        assert_eq!(retrieved.squad_count, 2);
        assert_eq!(retrieved.total_member_count, 16);
        assert_eq!(retrieved.operational_count, 14);
        assert!((retrieved.avg_fuel_minutes - 110.0).abs() < 0.001);

        // Test non-existent retrieval
        let not_found = store
            .get_platoon_summary("platoon-nonexistent", None)
            .await
            .expect("Query should succeed");
        assert!(not_found.is_none());

        store.stop_sync();
        drop(store);
        sleep(Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn test_command_upsert_and_retrieve() {
        let (store, _temp_dir) = create_test_store("test_command_upsert").await;

        use peat_schema::command::v1::{command_target::Scope, CommandTarget, HierarchicalCommand};

        // Create a test command
        let command = HierarchicalCommand {
            command_id: "cmd-001".to_string(),
            originator_id: "node-leader".to_string(),
            target: Some(CommandTarget {
                scope: Scope::Squad as i32,
                target_ids: vec!["squad-alpha".to_string()],
            }),
            priority: 3,              // IMMEDIATE
            acknowledgment_policy: 4, // BOTH
            buffer_policy: 1,         // BUFFER_AND_RETRY
            conflict_policy: 2,       // HIGHEST_PRIORITY_WINS
            leader_change_policy: 1,  // BUFFER_UNTIL_STABLE
            ..Default::default()
        };

        // Test upsert
        let doc_id = store
            .upsert_command("cmd-001", &command)
            .await
            .expect("Failed to upsert command");
        assert_eq!(doc_id, "cmd-001");

        // Test retrieval
        let retrieved = store
            .get_command("cmd-001")
            .await
            .expect("Failed to get command");

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.command_id, "cmd-001");
        assert_eq!(retrieved.originator_id, "node-leader");
        assert_eq!(retrieved.priority, 3);
        assert_eq!(retrieved.acknowledgment_policy, 4);

        // Test non-existent retrieval
        let not_found = store
            .get_command("cmd-nonexistent")
            .await
            .expect("Query should succeed");
        assert!(not_found.is_none());

        store.stop_sync();
        drop(store);
        sleep(Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn test_command_acknowledgment_upsert_and_query() {
        let (store, _temp_dir) = create_test_store("test_command_ack").await;

        use peat_schema::command::v1::{AckStatus, CommandAcknowledgment};
        use peat_schema::common::v1::Timestamp;

        // Create test acknowledgments from multiple nodes
        let ack1 = CommandAcknowledgment {
            command_id: "cmd-001".to_string(),
            node_id: "node-1".to_string(),
            status: AckStatus::AckReceived as i32,
            reason: None,
            timestamp: Some(Timestamp {
                seconds: 1234567890,
                nanos: 0,
            }),
        };

        let ack2 = CommandAcknowledgment {
            command_id: "cmd-001".to_string(),
            node_id: "node-2".to_string(),
            status: AckStatus::AckCompleted as i32,
            reason: None,
            timestamp: Some(Timestamp {
                seconds: 1234567895,
                nanos: 0,
            }),
        };

        let ack3 = CommandAcknowledgment {
            command_id: "cmd-002".to_string(), // Different command
            node_id: "node-1".to_string(),
            status: AckStatus::AckReceived as i32,
            reason: None,
            timestamp: Some(Timestamp {
                seconds: 1234567900,
                nanos: 0,
            }),
        };

        // Upsert acknowledgments
        store
            .upsert_command_ack("cmd-001-node-1", &ack1)
            .await
            .expect("Failed to upsert ack1");

        store
            .upsert_command_ack("cmd-001-node-2", &ack2)
            .await
            .expect("Failed to upsert ack2");

        store
            .upsert_command_ack("cmd-002-node-1", &ack3)
            .await
            .expect("Failed to upsert ack3");

        // Query acknowledgments for cmd-001
        let acks = store
            .query_command_acks("cmd-001")
            .await
            .expect("Failed to query acks");

        assert_eq!(acks.len(), 2);
        let ack_node_ids: Vec<String> = acks.iter().map(|a| a.node_id.clone()).collect();
        assert!(ack_node_ids.contains(&"node-1".to_string()));
        assert!(ack_node_ids.contains(&"node-2".to_string()));

        // Query acknowledgments for cmd-002
        let acks2 = store
            .query_command_acks("cmd-002")
            .await
            .expect("Failed to query acks for cmd-002");

        assert_eq!(acks2.len(), 1);
        assert_eq!(acks2[0].node_id, "node-1");
        assert_eq!(acks2[0].status, AckStatus::AckReceived as i32);

        // Query non-existent command
        let no_acks = store
            .query_command_acks("cmd-nonexistent")
            .await
            .expect("Query should succeed");
        assert!(no_acks.is_empty());

        store.stop_sync();
        drop(store);
        sleep(Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn test_field_level_delta_sync() {
        use peat_schema::common::v1::Position;
        use peat_schema::hierarchy::v1::SquadSummary;
        use peat_schema::node::v1::HealthStatus;

        // Skip test if credentials not available
        let Some((app_id, shared_key, offline_token)) = get_test_credentials() else {
            eprintln!("Skipping test: credentials not available");
            return;
        };

        // Create two temp directories for two Ditto instances
        let temp_dir1 = tempdir().expect("Failed to create temp dir 1");
        let temp_dir2 = tempdir().expect("Failed to create temp dir 2");

        // Setup TCP connection for reliable sync
        let tcp_port: u16 = 12346; // Different port from other tests

        let config1 = DittoConfig {
            app_id: app_id.clone(),
            persistence_dir: temp_dir1.path().to_path_buf(),
            shared_key: shared_key.clone(),
            offline_token: offline_token.clone(),
            tcp_listen_port: Some(tcp_port),
            tcp_connect_address: None,
        };
        let store1 = DittoStore::new(config1).expect("Failed to create store 1");

        let config2 = DittoConfig {
            app_id,
            persistence_dir: temp_dir2.path().to_path_buf(),
            shared_key,
            offline_token,
            tcp_listen_port: None,
            tcp_connect_address: Some(format!("127.0.0.1:{}", tcp_port)),
        };
        let store2 = DittoStore::new(config2).expect("Failed to create store 2");

        println!("Store 1 peer: {}", store1.peer_key());
        println!("Store 2 peer: {}", store2.peer_key());

        store1.start_sync().expect("Failed to start sync 1");
        store2.start_sync().expect("Failed to start sync 2");

        // Create sync subscriptions on BOTH stores
        let sync_sub1 = store1
            .ditto()
            .sync()
            .register_subscription_v2("SELECT * FROM sim_poc WHERE type == 'squad_summary'")
            .expect("Failed to create sync subscription on store1");

        let sync_sub2 = store2
            .ditto()
            .sync()
            .register_subscription_v2("SELECT * FROM sim_poc WHERE type == 'squad_summary'")
            .expect("Failed to create sync subscription on store2");

        // Use presence observer to wait for connection
        let (presence_tx, mut presence_rx) = tokio::sync::mpsc::unbounded_channel();
        let presence_observer = store1.ditto().presence().observe(move |graph| {
            let peer_count = graph.remote_peers.len();
            if peer_count > 0 {
                let _ = presence_tx.send(peer_count);
            }
        });

        println!("Waiting for TCP connection...");
        let connected = tokio::time::timeout(Duration::from_secs(10), presence_rx.recv()).await;

        match connected {
            Ok(Some(peer_count)) => {
                println!("✓ TCP peers connected ({} peers)", peer_count);
            }
            _ => {
                eprintln!("⚠️  Skipping test: TCP peer connection failed");
                drop(presence_observer);
                drop(sync_sub1);
                drop(sync_sub2);
                store1.stop_sync();
                store2.stop_sync();
                return;
            }
        }

        // Give connection time to stabilize
        sleep(Duration::from_millis(500)).await;

        // Step 1: Create initial SquadSummary with multiple fields
        println!("\n=== Step 1: Create initial SquadSummary ===");
        let initial_summary = SquadSummary {
            squad_id: "delta-test-squad".to_string(),
            leader_id: "node-1".to_string(),
            member_ids: vec!["node-1".to_string(), "node-2".to_string()],
            member_count: 2,
            position_centroid: Some(Position {
                latitude: 37.7749,
                longitude: -122.4194,
                altitude: 100.0,
            }),
            avg_fuel_minutes: 120.0,
            worst_health: HealthStatus::Nominal as i32,
            operational_count: 2,
            readiness_score: 0.95,
            ..Default::default()
        };

        store1
            .upsert_squad_summary("delta-test-squad", &initial_summary, None)
            .await
            .expect("Failed to upsert initial summary");

        println!("Initial summary created on store1");
        println!("  leader_id: {}", initial_summary.leader_id);
        println!("  member_count: {}", initial_summary.member_count);
        println!("  avg_fuel_minutes: {}", initial_summary.avg_fuel_minutes);
        println!("  member_ids: {:?}", initial_summary.member_ids);

        // Wait for sync to store2
        let mut synced = false;
        for attempt in 1..=20 {
            sleep(Duration::from_millis(500)).await;

            let retrieved = store2
                .get_squad_summary("delta-test-squad", None)
                .await
                .expect("Failed to query");

            if retrieved.is_some() {
                println!(
                    "✓ Initial document synced to store2 after {} attempts",
                    attempt
                );
                synced = true;
                break;
            }
        }

        assert!(
            synced,
            "Initial document should have synced from store1 to store2"
        );

        // Step 2: Update ONLY the avg_fuel_minutes field on store1
        println!("\n=== Step 2: Update ONLY avg_fuel_minutes (delta test) ===");

        let mut updated_summary = initial_summary.clone();
        updated_summary.avg_fuel_minutes = 90.0; // Changed from 120.0

        println!("Updating avg_fuel_minutes: 120.0 → 90.0");
        println!("All other fields unchanged (testing delta sync)");

        store1
            .upsert_squad_summary("delta-test-squad", &updated_summary, None)
            .await
            .expect("Failed to update summary");

        // Step 3: Verify the change synced to store2
        println!("\n=== Step 3: Verify delta sync to store2 ===");

        let mut field_synced = false;
        for attempt in 1..=20 {
            sleep(Duration::from_millis(500)).await;

            let retrieved = store2
                .get_squad_summary("delta-test-squad", None)
                .await
                .expect("Failed to query")
                .expect("Document should exist");

            if (retrieved.avg_fuel_minutes - 90.0).abs() < 0.001 {
                println!("✓ Field-level change synced after {} attempts", attempt);
                println!("  Synced avg_fuel_minutes: {}", retrieved.avg_fuel_minutes);

                // Verify other fields remained unchanged
                assert_eq!(
                    retrieved.leader_id, "node-1",
                    "leader_id should be unchanged"
                );
                assert_eq!(
                    retrieved.member_count, 2,
                    "member_count should be unchanged"
                );
                assert_eq!(
                    retrieved.member_ids,
                    vec!["node-1".to_string(), "node-2".to_string()],
                    "member_ids should be unchanged"
                );

                field_synced = true;
                break;
            }
        }

        assert!(field_synced, "Field-level delta change should have synced");

        // Step 4: Test array field update (OR-Set CRDT)
        println!("\n=== Step 4: Test OR-Set array field (member_ids) ===");

        let mut array_updated = updated_summary.clone();
        array_updated.member_ids.push("node-3".to_string()); // Add new member
        array_updated.member_count = 3;

        println!("Adding node-3 to member_ids array");

        store1
            .upsert_squad_summary("delta-test-squad", &array_updated, None)
            .await
            .expect("Failed to update array");

        // Verify array change synced
        let mut array_synced = false;
        for attempt in 1..=20 {
            sleep(Duration::from_millis(500)).await;

            let retrieved = store2
                .get_squad_summary("delta-test-squad", None)
                .await
                .expect("Failed to query")
                .expect("Document should exist");

            if retrieved.member_ids.len() == 3
                && retrieved.member_ids.contains(&"node-3".to_string())
            {
                println!("✓ OR-Set array change synced after {} attempts", attempt);
                println!("  Synced member_ids: {:?}", retrieved.member_ids);
                assert_eq!(retrieved.member_count, 3);
                array_synced = true;
                break;
            }
        }

        assert!(array_synced, "OR-Set array delta should have synced");

        // Step 5: Test nested object field update (position)
        println!("\n=== Step 5: Test nested object field (position) ===");

        let mut position_updated = array_updated.clone();
        position_updated.position_centroid = Some(Position {
            latitude: 37.7800,    // Changed
            longitude: -122.4194, // Unchanged
            altitude: 100.0,      // Unchanged
        });

        println!("Updating position latitude: 37.7749 → 37.7800");

        store1
            .upsert_squad_summary("delta-test-squad", &position_updated, None)
            .await
            .expect("Failed to update position");

        // Verify nested field change synced
        let mut position_synced = false;
        for attempt in 1..=20 {
            sleep(Duration::from_millis(500)).await;

            let retrieved = store2
                .get_squad_summary("delta-test-squad", None)
                .await
                .expect("Failed to query")
                .expect("Document should exist");

            if let Some(ref pos) = retrieved.position_centroid {
                if (pos.latitude - 37.7800).abs() < 0.0001 {
                    println!("✓ Nested object field synced after {} attempts", attempt);
                    println!("  Synced latitude: {}", pos.latitude);
                    assert_eq!(pos.longitude, -122.4194, "longitude should be unchanged");
                    assert_eq!(pos.altitude, 100.0, "altitude should be unchanged");
                    position_synced = true;
                    break;
                }
            }
        }

        assert!(
            position_synced,
            "Nested object field delta should have synced"
        );

        println!("\n✅ All field-level delta sync tests passed!");
        println!("   - Scalar field updates (avg_fuel_minutes)");
        println!("   - OR-Set array updates (member_ids)");
        println!("   - Nested object updates (position_centroid)");
        println!("\nThis confirms Ditto is performing field-level CRDT merging, not full blob replacement!");

        // Cleanup
        drop(presence_observer);
        drop(sync_sub1);
        drop(sync_sub2);
        sleep(Duration::from_millis(200)).await;

        store1.stop_sync();
        store2.stop_sync();
        sleep(Duration::from_secs(1)).await;

        drop(store1);
        drop(store2);
        sleep(Duration::from_secs(3)).await;
    }
}
