use peat_inference::{
    AiModelInfo, AiModelPlatform, AuthorityLevel, Coordinator, OperatorPlatform, Platform,
    SensorCapability, Team, VehiclePlatform,
};

fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();

    tracing::info!("Peat M1 POC - Object Tracking Demo");

    // Create a coordinator for formation-level aggregation
    // Note: In a real deployment, the Peat protocol's CRDT mesh handles
    // all network connectivity and document synchronization automatically.
    let mut coordinator = Coordinator::new("M1 Coordinator");

    // Create Team Alpha: 1 operator + 1 UAV + 1 AI model
    let mut alpha = Team::new("Alpha Team");
    alpha.add_member(Platform::Operator(
        OperatorPlatform::new("Alpha-1", "ALPHA-1", AuthorityLevel::Commander)
            .with_position(33.7749, -84.3958),
    ));
    alpha.add_member(Platform::Vehicle(
        VehiclePlatform::new_uav("Alpha-2")
            .with_sensor(SensorCapability::eo_ir("4K", 45.0, 5000.0))
            .with_position(33.7749, -84.3958, 100.0),
    ));
    alpha.add_member(Platform::AiModel(
        AiModelPlatform::new("Alpha-3", AiModelInfo::object_tracker("1.3.0"))
            .with_sensor_source("Alpha-2"),
    ));

    // Create Team Bravo: 1 operator + 1 UGV + 1 AI model
    let mut bravo = Team::new("Bravo Team");
    bravo.add_member(Platform::Operator(
        OperatorPlatform::new("Bravo-1", "BRAVO-1", AuthorityLevel::Commander)
            .with_position(33.7850, -84.3900),
    ));
    bravo.add_member(Platform::Vehicle(
        VehiclePlatform::new_ugv("Bravo-2")
            .with_sensor(SensorCapability::camera("1920x1080", 60.0, 30.0))
            .with_position(33.7850, -84.3900, 0.0),
    ));
    bravo.add_member(Platform::AiModel(
        AiModelPlatform::new("Bravo-3", AiModelInfo::object_tracker("1.3.0"))
            .with_sensor_source("Bravo-2"),
    ));

    // Register teams with coordinator
    // In production, teams are discovered via Peat mesh document sync
    coordinator.register_team(alpha);
    coordinator.register_team(bravo);

    tracing::info!(
        "Coordinator '{}' managing {} teams ({} platforms)",
        coordinator.name,
        coordinator.team_count(),
        coordinator.total_platforms()
    );

    // Display formation-level capability summary
    let summary = coordinator.get_formation_summary();
    tracing::info!("Formation Summary:");
    tracing::info!("  Total cameras: {}", summary.total_cameras);
    tracing::info!(
        "  Total trackers: {} ({})",
        summary.total_trackers,
        summary.tracker_versions.join(", ")
    );
    tracing::info!("  Coverage: {}", summary.coverage_sectors.join(" + "));
    tracing::info!("  Readiness: {:.1}%", summary.readiness_score * 100.0);

    for team in &coordinator.teams {
        tracing::info!("  Team '{}': {} members", team.name, team.member_count());
        for member in &team.members {
            let caps = member.get_capabilities();
            tracing::info!(
                "    - {} ({}) with {} capabilities",
                member.name(),
                member.platform_type(),
                caps.len()
            );
        }
    }
}
