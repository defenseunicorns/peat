//! Test fixtures for M1 vignette E2E testing
//!
//! Provides pre-configured platform, team, and C2 fixtures.

use crate::messages::{
    CommandType, OperationalBoundary, Position, Priority, TrackCommand, TrackUpdate,
};
use crate::platform::{
    AiModelInfo, AiModelPlatform, AuthorityLevel, OperatorPlatform, Platform, SensorCapability,
    VehiclePlatform,
};
use crate::team::Team;
use std::time::Instant;
use uuid::Uuid;

/// A complete team fixture with operator, vehicle, and AI model
#[derive(Debug)]
pub struct TeamFixture {
    /// Team name (e.g., "Alpha", "Bravo")
    pub name: String,

    /// The team container
    pub team: Team,

    /// Operator platform
    pub operator: OperatorPlatform,

    /// Vehicle platform (UGV or UAV)
    pub vehicle: VehiclePlatform,

    /// AI model platform
    pub ai_model: AiModelPlatform,

    /// Network identifier (for simulating isolated networks)
    pub network_id: String,
}

impl TeamFixture {
    /// Create Team Alpha (UAV-based, Network A)
    pub fn alpha() -> Self {
        let operator = OperatorPlatform::new("Alpha-1", "ALPHA-1", AuthorityLevel::Commander)
            .with_tak_device("ATAK")
            .with_position(33.7749, -84.3958);

        let vehicle = VehiclePlatform::new_uav("Alpha-2")
            .with_sensor(SensorCapability::eo_ir("4K", 45.0, 5000.0))
            .with_position(33.7749, -84.3958, 100.0);

        let ai_model = AiModelPlatform::new(
            "Alpha-3",
            AiModelInfo::object_tracker("1.3.0").with_hash("sha256:alpha123"),
        )
        .with_sensor_source("Alpha-2")
        .with_memory(4096);

        let mut team = Team::new("Alpha Team");
        team.add_member(Platform::Operator(operator.clone()));
        team.add_member(Platform::Vehicle(vehicle.clone()));
        team.add_member(Platform::AiModel(ai_model.clone()));

        Self {
            name: "Alpha".to_string(),
            team,
            operator,
            vehicle,
            ai_model,
            network_id: "network-a".to_string(),
        }
    }

    /// Create Team Bravo (UGV-based, Network B)
    pub fn bravo() -> Self {
        let operator = OperatorPlatform::new("Bravo-1", "BRAVO-1", AuthorityLevel::Commander)
            .with_tak_device("ATAK")
            .with_position(33.7850, -84.3900);

        let vehicle = VehiclePlatform::new_ugv("Bravo-2")
            .with_sensor(SensorCapability::camera("1920x1080", 60.0, 30.0))
            .with_position(33.7850, -84.3900, 0.0);

        let ai_model = AiModelPlatform::new(
            "Bravo-3",
            AiModelInfo::object_tracker("1.3.0").with_hash("sha256:bravo456"),
        )
        .with_sensor_source("Bravo-2")
        .with_memory(4096);

        let mut team = Team::new("Bravo Team");
        team.add_member(Platform::Operator(operator.clone()));
        team.add_member(Platform::Vehicle(vehicle.clone()));
        team.add_member(Platform::AiModel(ai_model.clone()));

        Self {
            name: "Bravo".to_string(),
            team,
            operator,
            vehicle,
            ai_model,
            network_id: "network-b".to_string(),
        }
    }

    /// Get all platform IDs in this team
    pub fn platform_ids(&self) -> Vec<String> {
        vec![
            self.operator.id.clone(),
            self.vehicle.id.clone(),
            self.ai_model.id.clone(),
        ]
    }

    /// Generate a track update from this team's AI model
    pub fn generate_track_update(&self, track_id: &str, classification: &str) -> TrackUpdate {
        let position = self.vehicle.position.unwrap_or((33.78, -84.39, 0.0));

        TrackUpdate::new(
            track_id,
            classification,
            0.89,
            Position::with_cep(position.0, position.1, 2.5),
            &self.vehicle.name,
            &self.ai_model.name,
            &self.ai_model.model.version,
        )
    }
}

/// Coordinator/Bridge fixture for cross-network communication
#[derive(Debug)]
pub struct CoordinatorFixture {
    /// Coordinator ID
    pub id: String,

    /// Coordinator name
    pub name: String,

    /// Connected network IDs
    pub networks: Vec<String>,

    /// Registered teams
    pub teams: Vec<String>,

    /// Bridge mode (true if dual-homed)
    pub is_bridge: bool,
}

impl CoordinatorFixture {
    /// Create a new coordinator fixture
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            networks: Vec::new(),
            teams: Vec::new(),
            is_bridge: false,
        }
    }

    /// Create a bridge coordinator connecting two networks
    pub fn bridge(name: impl Into<String>, network_a: &str, network_b: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            networks: vec![network_a.to_string(), network_b.to_string()],
            teams: Vec::new(),
            is_bridge: true,
        }
    }

    /// Register a team with this coordinator
    pub fn register_team(&mut self, team_name: &str) {
        if !self.teams.contains(&team_name.to_string()) {
            self.teams.push(team_name.to_string());
        }
    }

    /// Check if coordinator connects two networks
    pub fn connects(&self, network_a: &str, network_b: &str) -> bool {
        self.networks.contains(&network_a.to_string())
            && self.networks.contains(&network_b.to_string())
    }
}

/// Simulated C2 element for testing
#[derive(Debug)]
pub struct SimulatedC2 {
    /// C2 identifier
    pub id: String,

    /// C2 name
    pub name: String,

    /// Authority identifier
    pub authority: String,

    /// Issued commands
    commands_issued: Vec<TrackCommand>,

    /// Received track updates
    tracks_received: Vec<TrackUpdate>,

    /// Command issue timestamps (for latency measurement)
    command_timestamps: Vec<(Uuid, Instant)>,
}

impl SimulatedC2 {
    /// Create a new simulated C2
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            authority: "C2-Commander".to_string(),
            commands_issued: Vec::new(),
            tracks_received: Vec::new(),
            command_timestamps: Vec::new(),
        }
    }

    /// Issue a TRACK_TARGET command
    pub fn issue_track_command(
        &mut self,
        target_description: &str,
        priority: Priority,
        boundary: Option<OperationalBoundary>,
    ) -> TrackCommand {
        let mut cmd = TrackCommand::new(target_description, priority, &self.authority);

        if let Some(b) = boundary {
            cmd = cmd.with_boundary(b);
        }

        // Record timestamp for latency measurement
        self.command_timestamps
            .push((cmd.command_id, Instant::now()));
        self.commands_issued.push(cmd.clone());

        cmd
    }

    /// Issue a cancel track command
    pub fn issue_cancel_command(&mut self, track_id: &str) -> TrackCommand {
        let mut cmd = TrackCommand::new(
            format!("Cancel track {}", track_id),
            Priority::Normal,
            &self.authority,
        );
        cmd.command_type = CommandType::CancelTrack;

        self.commands_issued.push(cmd.clone());
        cmd
    }

    /// Receive a track update
    pub fn receive_track(&mut self, update: TrackUpdate) {
        self.tracks_received.push(update);
    }

    /// Get command issue timestamp for latency calculation
    pub fn get_command_timestamp(&self, command_id: &Uuid) -> Option<Instant> {
        self.command_timestamps
            .iter()
            .find(|(id, _)| id == command_id)
            .map(|(_, ts)| *ts)
    }

    /// Get number of commands issued
    pub fn command_count(&self) -> usize {
        self.commands_issued.len()
    }

    /// Get number of tracks received
    pub fn track_count(&self) -> usize {
        self.tracks_received.len()
    }

    /// Get all received tracks
    pub fn tracks(&self) -> &[TrackUpdate] {
        &self.tracks_received
    }

    /// Get the latest track update for a given track ID
    pub fn get_latest_track(&self, track_id: &str) -> Option<&TrackUpdate> {
        self.tracks_received
            .iter()
            .rev()
            .find(|t| t.track_id == track_id)
    }

    /// Clear received tracks (for test reset)
    pub fn clear_tracks(&mut self) {
        self.tracks_received.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::PlatformType;

    #[test]
    fn test_team_alpha_fixture() {
        let alpha = TeamFixture::alpha();

        assert_eq!(alpha.name, "Alpha");
        assert_eq!(alpha.team.member_count(), 3);
        assert_eq!(alpha.operator.authority, AuthorityLevel::Commander);
        assert_eq!(alpha.vehicle.vehicle_type, PlatformType::Uav);
        assert_eq!(alpha.network_id, "network-a");
    }

    #[test]
    fn test_team_bravo_fixture() {
        let bravo = TeamFixture::bravo();

        assert_eq!(bravo.name, "Bravo");
        assert_eq!(bravo.team.member_count(), 3);
        assert_eq!(bravo.vehicle.vehicle_type, PlatformType::Ugv);
        assert_eq!(bravo.network_id, "network-b");
    }

    #[test]
    fn test_team_generates_track_update() {
        let alpha = TeamFixture::alpha();
        let update = alpha.generate_track_update("TRACK-001", "person");

        assert_eq!(update.track_id, "TRACK-001");
        assert_eq!(update.classification, "person");
        assert_eq!(update.source_platform, "Alpha-2");
        assert_eq!(update.source_model, "Alpha-3");
    }

    #[test]
    fn test_coordinator_bridge() {
        let mut coord = CoordinatorFixture::bridge("M1-Bridge", "network-a", "network-b");

        assert!(coord.is_bridge);
        assert!(coord.connects("network-a", "network-b"));
        assert!(!coord.connects("network-a", "network-c"));

        coord.register_team("Alpha");
        coord.register_team("Bravo");
        assert_eq!(coord.teams.len(), 2);
    }

    #[test]
    fn test_simulated_c2() {
        let mut c2 = SimulatedC2::new("TAK Server");

        let cmd = c2.issue_track_command("Adult male, blue jacket", Priority::High, None);

        assert_eq!(c2.command_count(), 1);
        assert!(c2.get_command_timestamp(&cmd.command_id).is_some());

        let track = TrackUpdate::new(
            "TRACK-001",
            "person",
            0.89,
            Position::new(33.77, -84.39),
            "Alpha-2",
            "Alpha-3",
            "1.3.0",
        );

        c2.receive_track(track);
        assert_eq!(c2.track_count(), 1);
        assert!(c2.get_latest_track("TRACK-001").is_some());
    }
}
