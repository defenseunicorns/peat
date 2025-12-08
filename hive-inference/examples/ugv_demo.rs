//! UGV Demo - Simulated Unmanned Ground Vehicle
//!
//! Demonstrates the UGV client for the M1 vignette demo.
//!
//! Run with: cargo run --example ugv_demo
//!
//! This example:
//! 1. Creates a simulated UGV at a starting position
//! 2. Advertises capabilities
//! 3. Runs a waypoint patrol mission
//! 4. Publishes position updates as TrackUpdate messages
//! 5. Responds to simulated mission commands

use hive_inference::{MissionCommand, PatrolPattern, UgvClient, UgvConfig};
use std::time::Duration;
use tracing::{info, Level};

fn main() {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("=== HIVE UGV Demo ===");
    info!("Simulating Unmanned Ground Vehicle for M1 Vignette");

    // Create UGV configuration with patrol waypoints (Atlanta area)
    let waypoints = vec![
        (33.7749, -84.3958), // Start position
        (33.7755, -84.3950), // Waypoint 1
        (33.7760, -84.3955), // Waypoint 2
        (33.7758, -84.3962), // Waypoint 3
    ];

    let config = UgvConfig::new("UGV-Alpha-1")
        .with_position(33.7749, -84.3958)
        .with_base(33.7749, -84.3958)
        .with_waypoints(waypoints.clone())
        .with_speed(5.0); // 5 m/s

    let mut ugv = UgvClient::new(config);

    // Display initial capabilities
    info!("\n=== UGV Capabilities ===");
    for cap in ugv.get_capabilities() {
        info!(
            "  - {} ({:?}): confidence={:.2}",
            cap.name, cap.capability_type, cap.confidence
        );
    }

    // Start patrol mission
    info!("\n=== Starting Patrol Mission ===");
    ugv.handle_mission(MissionCommand::SearchArea {
        boundary: waypoints,
        patrol_pattern: PatrolPattern::Sequential,
    });

    // Simulate 30 seconds of patrol
    let update_interval = Duration::from_millis(500);
    let sim_duration = Duration::from_secs(30);
    let mut elapsed = Duration::ZERO;
    let mut last_position_log = Duration::ZERO;

    info!(
        "Running simulation for {} seconds...",
        sim_duration.as_secs()
    );
    info!(
        "Position updates every {} ms\n",
        update_interval.as_millis()
    );

    while elapsed < sim_duration {
        // Update UGV state
        ugv.update(update_interval);
        elapsed += update_interval;

        // Log position every 2 seconds
        if elapsed - last_position_log >= Duration::from_secs(2) {
            let _track = ugv.get_position_update();
            let (lat, lon) = ugv.position();
            info!(
                "[{:>5.1}s] {} | Pos: ({:.5}, {:.5}) | Heading: {:.1}° | Battery: {:.0}%",
                elapsed.as_secs_f32(),
                ugv.state(),
                lat,
                lon,
                ugv.heading(),
                ugv.battery_level() * 100.0
            );
            last_position_log = elapsed;
        }

        // Simulate receiving a track target command after 15 seconds
        if elapsed == Duration::from_secs(15) {
            info!("\n=== Received TRACK_TARGET Command ===");
            info!("Target detected at (33.7765, -84.3945)");
            ugv.handle_mission(MissionCommand::TrackTarget {
                target_id: "TRK-001".to_string(),
                last_known_position: (33.7765, -84.3945),
            });
        }

        // Simulate abort after 25 seconds
        if elapsed == Duration::from_secs(25) {
            info!("\n=== Received ABORT Command ===");
            ugv.handle_mission(MissionCommand::Abort);
        }
    }

    // Final status
    info!("\n=== Simulation Complete ===");
    let final_track = ugv.get_position_update();
    info!("Final Track ID: {}", final_track.track_id);
    info!(
        "Final Position: ({:.5}, {:.5})",
        final_track.position.lat, final_track.position.lon
    );
    info!("Final State: {}", ugv.state());
    info!("Battery Level: {:.1}%", ugv.battery_level() * 100.0);

    // Show final TrackUpdate JSON (for HIVE integration)
    info!("\n=== Sample TrackUpdate JSON ===");
    let json = serde_json::to_string_pretty(&final_track).unwrap();
    println!("{}", json);
}
