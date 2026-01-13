//! HIVE TAK Test Client
//!
//! This example creates a HIVE node that publishes mock data for testing
//! mDNS peer discovery with the ATAK plugin.
//!
//! # Running the Example
//!
//! ```bash
//! CXXFLAGS="-include cstdint" cargo run --example hive_tak_client -p hive-ffi --features sync
//! ```
//!
//! # What It Does
//!
//! 1. Creates a HIVE node with mDNS discovery enabled
//! 2. Publishes moving tracks that fly patterns over Atlanta
//! 3. Starts P2P sync and waits for peers to discover via mDNS
//!
//! # IMPORTANT: Test Isolation
//!
//! By default, this uses a TEST-ONLY formation ID (`hive-test-tak-client`) that is
//! isolated from production ATAK deployments. To test with the ATAK plugin, you must
//! explicitly set the same formation ID:
//!
//! ```bash
//! HIVE_APP_ID=default-atak-formation cargo run --example hive_tak_client ...
//! ```
//!
//! This prevents test data from accidentally polluting production deployments.

use hive_ffi::{create_node, NodeConfig};
use std::f64::consts::PI;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Atlanta area center coordinates
const ATLANTA_LAT: f64 = 33.749;
const ATLANTA_LON: f64 = -84.388;

/// Flight pattern configuration
struct FlightPattern {
    name: &'static str,
    pattern_type: PatternType,
    center_lat: f64,
    center_lon: f64,
    radius_deg: f64,   // Pattern radius in degrees
    altitude_m: f64,   // Altitude in meters
    speed_factor: f64, // How fast the pattern progresses
    classification: &'static str,
    category: &'static str,
}

#[derive(Clone, Copy, Debug)]
enum PatternType {
    Orbit,     // Circular orbit
    Racetrack, // Oval racetrack pattern
    Figure8,   // Figure-8 pattern
    Lawnmower, // Search pattern (back and forth)
}

fn main() {
    println!("=== HIVE TAK Test Client - Atlanta Flight Patterns ===\n");

    // Use TEST-ONLY formation by default to avoid polluting production deployments
    // Set HIVE_APP_ID=default-atak-formation to test with real ATAK plugin
    let app_id = std::env::var("HIVE_APP_ID").unwrap_or_else(|_| "hive-test-tak-client".into());
    let shared_key = std::env::var("HIVE_SHARED_KEY")
        .unwrap_or_else(|_| "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into());

    // Create storage directory
    let storage_path =
        std::env::var("HIVE_STORAGE_PATH").unwrap_or_else(|_| "/tmp/hive-tak-client".into());
    std::fs::create_dir_all(&storage_path).expect("Failed to create storage directory");

    println!("Configuration:");
    println!("  Formation: {}", app_id);
    println!("  Storage: {}", storage_path);
    println!("  Area: Atlanta ({:.3}, {:.3})", ATLANTA_LAT, ATLANTA_LON);
    println!();

    println!("Creating HIVE node with mDNS discovery...");
    let config = NodeConfig {
        app_id: app_id.clone(),
        shared_key: shared_key.clone(),
        bind_address: Some("0.0.0.0:42008".into()), // Fixed port for testing
        storage_path: storage_path.clone(),
        transport: None, // Use default Iroh transport only
    };

    let node: Arc<hive_ffi::HiveNode> = match create_node(config) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Failed to create HIVE node: {:?}", e);
            return;
        }
    };

    println!("Node ID: {}", node.node_id());
    println!("Endpoint: {}", node.endpoint_addr());
    println!();

    // Define flight patterns over Atlanta
    let patterns = vec![
        FlightPattern {
            name: "HAWK-1",
            pattern_type: PatternType::Orbit,
            center_lat: ATLANTA_LAT + 0.02, // North of downtown
            center_lon: ATLANTA_LON - 0.01,
            radius_deg: 0.015,
            altitude_m: 300.0,
            speed_factor: 1.0,
            classification: "a-f-A-M-F-Q", // Friendly UAV
            category: "aircraft",
        },
        FlightPattern {
            name: "HAWK-2",
            pattern_type: PatternType::Racetrack,
            center_lat: ATLANTA_LAT - 0.015, // South of downtown
            center_lon: ATLANTA_LON + 0.02,
            radius_deg: 0.02,
            altitude_m: 250.0,
            speed_factor: 0.8,
            classification: "a-f-A-M-F-Q",
            category: "aircraft",
        },
        FlightPattern {
            name: "SCOUT-1",
            pattern_type: PatternType::Figure8,
            center_lat: ATLANTA_LAT, // Over downtown
            center_lon: ATLANTA_LON,
            radius_deg: 0.025,
            altitude_m: 400.0,
            speed_factor: 0.6,
            classification: "a-f-A-M-F-Q",
            category: "aircraft",
        },
        FlightPattern {
            name: "SEARCH-1",
            pattern_type: PatternType::Lawnmower,
            center_lat: ATLANTA_LAT + 0.01, // East of downtown (near airport)
            center_lon: ATLANTA_LON + 0.04,
            radius_deg: 0.03,
            altitude_m: 200.0,
            speed_factor: 0.5,
            classification: "a-f-A-M-F-Q",
            category: "aircraft",
        },
    ];

    // Initial publish
    let mut time_offset = 0u64;
    publish_flight_patterns(&node, &patterns, time_offset);
    publish_cells_and_platforms(&node);

    // Verify data was stored
    println!("\n--- Verifying stored data ---");
    match node.list_documents("cells") {
        Ok(docs) => println!("Cells: {} stored", docs.len()),
        Err(e) => println!("Error listing cells: {:?}", e),
    }
    match node.list_documents("tracks") {
        Ok(docs) => println!("Tracks: {} stored", docs.len()),
        Err(e) => println!("Error listing tracks: {:?}", e),
    }
    match node.list_documents("platforms") {
        Ok(docs) => println!("Platforms: {} stored", docs.len()),
        Err(e) => println!("Error listing platforms: {:?}", e),
    }

    // Start sync
    println!("\n--- Starting P2P sync with mDNS discovery ---");
    if let Err(e) = node.start_sync() {
        eprintln!("Failed to start sync: {:?}", e);
        return;
    }
    println!("Sync started. Initial peer count: {}", node.peer_count());

    // Keep running and update positions
    println!("\nFlying patterns over Atlanta... (Ctrl+C to exit)");
    println!("ATAK plugin should discover this node via mDNS.\n");

    let mut last_platform_count = 0;
    loop {
        std::thread::sleep(std::time::Duration::from_secs(2));
        time_offset += 2;

        let peers = node.peer_count();
        let connected = node.connected_peers();

        // Update track positions and platform positions/heartbeats
        publish_flight_patterns(&node, &patterns, time_offset);
        update_platform_positions(&node, &patterns, time_offset);

        // Check for received platforms (ATAK PLI)
        let platforms = node.get_platforms().unwrap_or_default();

        // Log new platforms received from ATAK
        if platforms.len() != last_platform_count {
            println!(
                "\n=== Received {} platforms from network ===",
                platforms.len()
            );
            for p in &platforms {
                // Skip our own platforms (they have "platform-" prefix)
                if p.id.starts_with("platform-") {
                    continue;
                }
                println!(
                    "  [PLI] {} ({}) @ {:.4}, {:.4} - {}",
                    p.name,
                    p.id,
                    p.lat,
                    p.lon,
                    p.status.as_str()
                );
            }
            println!();
            last_platform_count = platforms.len();
        }

        println!("[t={}s] Peers: {} | Tracks updated", time_offset, peers);

        if !connected.is_empty() {
            println!("  Connected: {:?}", connected);
        }
    }
}

fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Calculate position for a given flight pattern at a given time
fn calculate_position(pattern: &FlightPattern, time_secs: u64) -> (f64, f64, f64) {
    let t = (time_secs as f64) * pattern.speed_factor * 0.1; // Scale time for smooth movement

    let (lat_offset, lon_offset) = match pattern.pattern_type {
        PatternType::Orbit => {
            // Simple circular orbit
            let angle = t % (2.0 * PI);
            (
                pattern.radius_deg * angle.sin(),
                pattern.radius_deg * angle.cos(),
            )
        }
        PatternType::Racetrack => {
            // Oval racetrack: two semicircles connected by straight segments
            let cycle = t % (2.0 * PI);
            if cycle < PI / 2.0 {
                // First straight
                let progress = cycle / (PI / 2.0);
                (
                    pattern.radius_deg * 0.5,
                    pattern.radius_deg * (progress - 0.5),
                )
            } else if cycle < PI {
                // First turn
                let angle = (cycle - PI / 2.0) * 2.0;
                (
                    pattern.radius_deg * 0.5 * angle.cos(),
                    pattern.radius_deg * 0.5 + pattern.radius_deg * 0.5 * angle.sin(),
                )
            } else if cycle < 3.0 * PI / 2.0 {
                // Second straight
                let progress = (cycle - PI) / (PI / 2.0);
                (
                    -pattern.radius_deg * 0.5,
                    pattern.radius_deg * (0.5 - progress),
                )
            } else {
                // Second turn
                let angle = (cycle - 3.0 * PI / 2.0) * 2.0 + PI;
                (
                    pattern.radius_deg * 0.5 * angle.cos(),
                    -pattern.radius_deg * 0.5 + pattern.radius_deg * 0.5 * angle.sin(),
                )
            }
        }
        PatternType::Figure8 => {
            // Figure-8 using lemniscate of Bernoulli
            let angle = t % (2.0 * PI);
            let denom = 1.0 + angle.sin().powi(2);
            (
                pattern.radius_deg * angle.sin() / denom,
                pattern.radius_deg * angle.sin() * angle.cos() / denom,
            )
        }
        PatternType::Lawnmower => {
            // Back and forth search pattern
            let row_time = 10.0; // seconds per row
            let total_rows = 6.0;
            let cycle_time = row_time * total_rows;
            let t_cycle = t % cycle_time;
            let row = (t_cycle / row_time).floor();
            let row_progress = (t_cycle % row_time) / row_time;

            let lat_offset = pattern.radius_deg * (row / total_rows - 0.5);
            let lon_offset = if row as i32 % 2 == 0 {
                pattern.radius_deg * (row_progress - 0.5)
            } else {
                pattern.radius_deg * (0.5 - row_progress)
            };
            (lat_offset, lon_offset)
        }
    };

    let lat = pattern.center_lat + lat_offset;
    let lon = pattern.center_lon + lon_offset;
    let alt = pattern.altitude_m;

    (lat, lon, alt)
}

/// Calculate heading based on movement direction
fn calculate_heading(pattern: &FlightPattern, time_secs: u64) -> f64 {
    let (lat1, lon1, _) = calculate_position(pattern, time_secs);
    let (lat2, lon2, _) = calculate_position(pattern, time_secs + 1);

    let dlat = lat2 - lat1;
    let dlon = lon2 - lon1;

    let heading_rad = dlon.atan2(dlat);
    let heading_deg = heading_rad.to_degrees();

    // Normalize to 0-360
    if heading_deg < 0.0 {
        heading_deg + 360.0
    } else {
        heading_deg
    }
}

/// Publish flight pattern tracks
fn publish_flight_patterns(node: &hive_ffi::HiveNode, patterns: &[FlightPattern], time_secs: u64) {
    let now = current_timestamp();

    for (i, pattern) in patterns.iter().enumerate() {
        let (lat, lon, alt) = calculate_position(pattern, time_secs);
        let heading = calculate_heading(pattern, time_secs);

        let track_id = format!("track-{}", pattern.name.to_lowercase().replace('-', "_"));

        let track = serde_json::json!({
            "id": track_id,
            "source_platform": format!("platform-{}", pattern.name.to_lowercase()),
            "cell_id": "cell-atlanta-001",
            "formation_id": "atlanta-isr",
            "lat": lat,
            "lon": lon,
            "hae": alt,
            "heading": heading,
            "speed": 25.0 + (i as f64 * 5.0),  // Varying speeds
            "classification": pattern.classification,
            "confidence": 0.95,
            "category": pattern.category,
            "attributes": {
                "callsign": pattern.name,
                "pattern": format!("{:?}", pattern.pattern_type),
                "altitude_ft": (alt * 3.28084) as i32
            },
            "created_at": now,
            "last_update": now
        });

        let json = track.to_string();
        if let Err(e) = node.put_document("tracks", &track_id, &json) {
            eprintln!("Error publishing track {}: {:?}", track_id, e);
        } else {
            // Sync the document to connected peers
            if let Err(e) = node.sync_document("tracks", &track_id) {
                // Only log at debug level - sync may fail if no peers connected yet
                if node.peer_count() > 0 {
                    eprintln!("Error syncing track {}: {:?}", track_id, e);
                }
            }
        }
    }
}

/// Publish cells and platforms (static data)
fn publish_cells_and_platforms(node: &hive_ffi::HiveNode) {
    println!("--- Publishing cells and platforms ---");

    // Publish Atlanta cell
    let cell = serde_json::json!({
        "id": "cell-atlanta-001",
        "name": "Atlanta ISR Cell",
        "status": "active",
        "platform_count": 4,
        "center_lat": ATLANTA_LAT,
        "center_lon": ATLANTA_LON,
        "capabilities": ["ISR", "SURVEILLANCE", "RECON"],
        "formation_id": "atlanta-isr",
        "leader_id": "platform-hawk_1",
        "last_update": current_timestamp()
    });

    let json = cell.to_string();
    match node.put_document("cells", "cell-atlanta-001", &json) {
        Ok(()) => println!("  Published cell: cell-atlanta-001"),
        Err(e) => eprintln!("  Error publishing cell: {:?}", e),
    }

    // Publish platforms
    let platforms = vec![
        (
            "HAWK-1",
            "UAV",
            ATLANTA_LAT + 0.02,
            ATLANTA_LON - 0.01,
            300.0,
        ),
        (
            "HAWK-2",
            "UAV",
            ATLANTA_LAT - 0.015,
            ATLANTA_LON + 0.02,
            250.0,
        ),
        ("SCOUT-1", "UAV", ATLANTA_LAT, ATLANTA_LON, 400.0),
        (
            "SEARCH-1",
            "UAV",
            ATLANTA_LAT + 0.01,
            ATLANTA_LON + 0.04,
            200.0,
        ),
    ];

    for (name, ptype, lat, lon, alt) in platforms {
        let platform_id = format!("platform-{}", name.to_lowercase().replace('-', "_"));
        let platform = serde_json::json!({
            "id": platform_id,
            "name": name,
            "platform_type": ptype,
            "lat": lat,
            "lon": lon,
            "hae": alt,
            "readiness": 0.95,
            "cell_id": "cell-atlanta-001",
            "capabilities": ["ISR", "EO/IR"],
            "status": "active",
            "last_heartbeat": current_timestamp()
        });

        let json = platform.to_string();
        match node.put_document("platforms", &platform_id, &json) {
            Ok(()) => println!("  Published platform: {}", platform_id),
            Err(e) => eprintln!("  Error publishing platform {}: {:?}", platform_id, e),
        }
    }
}

/// Update platform positions and heartbeats (called every refresh cycle)
fn update_platform_positions(
    node: &hive_ffi::HiveNode,
    patterns: &[FlightPattern],
    time_secs: u64,
) {
    let now = current_timestamp();

    for pattern in patterns {
        let (lat, lon, alt) = calculate_position(pattern, time_secs);
        let heading = calculate_heading(pattern, time_secs);

        let platform_id = format!("platform-{}", pattern.name.to_lowercase().replace('-', "_"));

        let platform = serde_json::json!({
            "id": platform_id,
            "name": pattern.name,
            "platform_type": "UAV",
            "lat": lat,
            "lon": lon,
            "hae": alt,
            "heading": heading,
            "speed": 25.0,
            "readiness": 0.95,
            "cell_id": "cell-atlanta-001",
            "capabilities": ["ISR", "EO/IR"],
            "status": "active",
            "last_heartbeat": now
        });

        let json = platform.to_string();
        if let Err(e) = node.put_document("platforms", &platform_id, &json) {
            eprintln!("Error updating platform {}: {:?}", platform_id, e);
        }
    }
}
