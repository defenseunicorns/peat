//! HIVE BLE Mesh Transport Test Application
//!
//! Tests the hive-btle integration with hive-protocol's transport abstraction.
//!
//! ## Current Status
//!
//! This app currently uses the StubAdapter (no real BLE) to demonstrate the
//! integration between hive-protocol and hive-btle. Platform-specific adapters
//! will be enabled once hive-btle exports them properly.
//!
//! ## Building
//!
//! ```bash
//! cargo run -p hive-ble-test
//! ```
//!
//! ## Future Platform Support
//!
//! Once hive-btle exposes platform adapters:
//! - macOS: `cargo run -p hive-ble-test --features macos`
//! - Linux: `cargo run -p hive-ble-test --features linux`
//! - Windows: `cargo run -p hive-ble-test --features windows`

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::sync::Arc;
use std::time::Duration;
use tracing::info;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
use tracing::warn;

#[cfg(target_os = "macos")]
use hive_btle::platform::apple::CoreBluetoothAdapter;
#[cfg(target_os = "linux")]
use hive_btle::platform::linux::BluerAdapter;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
use hive_btle::StubAdapter;
use hive_btle::{BleConfig, BluetoothLETransport};
use hive_protocol::transport::{
    HiveBleTransport, MessageRequirements, NodeId, TransportCapabilities, TransportInstance,
    TransportManager, TransportManagerConfig, TransportPolicy, TransportType,
};

#[derive(Parser)]
#[command(name = "hive-ble-test")]
#[command(about = "HIVE BLE mesh transport test application")]
struct Cli {
    /// Node ID (hex, e.g., "12345678")
    #[arg(short, long, default_value = "00000000")]
    node_id: String,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan for nearby HIVE BLE nodes
    Scan {
        /// Duration in seconds
        #[arg(short, long, default_value = "10")]
        duration: u64,
    },

    /// Advertise this node and wait for connections
    Advertise {
        /// Duration in seconds (0 = indefinite)
        #[arg(short, long, default_value = "0")]
        duration: u64,
    },

    /// Run as a mesh node (scan + advertise)
    Mesh,

    /// Show transport capabilities and status
    Status,

    /// Test the PACE policy selection
    Pace,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&cli.log_level)),
        )
        .init();

    info!("HIVE BLE Test Application");
    info!("Node ID: {}", cli.node_id);

    // Parse node ID
    let node_id_u32 = u32::from_str_radix(cli.node_id.trim_start_matches("0x"), 16)
        .context("Invalid node ID format (expected hex)")?;

    // Create transport using stub adapter
    // TODO: Use platform-specific adapter when hive-btle exports them
    let transport = create_transport(node_id_u32).await?;

    // Create TransportManager with PACE policy
    let policy = TransportPolicy::new("ble-test")
        .primary(vec!["ble-primary"])
        .alternate(vec!["ble-backup"])
        .contingency(vec!["ble-emergency"]);

    let config = TransportManagerConfig::with_policy(policy);
    let manager = TransportManager::new(config);

    // Register the BLE transport instance
    let instance = TransportInstance::new(
        "ble-primary",
        TransportType::BluetoothLE,
        TransportCapabilities::bluetooth_le(),
    )
    .with_description("Primary BLE adapter (stub)");

    manager.register_instance(instance, Arc::new(transport));

    info!("Transport registered");
    info!("PACE level: {}", manager.current_pace_level());
    info!(
        "Registered instances: {:?}",
        manager.registered_instance_ids()
    );

    match cli.command {
        Commands::Scan { duration } => {
            run_scan(&manager, Duration::from_secs(duration)).await?;
        }
        Commands::Advertise { duration } => {
            let dur = if duration == 0 {
                None
            } else {
                Some(Duration::from_secs(duration))
            };
            run_advertise(&manager, dur).await?;
        }
        Commands::Mesh => {
            run_mesh(&manager).await?;
        }
        Commands::Status => {
            show_status(&manager).await?;
        }
        Commands::Pace => {
            test_pace_selection(&manager).await?;
        }
    }

    Ok(())
}

/// Create BLE transport with platform-specific adapter
#[cfg(target_os = "macos")]
async fn create_transport(node_id: u32) -> Result<HiveBleTransport<CoreBluetoothAdapter>> {
    use hive_btle::platform::BleAdapter;

    info!("Using CoreBluetoothAdapter for macOS");

    let config = BleConfig::hive_lite(hive_btle::NodeId::new(node_id));
    let mut adapter =
        CoreBluetoothAdapter::new().context("Failed to create CoreBluetooth adapter")?;
    adapter
        .init(&config)
        .await
        .context("Failed to initialize CoreBluetooth adapter")?;
    let btle = BluetoothLETransport::new(config, adapter);
    let transport = HiveBleTransport::new(btle);

    Ok(transport)
}

#[cfg(target_os = "linux")]
async fn create_transport(node_id: u32) -> Result<HiveBleTransport<BluerAdapter>> {
    use hive_btle::platform::BleAdapter;

    info!("Using BluerAdapter for Linux");

    let config = BleConfig::hive_lite(hive_btle::NodeId::new(node_id));
    let mut adapter = BluerAdapter::new()
        .await
        .context("Failed to create BlueZ adapter")?;
    adapter
        .init(&config)
        .await
        .context("Failed to initialize BlueZ adapter")?;
    let btle = BluetoothLETransport::new(config, adapter);
    let transport = HiveBleTransport::new(btle);

    Ok(transport)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
async fn create_transport(node_id: u32) -> Result<HiveBleTransport<StubAdapter>> {
    use hive_btle::platform::BleAdapter;

    warn!("Using StubAdapter - no real BLE hardware on this platform");

    let config = BleConfig::hive_lite(hive_btle::NodeId::new(node_id));
    let mut adapter = StubAdapter::default();
    adapter
        .init(&config)
        .await
        .context("Failed to initialize stub adapter")?;
    let btle = BluetoothLETransport::new(config, adapter);
    let transport = HiveBleTransport::new(btle);

    Ok(transport)
}

async fn run_scan(manager: &TransportManager, duration: Duration) -> Result<()> {
    info!("Scanning for HIVE nodes for {:?}...", duration);

    if let Some(transport) = manager.get_instance(&"ble-primary".to_string()) {
        transport
            .start()
            .await
            .context("Failed to start transport")?;

        // Wait for scan duration
        tokio::time::sleep(duration).await;

        // Report discovered peers
        let peers = transport.connected_peers();
        if peers.is_empty() {
            info!("No peers discovered (expected with stub adapter)");
        } else {
            info!("Discovered {} peers:", peers.len());
            for peer in peers {
                info!("  - {}", peer);
            }
        }

        transport.stop().await.context("Failed to stop transport")?;
    }

    Ok(())
}

async fn run_advertise(manager: &TransportManager, duration: Option<Duration>) -> Result<()> {
    info!("Advertising as HIVE node...");

    if let Some(transport) = manager.get_instance(&"ble-primary".to_string()) {
        transport
            .start()
            .await
            .context("Failed to start transport")?;

        match duration {
            Some(dur) => {
                info!("Advertising for {:?}", dur);
                tokio::time::sleep(dur).await;
            }
            None => {
                info!("Advertising indefinitely (Ctrl+C to stop)");
                tokio::signal::ctrl_c().await?;
            }
        }

        transport.stop().await.context("Failed to stop transport")?;
    }

    Ok(())
}

async fn run_mesh(manager: &TransportManager) -> Result<()> {
    info!("Running as mesh node");
    info!("Press Ctrl+C to stop");

    if let Some(transport) = manager.get_instance(&"ble-primary".to_string()) {
        transport
            .start()
            .await
            .context("Failed to start transport")?;

        // Main loop - report status periodically
        let mut interval = tokio::time::interval(Duration::from_secs(5));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let peer_count = transport.peer_count();
                    let pace_level = manager.current_pace_level();
                    info!(
                        "Status: {} peers connected, PACE level: {}",
                        peer_count, pace_level
                    );
                }
                _ = tokio::signal::ctrl_c() => {
                    info!("Shutting down...");
                    break;
                }
            }
        }

        transport.stop().await.context("Failed to stop transport")?;
    }

    Ok(())
}

async fn show_status(manager: &TransportManager) -> Result<()> {
    info!("=== Transport Status ===");
    info!(
        "Registered instances: {:?}",
        manager.registered_instance_ids()
    );
    info!(
        "Available instances: {:?}",
        manager.available_instance_ids()
    );
    info!("Current PACE level: {}", manager.current_pace_level());

    if let Some(transport) = manager.get_instance(&"ble-primary".to_string()) {
        info!("BLE Transport:");
        info!("  Available: {}", transport.is_available());
        info!("  Connected peers: {}", transport.peer_count());

        // Show capabilities
        let caps = transport.capabilities();
        info!("  Capabilities:");
        info!("    Max bandwidth: {} bps", caps.max_bandwidth_bps);
        info!("    Typical latency: {} ms", caps.typical_latency_ms);
        info!("    Max range: {} m", caps.max_range_meters);
        info!("    Reliable: {}", caps.reliable);
        info!("    Broadcast: {}", caps.supports_broadcast);
    }

    Ok(())
}

async fn test_pace_selection(manager: &TransportManager) -> Result<()> {
    info!("=== PACE Policy Selection Test ===");

    // Show current state
    info!("Current PACE level: {}", manager.current_pace_level());
    info!(
        "Available transports: {:?}",
        manager.available_instance_ids()
    );

    // Test selection for a hypothetical peer
    let test_peer = NodeId::new("DEADBEEF".to_string());
    let requirements = MessageRequirements::default();

    info!("Testing transport selection for peer: {}", test_peer);

    // Note: This will likely return None since the stub adapter
    // doesn't report reachability for unknown peers
    match manager.select_transport_pace(&test_peer, &requirements) {
        Some(transport_id) => {
            info!("  Selected: {}", transport_id);
        }
        None => {
            info!("  No transport selected (peer not reachable via stub adapter)");
            info!("  With real BLE, this would select from available transports");
        }
    }

    // Show what would be selected if we had multiple transports
    info!("\nPACE policy order (if all were available):");
    info!("  1. PRIMARY: ble-primary");
    info!("  2. ALTERNATE: ble-backup");
    info!("  3. CONTINGENCY: ble-emergency");

    Ok(())
}
