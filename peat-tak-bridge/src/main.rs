//! Peat-TAK Bridge Service
//!
//! Connects Peat mesh network to TAK Server for C2 visibility.
//!
//! ## Architecture
//!
//! ```text
//! Peat Mesh (Automerge/Iroh)
//!     │
//!     │ Document subscriptions
//!     ▼
//! PeatTakBridge (filtering, aggregation, encoding)
//!     │
//!     │ CoT/TCP
//!     ▼
//! TAK Server (FreeTAKServer / official)
//!     │
//!     ▼
//! WebTAK / ATAK (C2 visibility)
//! ```

use anyhow::Result;
use clap::Parser;
use futures::StreamExt;
use peat_protocol::cot::{
    types::CapabilityInfo, CapabilityAdvertisement, MissionTask, OperationalStatus, Position,
    TrackUpdate,
};
use peat_protocol::network::IrohTransport;
use peat_protocol::storage::{AutomergeBackend, AutomergeStore, StorageBackend};
use peat_transport::tak::bridge::{BridgeConfig, PeatMessage, PeatTakBridge, PublishResult};
use peat_transport::tak::server::TakServerTransport;
use peat_transport::tak::{
    CotEventStream, CotFilter, TakProtocolVersion, TakTransport, TakTransportConfig,
    TakTransportMode,
};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// Peat-TAK Bridge Service
///
/// Bridges Peat mesh network to TAK Server for C2 visibility.
#[derive(Parser, Debug)]
#[command(name = "peat-tak-bridge")]
#[command(about = "Peat-TAK Bridge - connects Peat mesh to TAK Server")]
struct Args {
    /// TAK Server address (host:port)
    #[arg(long, env = "TAK_SERVER", default_value = "127.0.0.1:8087")]
    tak_server: SocketAddr,

    /// Use TLS for TAK Server connection
    #[arg(long, env = "TAK_USE_TLS", default_value = "false")]
    tak_use_tls: bool,

    /// Path to client certificate (PEM format) for TAK Server TLS
    #[arg(long, env = "TAK_CLIENT_CERT")]
    tak_client_cert: Option<PathBuf>,

    /// Path to client private key (PEM format) for TAK Server TLS
    #[arg(long, env = "TAK_CLIENT_KEY")]
    tak_client_key: Option<PathBuf>,

    /// Path to CA certificate (PEM format) for TAK Server TLS
    #[arg(long, env = "TAK_CA_CERT")]
    tak_ca_cert: Option<PathBuf>,

    /// TAK protocol version: raw (FreeTAKServer), xml (legacy), protobuf (official TAK Server)
    #[arg(long, env = "TAK_PROTOCOL", default_value = "raw")]
    tak_protocol: String,

    /// Bridge callsign (shown in TAK)
    #[arg(long, env = "BRIDGE_CALLSIGN", default_value = "Peat-BRIDGE")]
    callsign: String,

    /// Peat app ID for mesh authentication
    #[arg(long, env = "PEAT_APP_ID", default_value = "peat-demo")]
    peat_app_id: String,

    /// Peat shared key (base64) - required for mesh authentication
    #[arg(long, env = "PEAT_SHARED_KEY")]
    peat_shared_key: Option<String>,

    /// Peat storage path for Automerge documents
    #[arg(long, env = "PEAT_STORAGE", default_value = "/tmp/peat-bridge")]
    peat_storage: PathBuf,

    /// Peat peer to connect to (format: node_id@address)
    #[arg(long, env = "PEAT_PEER")]
    peat_peer: Vec<String>,

    /// Document collections to subscribe to (comma-separated)
    #[arg(long, default_value = "tracks,capabilities")]
    collections: String,

    /// Run in demo mode (simulated messages, no Peat connection)
    #[arg(long)]
    demo: bool,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let filter = if args.verbose {
        "peat_tak_bridge=debug,peat_transport=debug,peat_protocol=debug"
    } else {
        "peat_tak_bridge=info,peat_transport=info"
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    info!("Peat-TAK Bridge starting");
    info!("TAK Server: {}", args.tak_server);
    info!("Callsign: {}", args.callsign);

    // Parse protocol version
    let protocol_version = match args.tak_protocol.to_lowercase().as_str() {
        "raw" | "rawxml" => TakProtocolVersion::RawXml,
        "xml" | "xmltcp" => TakProtocolVersion::XmlTcp,
        "protobuf" | "proto" | "v1" => TakProtocolVersion::ProtobufV1,
        _ => {
            warn!(
                "Unknown protocol '{}', defaulting to raw XML",
                args.tak_protocol
            );
            TakProtocolVersion::RawXml
        }
    };
    info!("Protocol: {:?}", protocol_version);

    // Create TAK transport configuration
    let mut tak_config = TakTransportConfig {
        mode: TakTransportMode::TakServer {
            address: args.tak_server,
            use_tls: args.tak_use_tls,
        },
        identity: Some(peat_transport::tak::TakIdentity {
            callsign: args.callsign.clone(),
            client_cert: args.tak_client_cert.clone().unwrap_or_default(),
            client_key: args.tak_client_key.clone().unwrap_or_default(),
            ca_cert: args.tak_ca_cert.clone(),
            credentials: None,
        }),
        ..Default::default()
    };
    tak_config.protocol.version = protocol_version;

    // Log TLS configuration
    if args.tak_use_tls {
        info!("TLS enabled");
        if let Some(ref cert) = args.tak_client_cert {
            info!("Client cert: {:?}", cert);
        }
        if let Some(ref ca) = args.tak_ca_cert {
            info!("CA cert: {:?}", ca);
        }
    }

    // Create TAK transport
    let mut tak_transport = TakServerTransport::new(tak_config)?;

    // Connect to TAK Server
    info!("Connecting to TAK Server...");
    match tak_transport.connect().await {
        Ok(()) => info!("Connected to TAK Server"),
        Err(e) => {
            error!("Failed to connect to TAK Server: {}", e);
            return Err(e.into());
        }
    }

    // Subscribe to mission CoT events from TAK Server (do this before creating bridge)
    // Filter for mission-type events: t-x-m-c-*
    let mission_filter = CotFilter::all().with_type_prefix("t-x-m");
    let tak_event_stream = match tak_transport.subscribe(mission_filter).await {
        Ok(stream) => {
            info!("Subscribed to TAK Server mission events");
            Some(stream)
        }
        Err(e) => {
            warn!(
                "Failed to subscribe to TAK events (TAK→Peat disabled): {}",
                e
            );
            None
        }
    };

    // Create bridge
    let bridge_config = BridgeConfig::default();
    let bridge = Arc::new(PeatTakBridge::new(tak_transport, bridge_config));

    info!("Bridge initialized, ready to relay messages");

    // Parse collections to subscribe to
    let collections: Vec<String> = args.collections.split(',').map(|s| s.to_string()).collect();
    info!("Subscribing to collections: {:?}", collections);

    if args.demo {
        // Demo mode - simulated messages
        info!("Running in DEMO mode (simulated messages)");
        let bridge_clone = Arc::clone(&bridge);
        let demo_task = tokio::spawn(async move {
            demo_messages(bridge_clone).await;
        });

        tokio::signal::ctrl_c().await?;
        demo_task.abort();
    } else {
        // Production mode - connect to Peat mesh
        info!("Connecting to Peat mesh...");
        info!("App ID: {}", args.peat_app_id);
        info!("Storage: {:?}", args.peat_storage);

        // Create storage directory
        std::fs::create_dir_all(&args.peat_storage)?;

        // Create Automerge store
        let store = Arc::new(AutomergeStore::open(&args.peat_storage)?);

        // Create Iroh transport for P2P connectivity
        let seed = format!("{}/bridge", args.peat_app_id);
        let transport = Arc::new(IrohTransport::from_seed(&seed).await?);
        info!(
            "Peat Node ID: {}",
            hex::encode(transport.endpoint_id().as_bytes())
        );

        // Create storage backend with transport
        let backend = AutomergeBackend::with_transport(Arc::clone(&store), Arc::clone(&transport));

        // Connect to specified peers
        for peer_str in &args.peat_peer {
            if let Some((node_id, addr)) = peer_str.split_once('@') {
                info!("Connecting to peer {} at {}", node_id, addr);
                let peer_info = peat_protocol::network::PeerInfo {
                    name: "peer".to_string(),
                    node_id: node_id.to_string(),
                    addresses: vec![addr.to_string()],
                    relay_url: None,
                };
                if let Err(e) = transport.connect_peer(&peer_info).await {
                    warn!("Failed to connect to peer {}: {}", node_id, e);
                }
            } else {
                warn!(
                    "Invalid peer format: {} (expected node_id@address)",
                    peer_str
                );
            }
        }

        // Subscribe to document changes
        let change_rx = store.subscribe_to_changes();
        info!("Subscribed to document changes");

        // Spawn the Peat → TAK relay task
        let bridge_clone = Arc::clone(&bridge);
        let collections_clone = collections.clone();
        let peat_to_tak_task = tokio::spawn(async move {
            relay_peat_to_tak(change_rx, backend, bridge_clone, collections_clone).await;
        });

        // Spawn the TAK → Peat relay task (if subscription succeeded)
        let tak_to_peat_task = if let Some(stream) = tak_event_stream {
            let store_clone = Arc::clone(&store);
            Some(tokio::spawn(async move {
                relay_tak_to_peat(stream, store_clone).await;
            }))
        } else {
            info!("TAK→Peat relay disabled (no subscription)");
            None
        };

        // Wait for shutdown
        info!("Bridge running. Press Ctrl+C to stop.");
        tokio::signal::ctrl_c().await?;
        peat_to_tak_task.abort();
        if let Some(task) = tak_to_peat_task {
            task.abort();
        }
    }

    info!("Peat-TAK Bridge stopped");
    Ok(())
}

/// Relay Peat document changes to TAK Server
async fn relay_peat_to_tak(
    mut change_rx: broadcast::Receiver<String>,
    backend: AutomergeBackend,
    bridge: Arc<PeatTakBridge<TakServerTransport>>,
    collections: Vec<String>,
) {
    info!("Starting Peat→TAK relay loop");

    loop {
        match change_rx.recv().await {
            Ok(doc_key) => {
                debug!("Document changed: {}", doc_key);

                // Parse collection:doc_id format
                let (collection, doc_id) = match doc_key.split_once(':') {
                    Some((c, d)) => (c, d),
                    None => {
                        debug!("Skipping key without collection prefix: {}", doc_key);
                        continue;
                    }
                };

                // Check if we're subscribed to this collection
                if !collections.iter().any(|c| c == collection) {
                    debug!("Skipping unsubscribed collection: {}", collection);
                    continue;
                }

                // Fetch the document
                let coll = backend.collection(collection);
                let doc_data = match coll.get(doc_id) {
                    Ok(Some(data)) => data,
                    Ok(None) => {
                        debug!("Document {} not found", doc_key);
                        continue;
                    }
                    Err(e) => {
                        warn!("Failed to fetch document {}: {}", doc_key, e);
                        continue;
                    }
                };

                // Parse and convert to PeatMessage
                let message = match collection {
                    "tracks" => parse_track_document(&doc_data, doc_id),
                    "capabilities" => parse_capability_document(&doc_data, doc_id),
                    _ => {
                        debug!("Unknown collection type: {}", collection);
                        None
                    }
                };

                // Publish to TAK
                if let Some(msg) = message {
                    match bridge.publish_to_tak(msg).await {
                        PublishResult::Published => {
                            info!("Relayed {} to TAK", doc_key);
                        }
                        PublishResult::Filtered(reason) => {
                            debug!("Filtered {}: {}", doc_key, reason);
                        }
                        PublishResult::TransportError(e) => {
                            error!("Failed to relay {}: {}", doc_key, e);
                        }
                        other => {
                            debug!("Relay result for {}: {:?}", doc_key, other);
                        }
                    }
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!("Subscription lagged {} messages", n);
            }
            Err(broadcast::error::RecvError::Closed) => {
                info!("Document change channel closed");
                break;
            }
        }
    }
}

/// Parse a track document from JSON
fn parse_track_document(data: &[u8], doc_id: &str) -> Option<PeatMessage> {
    let json_str = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(e) => {
            warn!("Invalid UTF-8 in track document {}: {}", doc_id, e);
            return None;
        }
    };

    // Try to parse as TrackUpdate JSON
    match serde_json::from_str::<serde_json::Value>(json_str) {
        Ok(json) => {
            // Extract fields from JSON
            let track = TrackUpdate {
                track_id: json["track_id"].as_str().unwrap_or(doc_id).to_string(),
                source_platform: json["source_platform"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string(),
                source_model: json["source_model"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string(),
                model_version: json["model_version"].as_str().unwrap_or("1.0").to_string(),
                cell_id: json["cell_id"].as_str().map(|s| s.to_string()),
                formation_id: json["formation_id"].as_str().map(|s| s.to_string()),
                timestamp: chrono::Utc::now(),
                position: Position {
                    lat: json["position"]["lat"].as_f64().unwrap_or(0.0),
                    lon: json["position"]["lon"].as_f64().unwrap_or(0.0),
                    hae: json["position"]["hae"].as_f64(),
                    cep_m: json["position"]["cep_m"].as_f64(),
                },
                velocity: None,
                classification: json["classification"]
                    .as_str()
                    .unwrap_or("a-u-G")
                    .to_string(),
                confidence: json["confidence"].as_f64().unwrap_or(0.5),
                attributes: Default::default(),
            };
            Some(PeatMessage::Track(track))
        }
        Err(e) => {
            warn!("Failed to parse track JSON for {}: {}", doc_id, e);
            None
        }
    }
}

/// Parse a capability document from JSON
fn parse_capability_document(data: &[u8], doc_id: &str) -> Option<PeatMessage> {
    let json_str = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(e) => {
            warn!("Invalid UTF-8 in capability document {}: {}", doc_id, e);
            return None;
        }
    };

    match serde_json::from_str::<serde_json::Value>(json_str) {
        Ok(json) => {
            // Parse operational status
            let status = match json["status"].as_str().unwrap_or("READY") {
                "ACTIVE" => OperationalStatus::Active,
                "DEGRADED" => OperationalStatus::Degraded,
                "OFFLINE" => OperationalStatus::Offline,
                "LOADING" => OperationalStatus::Loading,
                _ => OperationalStatus::Ready,
            };

            // Parse capabilities array
            let capabilities: Vec<CapabilityInfo> = json["capabilities"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|c| {
                            Some(CapabilityInfo {
                                capability_type: c["type"].as_str()?.to_string(),
                                model_name: c["model"].as_str().unwrap_or("unknown").to_string(),
                                version: c["version"].as_str().unwrap_or("1.0").to_string(),
                                precision: c["precision"].as_f64().unwrap_or(0.9),
                                status: OperationalStatus::Ready,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            let cap = CapabilityAdvertisement {
                platform_id: json["platform_id"].as_str().unwrap_or(doc_id).to_string(),
                platform_type: json["platform_type"].as_str().unwrap_or("UGV").to_string(),
                position: Position {
                    lat: json["position"]["lat"].as_f64().unwrap_or(0.0),
                    lon: json["position"]["lon"].as_f64().unwrap_or(0.0),
                    hae: json["position"]["hae"].as_f64(),
                    cep_m: None,
                },
                status,
                readiness: json["readiness"].as_f64().unwrap_or(1.0),
                capabilities,
                cell_id: json["cell_id"].as_str().map(|s| s.to_string()),
                formation_id: json["formation_id"].as_str().map(|s| s.to_string()),
                timestamp: chrono::Utc::now(),
            };
            Some(PeatMessage::Capability(cap))
        }
        Err(e) => {
            warn!("Failed to parse capability JSON for {}: {}", doc_id, e);
            None
        }
    }
}

/// Relay TAK Server mission events to Peat mesh
///
/// Receives CoT events from TAK Server, converts mission-type events
/// to MissionTask, and stores them in the Automerge "missions" collection.
async fn relay_tak_to_peat(mut event_stream: CotEventStream, store: Arc<AutomergeStore>) {
    info!("Starting TAK→Peat relay loop");

    while let Some(result) = event_stream.next().await {
        match result {
            Ok(event) => {
                debug!(
                    "Received CoT event from TAK: {} (type: {})",
                    event.uid,
                    event.cot_type.as_str()
                );

                // Check if this is a mission event
                if !MissionTask::is_mission_cot_type(event.cot_type.as_str()) {
                    debug!("Skipping non-mission event: {}", event.cot_type.as_str());
                    continue;
                }

                // Convert to MissionTask
                match MissionTask::from_cot_event(&event) {
                    Ok(task) => {
                        info!(
                            "Converted mission task: {} (type: {})",
                            task.task_id,
                            task.task_type.as_str()
                        );

                        // Serialize to JSON for Automerge storage
                        match task.to_json() {
                            Ok(json) => {
                                // Store in missions collection
                                let collection = store.collection("missions");

                                match collection.upsert(&task.task_id, json.into_bytes()) {
                                    Ok(()) => {
                                        info!("Stored mission task {} in Peat mesh", task.task_id);
                                    }
                                    Err(e) => {
                                        error!(
                                            "Failed to store mission task {}: {}",
                                            task.task_id, e
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to serialize mission task {}: {}", task.task_id, e);
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Failed to convert CoT event {} to MissionTask: {}",
                            event.uid, e
                        );
                    }
                }
            }
            Err(e) => {
                error!("Error receiving TAK event: {}", e);
            }
        }
    }

    info!("TAK→Peat relay loop ended");
}

/// Demo function that sends simulated messages
async fn demo_messages(bridge: Arc<PeatTakBridge<TakServerTransport>>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
    let mut track_num = 0;

    loop {
        interval.tick().await;

        track_num += 1;
        let track_id = format!("TRACK-{:04}", track_num);

        // Create a simulated track update
        let track = TrackUpdate {
            track_id: track_id.clone(),
            source_platform: "Alpha-3".to_string(),
            source_model: "YOLOv8".to_string(),
            model_version: "1.3.0".to_string(),
            cell_id: Some("Alpha".to_string()),
            formation_id: Some("Demo-Formation".to_string()),
            timestamp: chrono::Utc::now(),
            position: Position {
                lat: 38.8977 + (track_num as f64 * 0.001),
                lon: -77.0365 + (track_num as f64 * 0.001),
                hae: Some(10.0),
                cep_m: Some(5.0),
            },
            velocity: None,
            classification: "a-f-G-U-C".to_string(),
            confidence: 0.92,
            attributes: Default::default(),
        };

        let message = PeatMessage::Track(track);

        match bridge.publish_to_tak(message).await {
            PublishResult::Published => {
                info!("Published track {} to TAK", track_id);
            }
            PublishResult::Filtered(reason) => {
                warn!("Track {} filtered: {}", track_id, reason);
            }
            PublishResult::TransportError(e) => {
                error!("Failed to publish track {}: {}", track_id, e);
            }
            other => {
                info!("Track {} result: {:?}", track_id, other);
            }
        }
    }
}
