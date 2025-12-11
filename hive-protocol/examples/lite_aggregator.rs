//! HIVE-Lite Aggregator Example
//!
//! A Full HIVE node that communicates with HIVE-Lite embedded devices.
//!
//! This example demonstrates:
//! - Enabling the lite-transport feature alongside the main backend
//! - Receiving sensor data from Lite nodes
//! - Broadcasting alerts/beacons to Lite nodes
//! - Collection filtering (which schemas go to/from Lite)
//!
//! # Usage
//!
//! ```bash
//! cargo run --example lite_aggregator --features "automerge-backend,lite-transport"
//! ```
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │                    Full HIVE Node                            │
//! │  ┌─────────────────┐    ┌─────────────────────────────────┐ │
//! │  │ AutomergeBackend│    │      LiteMeshTransport          │ │
//! │  │   (or Ditto)    │    │  - UDP port 5555                │ │
//! │  │                 │    │  - Receives: lite_sensors       │ │
//! │  │  DocumentStore  │◄──►│  - Sends: beacons, alerts       │ │
//! │  │  (collections)  │    │                                 │ │
//! │  └─────────────────┘    └─────────────────────────────────┘ │
//! │                              ▲                              │
//! └──────────────────────────────│──────────────────────────────┘
//!                                │ UDP broadcast
//!              ┌─────────────────┼─────────────────┐
//!              ▼                 ▼                 ▼
//!     ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
//!     │  ESP32 Lite  │  │  M5Stack     │  │  ESP32-C3    │
//!     │  (sensors)   │  │  (display)   │  │  (relay)     │
//!     └──────────────┘  └──────────────┘  └──────────────┘
//! ```

#[cfg(all(feature = "automerge-backend", feature = "lite-transport"))]
mod example {
    use hive_protocol::sync::automerge::AutomergeBackend;
    use hive_protocol::sync::traits::*;
    use hive_protocol::sync::types::*;
    use hive_protocol::transport::lite::{
        CrdtType, LiteDocumentBridge, LiteMeshTransport, LiteTransportConfig,
    };
    use hive_protocol::transport::{MeshTransport, PeerEvent};
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
        println!("=== HIVE-Lite Aggregator ===");
        println!("Listening for Lite nodes on UDP port 5555...\n");

        // Configure the Lite transport
        let lite_config = LiteTransportConfig {
            listen_port: 5555,
            broadcast_port: 5555,
            peer_timeout_secs: 30,
            enable_broadcast: true,
            broadcast_interval_secs: 2,
            // What we receive from Lite nodes
            inbound_collections: vec![
                "lite_sensors".to_string(),
                "lite_events".to_string(),
                "lite_status".to_string(),
            ],
            // What we send to Lite nodes
            outbound_collections: vec!["beacons".to_string(), "alerts".to_string()],
            max_document_age_secs: 300,
            ..Default::default()
        };

        // Create the Lite transport
        let local_node_id: u32 = 0xF0110001; // Full node ID
        let lite_transport = Arc::new(LiteMeshTransport::new(lite_config.clone(), local_node_id));

        // Create the bridge for CRDT translation
        let bridge = Arc::new(LiteDocumentBridge::new(
            lite_transport.clone(),
            lite_config.clone(),
        ));

        // Create the Automerge backend for Full CRDT storage
        let backend = AutomergeBackend::new();
        let config = BackendConfig {
            app_id: "lite-aggregator".to_string(),
            persistence_dir: std::path::PathBuf::from("/tmp/lite_aggregator"),
            shared_key: None,
            transport: TransportConfig::default(),
            extra: HashMap::new(),
        };
        backend.initialize(config).await?;
        let doc_store = backend.document_store();

        // Subscribe to peer events
        let mut peer_events = lite_transport.subscribe_peer_events();
        let transport_clone = lite_transport.clone();
        tokio::spawn(async move {
            while let Some(event) = peer_events.recv().await {
                match event {
                    PeerEvent::Connected { peer_id, .. } => {
                        println!("🔗 Lite node connected: {}", peer_id);
                        println!(
                            "   Total peers: {}",
                            transport_clone.connected_peers().len()
                        );
                    }
                    PeerEvent::Disconnected {
                        peer_id, reason, ..
                    } => {
                        println!("🔌 Lite node disconnected: {} ({:?})", peer_id, reason);
                    }
                    _ => {}
                }
            }
        });

        // Set up CRDT callback to write incoming data to DocumentStore
        let doc_store_clone = doc_store.clone();
        let bridge_clone = bridge.clone();
        lite_transport.set_crdt_callback(move |collection, doc_id, crdt_type, data| {
            // Check collection filter
            if !bridge_clone.accepts_inbound(collection) {
                log::debug!("Filtered out inbound collection: {}", collection);
                return;
            }

            // Decode CRDT and convert to document, with human-readable output
            let (fields, display_value) = match crdt_type {
                CrdtType::GCounter => {
                    if let Some((counts, total)) = LiteDocumentBridge::decode_gcounter(data) {
                        let display = format!("count={}", total);
                        (
                            LiteDocumentBridge::gcounter_to_fields(doc_id, &counts, total),
                            display,
                        )
                    } else {
                        eprintln!("⚠️  Failed to decode GCounter from {}", doc_id);
                        return;
                    }
                }
                CrdtType::LwwRegister => {
                    if let Some((ts, node, value)) = LiteDocumentBridge::decode_lww_register(data) {
                        // Try to interpret value as UTF-8 string or show hex
                        let value_str = String::from_utf8(value.clone())
                            .unwrap_or_else(|_| format!("0x{}", hex::encode(&value)));
                        let display = format!("value={} (from 0x{:08X})", value_str, node);
                        (
                            LiteDocumentBridge::lww_register_to_fields(doc_id, ts, &value),
                            display,
                        )
                    } else {
                        eprintln!("⚠️  Failed to decode LWW-Register from {}", doc_id);
                        return;
                    }
                }
                _ => {
                    eprintln!("⚠️  Unsupported CRDT type {:?}", crdt_type);
                    return;
                }
            };

            // Store in DocumentStore (blocking since callback is sync)
            let doc_id_owned = doc_id.to_string();
            let doc = Document::with_id(doc_id_owned.clone(), fields.clone());
            let collection_owned = collection.to_string();
            let doc_store = doc_store_clone.clone();

            // Print immediately (before async store)
            println!("📥 {} from {}: {}", collection, doc_id, display_value);

            tokio::spawn(async move {
                if let Err(e) = doc_store.upsert(&collection_owned, doc).await {
                    eprintln!("   └─ Failed to store: {}", e);
                }
            });
        });

        // Start the Lite transport
        lite_transport.start().await?;
        println!("✓ Lite transport started\n");

        // Demo: Periodically broadcast alerts to Lite nodes
        let bridge_for_alerts = bridge.clone();
        tokio::spawn(async move {
            let mut alert_num = 0u32;
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;

                alert_num += 1;
                let mut fields = HashMap::new();
                fields.insert("type".to_string(), serde_json::json!("alert"));
                fields.insert(
                    "message".to_string(),
                    serde_json::json!(format!("Test alert #{}", alert_num)),
                );
                fields.insert("priority".to_string(), serde_json::json!("low"));
                fields.insert(
                    "timestamp".to_string(),
                    serde_json::json!(std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64),
                );

                match bridge_for_alerts
                    .broadcast_document("alerts", &format!("alert-{}", alert_num), &fields)
                    .await
                {
                    Ok(_) => {
                        println!("📤 Broadcast alert #{} to Lite nodes", alert_num);
                    }
                    Err(e) => {
                        log::error!("Failed to broadcast alert: {}", e);
                    }
                }
            }
        });

        // Main loop: Display status periodically
        println!("Aggregator running. Press Ctrl+C to stop.\n");
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;

            let peers = lite_transport.connected_peers();
            if !peers.is_empty() {
                println!("--- Status ---");
                println!("Connected Lite nodes: {}", peers.len());
                for peer in &peers {
                    if let Some(health) = lite_transport.get_peer_health(peer) {
                        println!("  {} - {:?}", peer, health.state);
                    }
                }
                println!();
            }
        }
    }
}

#[cfg(all(feature = "automerge-backend", feature = "lite-transport"))]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    example::run().await
}

#[cfg(not(all(feature = "automerge-backend", feature = "lite-transport")))]
fn main() {
    eprintln!("This example requires both features:");
    eprintln!(
        "  cargo run --example lite_aggregator --features \"automerge-backend,lite-transport\""
    );
}
