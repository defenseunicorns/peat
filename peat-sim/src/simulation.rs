//! Simulation module for dynamic node behavior in the 48-node demo.
//!
//! Provides position seeding, movement simulation, platform type assignment,
//! and capability generation for a mixed human/robot company.

use peat_protocol::models::capability::{Capability, CapabilityExt, CapabilityType};
use rand::Rng;

// ============================================================================
// Geo math
// ============================================================================

const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// Great-circle distance between two lat/lon points in meters.
pub fn haversine_distance(p1: (f64, f64), p2: (f64, f64)) -> f64 {
    let lat1 = p1.0.to_radians();
    let lat2 = p2.0.to_radians();
    let dlat = (p2.0 - p1.0).to_radians();
    let dlon = (p2.1 - p1.1).to_radians();

    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();

    EARTH_RADIUS_M * c
}

/// Bearing from p1 to p2 in degrees (0 = North, 90 = East).
pub fn bearing(p1: (f64, f64), p2: (f64, f64)) -> f64 {
    let lat1 = p1.0.to_radians();
    let lat2 = p2.0.to_radians();
    let dlon = (p2.1 - p1.1).to_radians();

    let x = dlon.sin() * lat2.cos();
    let y = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();

    (x.atan2(y).to_degrees() + 360.0) % 360.0
}

/// Offset a lat/lon point by a bearing (degrees) and distance (meters).
pub fn offset_position(start: (f64, f64), bearing_deg: f64, distance_m: f64) -> (f64, f64) {
    let lat1 = start.0.to_radians();
    let lon1 = start.1.to_radians();
    let brng = bearing_deg.to_radians();
    let d = distance_m / EARTH_RADIUS_M;

    let lat2 = (lat1.sin() * d.cos() + lat1.cos() * d.sin() * brng.cos()).asin();
    let lon2 =
        lon1 + (brng.sin() * d.sin() * lat1.cos()).atan2(d.cos() - lat1.sin() * lat2.sin());

    (lat2.to_degrees(), lon2.to_degrees())
}

// ============================================================================
// Position seeding
// ============================================================================

/// Configurable center point + deterministic layout from node IDs.
#[derive(Clone, Copy)]
pub struct PositionSeed {
    pub center_lat: f64,
    pub center_lon: f64,
}

impl PositionSeed {
    /// Read center from env vars, defaulting to Point Loma, San Diego.
    pub fn from_env() -> Self {
        let center_lat: f64 = std::env::var("DEMO_CENTER_LAT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(32.6723);
        let center_lon: f64 = std::env::var("DEMO_CENTER_LON")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(-117.2425);
        Self {
            center_lat,
            center_lon,
        }
    }

    /// Derive initial position for a node based on its structured node ID.
    ///
    /// Layout (~2km spread):
    /// - Platoon 1: 500m North, Platoon 2: 500m South
    /// - 3 squads per platoon at 120° intervals, 200m from platoon center
    /// - Nodes: 30m radius circle around squad center
    /// - Leaders: at their unit's center
    pub fn initial_position(&self, node_id: &str) -> (f64, f64) {
        let center = (self.center_lat, self.center_lon);
        let parts: Vec<&str> = node_id.split('-').collect();

        // company-ALPHA-commander
        if node_id.ends_with("-commander") {
            return center;
        }

        // Extract platoon index (1-based)
        let platoon_idx = parts
            .iter()
            .position(|&p| p == "platoon")
            .and_then(|i| parts.get(i + 1))
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(1);

        // Platoon offset: 1 = North-Northeast (30°), 2 = North-Northwest (330°)
        // Both platoons go north to avoid putting squads in the ocean at Point Loma
        let platoon_bearing = if platoon_idx == 1 { 30.0 } else { 330.0 };
        let platoon_center = offset_position(center, platoon_bearing, 500.0);

        // company-ALPHA-platoon-N-leader
        if node_id.ends_with("-leader") && !node_id.contains("squad") {
            return platoon_center;
        }

        // Extract squad index (1-based)
        let squad_idx = parts
            .iter()
            .position(|&p| p == "squad")
            .and_then(|i| parts.get(i + 1))
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(1);

        // Squad offset: 3 squads at 120° intervals (300°, 60°, 180°)
        let squad_bearings = [300.0, 60.0, 180.0];
        let squad_bearing = squad_bearings[(squad_idx - 1).min(2)];
        let squad_center = offset_position(platoon_center, squad_bearing, 200.0);

        // Squad leader
        if node_id.ends_with("-leader") {
            return squad_center;
        }

        // Extract soldier index (1-based)
        let soldier_idx = parts
            .last()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(1);

        // Soldiers: evenly spaced on a 30m circle around squad center
        let angle = (soldier_idx as f64 - 1.0) * (360.0 / 6.0);
        offset_position(squad_center, angle, 30.0)
    }

    /// Get the squad center for a given node (for patrol bounds).
    pub fn squad_center(&self, node_id: &str) -> (f64, f64) {
        // Derive the squad leader ID and get its position
        let parts: Vec<&str> = node_id.split('-').collect();
        if let Some(soldier_pos) = parts.iter().position(|&p| p == "soldier") {
            let leader_id: String = parts[..soldier_pos].join("-") + "-leader";
            return self.initial_position(&leader_id);
        }
        // Fallback: use own position
        self.initial_position(node_id)
    }
}

// ============================================================================
// Platform type assignment
// ============================================================================

/// Platform types for the mixed human/robot company.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlatformType {
    Soldier,
    Ugv,
    Uav,
}

impl PlatformType {
    pub fn as_str(&self) -> &'static str {
        match self {
            PlatformType::Soldier => "SOLDIER",
            PlatformType::Ugv => "UGV",
            PlatformType::Uav => "UAV",
        }
    }

    /// Walking/patrol speed in m/s.
    pub fn speed_mps(&self) -> f64 {
        match self {
            PlatformType::Soldier => 1.4,
            PlatformType::Ugv => 3.0,
            PlatformType::Uav => 8.0,
        }
    }

    /// Patrol radius in meters.
    pub fn patrol_radius_m(&self) -> f64 {
        match self {
            PlatformType::Soldier => 50.0,
            PlatformType::Ugv => 80.0,
            PlatformType::Uav => 150.0,
        }
    }

    /// Fuel/battery drain per tick (minutes lost).
    /// Soldiers: ~5.5hr patrol endurance. UGV: ~3.3hr battery. UAV: ~2hr flight time.
    pub fn fuel_drain_per_tick(&self) -> f64 {
        match self {
            PlatformType::Soldier => 0.3,  // ~330 min endurance
            PlatformType::Ugv => 0.5,      // ~200 min battery
            PlatformType::Uav => 0.8,      // ~125 min flight time
        }
    }

    /// Default altitude in meters (for UAVs).
    pub fn default_altitude(&self) -> f64 {
        match self {
            PlatformType::Uav => 120.0,
            _ => 0.0,
        }
    }
}

/// Assign platform type based on soldier index within squad.
/// Per squad (6 nodes): soldier-1..4 = Soldier, soldier-5 = UGV, soldier-6 = UAV.
pub fn assign_platform_type(node_id: &str) -> PlatformType {
    let parts: Vec<&str> = node_id.split('-').collect();
    let soldier_idx = parts
        .last()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1);

    // Only actual soldier nodes get robot assignments; leaders stay as-is
    if !node_id.contains("soldier") {
        return PlatformType::Soldier;
    }

    match soldier_idx {
        5 => PlatformType::Ugv,
        6 => PlatformType::Uav,
        _ => PlatformType::Soldier,
    }
}

// ============================================================================
// Dynamic node state
// ============================================================================

/// Per-node simulation state with movement, fuel drain, and health changes.
pub struct NodeSimState {
    pub position: (f64, f64),
    home: (f64, f64),
    platform_type: PlatformType,
    pub fuel_minutes: f64,
    pub health: i32, // HealthStatus enum: 1=Nominal, 2=Degraded, 3=Critical
    heading: f64,
    patrol_target: Option<(f64, f64)>,
    tick_count: u64,
}

impl NodeSimState {
    pub fn new(node_id: &str, platform_type: PlatformType, seed: &PositionSeed) -> Self {
        let position = seed.initial_position(node_id);
        let home = seed.squad_center(node_id);
        Self {
            position,
            home,
            platform_type,
            fuel_minutes: 100.0,
            health: 1, // Nominal
            heading: 0.0,
            patrol_target: None,
            tick_count: 0,
        }
    }

    /// Advance simulation by one tick.
    pub fn tick(&mut self, dt_secs: f64) {
        self.tick_count += 1;
        let mut rng = rand::thread_rng();

        // --- Movement: random walk within patrol radius of squad center ---
        let patrol_radius = self.platform_type.patrol_radius_m();
        let speed = self.platform_type.speed_mps();

        // Pick a new target if we don't have one or reached current target
        if self.patrol_target.is_none()
            || haversine_distance(self.position, self.patrol_target.unwrap()) < 3.0
        {
            let angle = rng.gen_range(0.0..360.0);
            let dist = rng.gen_range(0.0..patrol_radius);
            self.patrol_target = Some(offset_position(self.home, angle, dist));
        }

        if let Some(target) = self.patrol_target {
            self.heading = bearing(self.position, target);
            let move_dist = speed * dt_secs;
            let dist_to_target = haversine_distance(self.position, target);
            let actual_dist = move_dist.min(dist_to_target);
            self.position = offset_position(self.position, self.heading, actual_dist);
        }

        // --- Fuel drain ---
        let drain_rate: f64 = std::env::var("SIM_FUEL_DRAIN_RATE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(self.platform_type.fuel_drain_per_tick());
        self.fuel_minutes = (self.fuel_minutes - drain_rate).max(0.0);

        // --- Health changes ---
        let degrade_prob: f64 = std::env::var("SIM_HEALTH_DEGRADE_PROB")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.01);

        if self.fuel_minutes < 20.0 {
            self.health = 3; // Critical
        } else if self.health == 1 && rng.gen::<f64>() < degrade_prob {
            self.health = 2; // Degraded
        }
    }

    pub fn fuel_minutes_u32(&self) -> u32 {
        self.fuel_minutes as u32
    }

    /// Readiness score based on fuel and health (0.0 - 1.0).
    pub fn readiness(&self) -> f64 {
        let fuel_factor = (self.fuel_minutes / 100.0).min(1.0);
        let health_factor = match self.health {
            1 => 1.0,
            2 => 0.6,
            3 => 0.2,
            _ => 0.0,
        };
        fuel_factor * health_factor
    }
}

// ============================================================================
// Capability generation
// ============================================================================

/// Generate role-appropriate capabilities for a node.
pub fn generate_capabilities(node_id: &str, platform_type: PlatformType, role: &str) -> Vec<Capability> {
    let mut caps = Vec::new();

    match role {
        "company_commander" => {
            caps.push(Capability::new("comm-cmd".into(), "Tactical Radio".into(), CapabilityType::Communication, 0.99));
            caps.push(Capability::new("compute-cmd".into(), "C2 Edge Compute".into(), CapabilityType::Compute, 0.95));
        }
        "platoon_leader" => {
            caps.push(Capability::new("comm-plt".into(), "Tactical Radio".into(), CapabilityType::Communication, 0.98));
            caps.push(Capability::new("compute-plt".into(), "Edge Compute".into(), CapabilityType::Compute, 0.9));
        }
        "squad_leader" => {
            caps.push(Capability::new("comm-sql".into(), "Squad Radio".into(), CapabilityType::Communication, 0.95));
            caps.push(Capability::new("sensor-sql".into(), "Optical Sensor".into(), CapabilityType::Sensor, 0.9));
            caps.push(Capability::new("compute-sql".into(), "Edge Compute".into(), CapabilityType::Compute, 0.8));
        }
        _ => {
            // Soldier-tier nodes: capabilities vary by platform type
            match platform_type {
                PlatformType::Soldier => {
                    caps.push(Capability::new("comm-sol".into(), "PRC-163 Radio".into(), CapabilityType::Communication, 0.9));
                    caps.push(Capability::new("mob-sol".into(), "Dismounted".into(), CapabilityType::Mobility, 0.85));

                    // Specialist by soldier index
                    let idx = node_id.split('-').last().and_then(|s| s.parse::<usize>().ok()).unwrap_or(1);
                    match idx {
                        1 => {
                            caps.push(Capability::new("sensor-therm".into(), "FLIR ThermoSight".into(), CapabilityType::Sensor, 0.8));
                            caps.push(Capability::new("sensor-lrf".into(), "Laser Rangefinder".into(), CapabilityType::Sensor, 0.9));
                        }
                        2 => caps.push(Capability::new("sensor-opt".into(), "LPVO Optic".into(), CapabilityType::Sensor, 0.85)),
                        3 => {
                            caps.push(Capability::new("compute-edge".into(), "ATAK EUD".into(), CapabilityType::Compute, 0.7));
                            caps.push(Capability::new("comm-mesh".into(), "MANET Relay".into(), CapabilityType::Communication, 0.8));
                        }
                        4 => caps.push(Capability::new("payload-cas".into(), "CASEVAC Kit".into(), CapabilityType::Payload, 0.9)),
                        _ => {}
                    }
                }
                PlatformType::Ugv => {
                    // Tracked UGV — sensor-heavy ISR/logistics platform
                    caps.push(Capability::new("comm-ugv".into(), "Silvus MIMO Radio".into(), CapabilityType::Communication, 0.92));
                    caps.push(Capability::new("mob-ugv".into(), "Tracked 6x6".into(), CapabilityType::Mobility, 0.95));
                    caps.push(Capability::new("sensor-flir-ugv".into(), "FLIR Boson 640".into(), CapabilityType::Sensor, 0.94));
                    caps.push(Capability::new("sensor-lidar-ugv".into(), "LiDAR 3D (200m)".into(), CapabilityType::Sensor, 0.90));
                    caps.push(Capability::new("sensor-eo-ugv".into(), "EO/IR Gimbal 30x".into(), CapabilityType::Sensor, 0.93));
                    caps.push(Capability::new("sensor-cbrn-ugv".into(), "CBRN Detector".into(), CapabilityType::Sensor, 0.85));
                    caps.push(Capability::new("payload-ugv".into(), "Cargo Bay 200kg".into(), CapabilityType::Payload, 0.95));
                    caps.push(Capability::new("compute-ugv".into(), "Jetson AGX Orin".into(), CapabilityType::Compute, 0.88));
                }
                PlatformType::Uav => {
                    // Small tactical UAV — ISR overwatch platform
                    caps.push(Capability::new("comm-uav".into(), "C2 Datalink (5km)".into(), CapabilityType::Communication, 0.85));
                    caps.push(Capability::new("mob-uav".into(), "Quadrotor VTOL".into(), CapabilityType::Mobility, 0.95));
                    caps.push(Capability::new("sensor-flir-uav".into(), "FLIR Vue Pro R 640".into(), CapabilityType::Sensor, 0.95));
                    caps.push(Capability::new("sensor-eo-uav".into(), "EO 4K Gimbal 20x".into(), CapabilityType::Sensor, 0.95));
                    caps.push(Capability::new("sensor-mti-uav".into(), "MTI Radar (GMTI)".into(), CapabilityType::Sensor, 0.80));
                    caps.push(Capability::new("compute-uav".into(), "Edge AI (YOLOv8)".into(), CapabilityType::Compute, 0.82));
                }
            }
        }
    }

    caps
}

/// Extract capability names as strings (for backward-compat JSON fields).
pub fn capability_names(caps: &[Capability]) -> Vec<String> {
    caps.iter().map(|c| c.name.clone()).collect()
}
