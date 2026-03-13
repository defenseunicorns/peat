//! Peat Beacon Mesh Integration Example
//!
//! Demonstrates connecting an edge device to the Peat mesh network
//! and publishing capability advertisements.
//!
//! Usage:
//!   # Generate a formation secret first (share with all nodes)
//!   cargo run --example beacon_mesh --release -- --generate-secret
//!
//!   # Start a beacon node
//!   export PEAT_APP_ID="test-formation"
//!   export PEAT_SECRET_KEY="<base64-secret>"
//!   cargo run --example beacon_mesh --release -- --bind 127.0.0.1:5000
//!
//!   # Start another beacon node and connect
//!   cargo run --example beacon_mesh --release -- --bind 127.0.0.1:5001 --peer 127.0.0.1:5000
//!
//! Options:
//!   --generate-secret     Generate a new formation secret and exit
//!   --bind <addr>         Bind to specific address (default: random port)
//!   --peer <addr>         Connect to a peer node
//!   --formation <id>      Formation ID (default: from Peat app ID env)
//!   --platform-id <id>    Platform identifier (default: auto-generated)

use peat_inference::beacon::{BeaconConfig, CameraSpec, ModelSpec, PeatBeacon};
use peat_inference::inference::JetsonInfo;
use peat_inference::messages::{ModelPerformance, OperationalStatus};
use peat_inference::sync::{collections, PeatSyncClient, SyncConfig};

use peat_protocol::network::IrohTransport;
use peat_protocol::security::FormationKey;
use peat_protocol::storage::AutomergeStore;
use peat_protocol::sync::automerge::AutomergeIrohBackend;
use peat_protocol::sync::types::{BackendConfig, TransportConfig};
use peat_protocol::sync::DataSyncBackend;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,iroh=warn,quinn=warn".to_string()),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();

    // Check for --generate-secret flag
    if args.iter().any(|a| a == "--generate-secret") {
        let secret = FormationKey::generate_secret();
        println!("Generated Formation Secret");
        println!("===========================");
        println!();
        println!("Secret (base64): {}", secret);
        println!();
        println!("Usage:");
        println!("  export PEAT_APP_ID=\"my-formation\"");
        println!("  export PEAT_SECRET_KEY=\"{}\"", secret);
        println!();
        println!("Share this secret with all nodes in the formation.");
        return Ok(());
    }

    // Parse arguments
    let bind_addr: Option<SocketAddr> = args
        .iter()
        .position(|a| a == "--bind")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok());

    let peer_addr: Option<String> = args
        .iter()
        .position(|a| a == "--peer")
        .and_then(|i| args.get(i + 1))
        .cloned();

    let formation_id = args
        .iter()
        .position(|a| a == "--formation")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .or_else(|| std::env::var("PEAT_APP_ID").ok())
        .unwrap_or_else(|| "test-formation".to_string());

    let platform_id = args
        .iter()
        .position(|a| a == "--platform-id")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| format!("beacon-{}", &uuid::Uuid::new_v4().to_string()[..8]));

    // Get shared secret from environment
    let secret_key = std::env::var("PEAT_SECRET_KEY").ok();

    println!("Peat Beacon Mesh Integration");
    println!("=============================");
    println!();
    println!("Formation: {}", formation_id);
    println!("Platform ID: {}", platform_id);
    println!("Bind address: {:?}", bind_addr);
    println!("Peer: {:?}", peer_addr);
    println!(
        "Secret key: {}",
        if secret_key.is_some() {
            "configured"
        } else {
            "NOT SET (will generate)"
        }
    );
    println!();

    // Create persistence directory
    let persistence_dir = PathBuf::from(format!("/tmp/peat-beacon-{}", platform_id));
    std::fs::create_dir_all(&persistence_dir)?;

    // Create AutomergeStore
    println!("Creating storage backend...");
    let store = Arc::new(AutomergeStore::open(&persistence_dir)?);

    // Create IrohTransport
    println!("Creating network transport...");
    let transport = if let Some(addr) = bind_addr {
        Arc::new(IrohTransport::bind(addr).await?)
    } else {
        Arc::new(IrohTransport::new().await?)
    };

    let endpoint_id = transport.endpoint_id();
    println!("  Node ID: {:?}", endpoint_id);

    // Create AutomergeIrohBackend
    println!("Creating Peat backend...");
    let backend = Arc::new(AutomergeIrohBackend::from_parts(
        store,
        Arc::clone(&transport),
    ));

    // Initialize with credentials
    let shared_key = secret_key.unwrap_or_else(|| {
        let key = FormationKey::generate_secret();
        println!("  Generated new secret: {}", key);
        println!("  (Set PEAT_SECRET_KEY to share with other nodes)");
        key
    });

    let config = BackendConfig {
        app_id: formation_id.clone(),
        persistence_dir: persistence_dir.clone(),
        shared_key: Some(shared_key),
        transport: TransportConfig::default(),
        extra: HashMap::new(),
    };

    backend.initialize(config).await?;
    println!("  Backend initialized");

    // Connect to peer if specified
    if let Some(peer_addr_str) = peer_addr {
        println!();
        println!("Connecting to peer: {}", peer_addr_str);

        // For now, we need the peer's node ID - in production this would come from discovery
        // This is a limitation: we need to know the peer's endpoint ID
        // In a real deployment, use mDNS discovery or a rendezvous server
        println!("  Note: Direct peer connection requires knowing the peer's node ID.");
        println!("  For automatic discovery, both nodes should use mDNS on the same network.");

        // Start mDNS discovery instead
        println!("  Starting peer discovery...");
        backend.peer_discovery().start().await?;
    } else {
        // Start peer discovery for incoming connections
        println!();
        println!("Starting peer discovery (waiting for connections)...");
        backend.peer_discovery().start().await?;
    }

    // Create beacon
    println!();
    println!("Creating beacon...");

    let jetson_info = JetsonInfo::detect().ok();

    let mut beacon_config = BeaconConfig::new(&platform_id)
        .with_camera(CameraSpec::imx219())
        .with_model(ModelSpec::yolov8n())
        .with_formation(&formation_id);

    if let Some(ref info) = jetson_info {
        beacon_config = beacon_config
            .with_name(&format!("{} @ {}", platform_id, info.model))
            .with_compute(peat_inference::beacon::ComputeSpec::from_jetson(info));
    }

    let beacon = PeatBeacon::new(beacon_config)?;
    beacon.set_status(OperationalStatus::Ready).await;

    // Simulate measured performance
    beacon
        .update_performance(ModelPerformance {
            precision: 0.72,
            recall: 0.68,
            fps: 3.2,
            latency_ms: Some(312.0),
        })
        .await;

    // Create PeatSyncClient
    println!();
    println!("Creating sync client...");
    let sync_config = SyncConfig::new(&platform_id, persistence_dir.to_str().unwrap())
        .with_formation(&formation_id);

    let mut sync_client = PeatSyncClient::with_backend(sync_config, backend.clone());

    // Publish platform registration
    println!();
    println!("Publishing platform registration...");
    let registration = beacon.generate_registration().await;

    // Convert registration to Document format
    let mut fields = HashMap::new();
    for (key, value) in registration.as_object().unwrap() {
        fields.insert(key.clone(), value.clone());
    }

    let doc = peat_protocol::sync::types::Document::with_id(&platform_id, fields);
    let doc_id = backend
        .document_store()
        .upsert(collections::PLATFORMS, doc)
        .await?;
    println!("  Registered as: {}", doc_id);

    // Publish capability advertisement
    println!();
    println!("Publishing capability advertisement...");
    let advert = beacon.generate_advertisement().await;
    let advert_id = sync_client.publish_capability(&advert).await?;
    println!("  Published: {}", advert_id);

    // Print current state
    println!();
    println!("Beacon Status");
    println!("-------------");
    println!("Platform ID: {}", beacon.platform_id());
    println!("Formation: {}", formation_id);
    println!("Status: {:?}", OperationalStatus::Ready);

    // Query capabilities in the mesh
    println!();
    println!("Querying mesh capabilities...");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let all_docs = backend
        .document_store()
        .query(
            collections::CAPABILITIES,
            &peat_protocol::sync::types::Query::All,
        )
        .await?;

    println!(
        "  Found {} capability advertisements in mesh",
        all_docs.len()
    );
    for doc in &all_docs {
        if let Some(id) = &doc.id {
            let platform = doc
                .get("platform_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            println!("    - {}: {}", id, platform);
        }
    }

    // Keep running and periodically publish updates
    println!();
    println!("Beacon running. Press Ctrl+C to stop.");
    println!("Will publish capability updates every 30 seconds.");
    println!();

    let mut interval = tokio::time::interval(Duration::from_secs(30));
    let mut update_count = 0u64;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                update_count += 1;

                // Update metrics (in production, these would come from actual inference)
                beacon.update_performance(ModelPerformance {
                    precision: 0.72 + (update_count as f64 * 0.001),
                    recall: 0.68,
                    fps: 3.2 + (rand::random::<f64>() * 0.5 - 0.25),
                    latency_ms: Some(312.0 + (rand::random::<f64>() * 20.0 - 10.0)),
                }).await;

                // Publish updated advertisement
                let advert = beacon.generate_advertisement().await;
                match sync_client.publish_capability(&advert).await {
                    Ok(_) => println!("[{}] Published capability update #{}", chrono::Utc::now().format("%H:%M:%S"), update_count),
                    Err(e) => println!("[{}] Failed to publish: {}", chrono::Utc::now().format("%H:%M:%S"), e),
                }

                // Log sync stats
                let stats = sync_client.stats();
                println!("  Sync stats: {} capabilities published", stats.capabilities_published);
            }
            _ = tokio::signal::ctrl_c() => {
                println!();
                println!("Shutting down beacon...");
                break;
            }
        }
    }

    // Cleanup
    println!("Beacon stopped.");

    Ok(())
}
