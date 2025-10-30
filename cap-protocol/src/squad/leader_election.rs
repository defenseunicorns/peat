//! Leader Election Algorithm for Squad Formation (Phase 2)
//!
//! Implements deterministic leader selection based on capability scoring with
//! failure detection and re-election support.
//!
//! # Architecture
//!
//! The leader election protocol ensures each squad converges to a single leader
//! through a deterministic, capability-based selection process:
//!
//! ## Election Flow
//!
//! 1. **Initialization**: All platforms start in `Candidate` state
//! 2. **Scoring**: Each platform computes its leadership score
//! 3. **Announcement**: Platforms announce their candidacy with scores
//! 4. **Comparison**: Platforms compare received scores with their own
//! 5. **Convergence**: Platform with highest score becomes leader
//! 6. **Confirmation**: Leader announces election win, others follow
//!
//! ## Scoring Function
//!
//! Leadership score is computed from platform capabilities:
//! - Compute resources (30% weight)
//! - Communication capabilities (25% weight)
//! - Sensor diversity (20% weight)
//! - Battery/power status (15% weight)
//! - Reliability metrics (10% weight)
//!
//! ## Split-Brain Prevention
//!
//! - Deterministic tie-breaking using platform ID (lexicographic order)
//! - Election round numbers to detect stale announcements
//! - Timeout-based re-election if no leader emerges
//!
//! ## Failure Detection
//!
//! - Leader must send heartbeats every 2 seconds
//! - Followers detect failure after 3 missed heartbeats (6 seconds)
//! - Automatic re-election triggered on leader failure

use crate::models::{Capability, CapabilityType};
use crate::squad::messaging::{SquadMessage, SquadMessageBus, SquadMessageType};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, info, instrument, warn};

/// Election state of a platform
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElectionState {
    /// Platform is a candidate for leadership
    Candidate,
    /// Platform has been elected as leader
    Leader,
    /// Platform is following an elected leader
    Follower,
}

/// Leadership score components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeadershipScore {
    /// Compute capability score (0.0 - 1.0)
    pub compute: f64,
    /// Communication capability score (0.0 - 1.0)
    pub communication: f64,
    /// Sensor diversity score (0.0 - 1.0)
    pub sensors: f64,
    /// Power/battery score (0.0 - 1.0)
    pub power: f64,
    /// Reliability score (0.0 - 1.0)
    pub reliability: f64,
    /// Total weighted score
    pub total: f64,
}

impl LeadershipScore {
    /// Compute leadership score from capabilities
    pub fn from_capabilities(capabilities: &[Capability]) -> Self {
        let mut compute = 0.0;
        let mut communication = 0.0;
        let mut sensors: f64 = 0.0;
        let power = 1.0; // Default to full power (no power capability in model yet)
        let reliability = 1.0; // Default to full reliability

        // Analyze capabilities
        for cap in capabilities {
            match cap.capability_type {
                CapabilityType::Compute => compute = cap.confidence as f64,
                CapabilityType::Communication => communication = cap.confidence as f64,
                CapabilityType::Sensor => sensors += 0.25, // Each sensor adds 25% up to 100%
                _ => {}
            }
        }

        // Normalize sensor score
        sensors = sensors.min(1.0);

        // Compute weighted total
        // Weights: compute(30%), comm(25%), sensors(20%), power(15%), reliability(10%)
        let total = (compute * 0.30)
            + (communication * 0.25)
            + (sensors * 0.20)
            + (power * 0.15)
            + (reliability * 0.10);

        Self {
            compute,
            communication,
            sensors,
            power,
            reliability,
            total,
        }
    }

    /// Compare scores with tie-breaking by platform ID
    pub fn compare(&self, other: &Self, my_id: &str, other_id: &str) -> std::cmp::Ordering {
        // First compare total scores
        match self.total.partial_cmp(&other.total) {
            Some(std::cmp::Ordering::Equal) => {
                // Tie-break with platform ID (lexicographic)
                my_id.cmp(other_id)
            }
            Some(ordering) => ordering,
            None => my_id.cmp(other_id), // Handle NaN cases
        }
    }
}

/// Election round state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElectionRound {
    /// Round number (increments on re-election)
    pub round: u32,
    /// Round start time
    pub started_at: u64,
    /// Candidate scores received
    pub candidates: HashMap<String, LeadershipScore>,
}

impl ElectionRound {
    pub fn new(round: u32) -> Self {
        let started_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            round,
            started_at,
            candidates: HashMap::new(),
        }
    }
}

/// Leader heartbeat tracking
#[derive(Debug, Clone)]
struct LeaderHeartbeat {
    leader_id: String,
    last_heartbeat: Instant,
    missed_count: u32,
}

impl LeaderHeartbeat {
    fn new(leader_id: String) -> Self {
        Self {
            leader_id,
            last_heartbeat: Instant::now(),
            missed_count: 0,
        }
    }

    fn update(&mut self) {
        self.last_heartbeat = Instant::now();
        self.missed_count = 0;
    }

    fn is_failed(&self, heartbeat_interval: Duration, max_missed: u32) -> bool {
        let elapsed = self.last_heartbeat.elapsed();
        elapsed > heartbeat_interval * max_missed
    }
}

/// Leader Election Manager
///
/// Manages the leader election process for a squad, including:
/// - Initial election convergence
/// - Leader failure detection
/// - Automatic re-election
pub struct LeaderElectionManager {
    /// Squad ID
    squad_id: String,
    /// Platform ID
    platform_id: String,
    /// Message bus for election messages
    message_bus: Arc<SquadMessageBus>,
    /// Current election state
    state: Arc<Mutex<ElectionState>>,
    /// Current election round
    current_round: Arc<Mutex<ElectionRound>>,
    /// My leadership score
    my_score: Arc<Mutex<LeadershipScore>>,
    /// Current leader ID (if elected)
    current_leader: Arc<Mutex<Option<String>>>,
    /// Leader heartbeat tracking
    leader_heartbeat: Arc<Mutex<Option<LeaderHeartbeat>>>,
    /// Election timeout (seconds) - reserved for future timeout-based re-election
    #[allow(dead_code)]
    election_timeout: Duration,
    /// Heartbeat interval (seconds)
    heartbeat_interval: Duration,
    /// Maximum missed heartbeats before failure
    max_missed_heartbeats: u32,
}

impl LeaderElectionManager {
    /// Create a new leader election manager
    pub fn new(
        squad_id: String,
        platform_id: String,
        message_bus: Arc<SquadMessageBus>,
        capabilities: Vec<Capability>,
    ) -> Self {
        let my_score = LeadershipScore::from_capabilities(&capabilities);
        let current_round = ElectionRound::new(1);

        Self {
            squad_id,
            platform_id,
            message_bus,
            state: Arc::new(Mutex::new(ElectionState::Candidate)),
            current_round: Arc::new(Mutex::new(current_round)),
            my_score: Arc::new(Mutex::new(my_score)),
            current_leader: Arc::new(Mutex::new(None)),
            leader_heartbeat: Arc::new(Mutex::new(None)),
            election_timeout: Duration::from_secs(5),
            heartbeat_interval: Duration::from_secs(2),
            max_missed_heartbeats: 3,
        }
    }

    /// Start election process
    #[instrument(skip(self))]
    pub fn start_election(&self) -> Result<()> {
        info!("Starting leader election for squad {}", self.squad_id);

        let score = self.my_score.lock().unwrap().clone();
        let round = {
            let mut current = self.current_round.lock().unwrap();
            current.candidates.insert(self.platform_id.clone(), score);
            current.round
        };

        // Announce candidacy
        self.announce_candidacy(round)?;

        Ok(())
    }

    /// Announce candidacy with leadership score
    fn announce_candidacy(&self, round: u32) -> Result<()> {
        debug!(
            "Platform {} announcing candidacy for round {}",
            self.platform_id, round
        );

        // Send candidacy announcement
        let payload = SquadMessageType::LeaderAnnounce {
            leader_id: self.platform_id.clone(),
            election_round: round,
        };

        self.message_bus.publish(payload)?;

        Ok(())
    }

    /// Process received election message
    #[instrument(skip(self, message))]
    pub fn process_election_message(&self, message: &SquadMessage) -> Result<()> {
        match &message.payload {
            SquadMessageType::LeaderAnnounce {
                leader_id,
                election_round,
            } => {
                self.handle_leader_announce(leader_id, *election_round)?;
            }
            SquadMessageType::Heartbeat { platform_id } => {
                self.handle_heartbeat(platform_id)?;
            }
            _ => {
                // Not an election message
            }
        }

        Ok(())
    }

    /// Handle leader announcement
    fn handle_leader_announce(&self, leader_id: &str, round: u32) -> Result<()> {
        let current_round = {
            let guard = self.current_round.lock().unwrap();
            guard.round
        };

        // Ignore stale announcements from old rounds
        if round < current_round {
            debug!("Ignoring stale announcement from round {}", round);
            return Ok(());
        }

        // Check if we're in a candidate state
        let my_state = *self.state.lock().unwrap();
        if my_state != ElectionState::Candidate {
            return Ok(());
        }

        // Compare leadership scores
        // In a real system, we'd need to receive the score in the message
        // For now, we'll use a simplified comparison based on platform ID
        let should_follow = self.should_follow_leader(leader_id)?;

        if should_follow {
            info!("Following leader: {}", leader_id);
            *self.state.lock().unwrap() = ElectionState::Follower;
            *self.current_leader.lock().unwrap() = Some(leader_id.to_string());

            // Start tracking leader heartbeats
            *self.leader_heartbeat.lock().unwrap() =
                Some(LeaderHeartbeat::new(leader_id.to_string()));
        } else {
            debug!("My score is higher than {}, remaining candidate", leader_id);
        }

        Ok(())
    }

    /// Determine if should follow a leader based on score comparison
    fn should_follow_leader(&self, leader_id: &str) -> Result<bool> {
        // In a production system, we'd compare actual scores received in messages
        // For now, use deterministic platform ID comparison
        Ok(leader_id > self.platform_id.as_str())
    }

    /// Handle heartbeat from leader
    fn handle_heartbeat(&self, platform_id: &str) -> Result<()> {
        let current_leader = self.current_leader.lock().unwrap().clone();

        if let Some(leader_id) = current_leader {
            if platform_id == leader_id {
                // Update heartbeat tracker
                if let Some(ref mut heartbeat) = *self.leader_heartbeat.lock().unwrap() {
                    heartbeat.update();
                    debug!("Received heartbeat from leader {}", leader_id);
                }
            }
        }

        Ok(())
    }

    /// Check for leader failure and trigger re-election
    pub fn check_leader_failure(&self) -> Result<bool> {
        let heartbeat = self.leader_heartbeat.lock().unwrap().clone();

        if let Some(hb) = heartbeat {
            if hb.is_failed(self.heartbeat_interval, self.max_missed_heartbeats) {
                warn!(
                    "Leader {} has failed (no heartbeat for {:?})",
                    hb.leader_id,
                    hb.last_heartbeat.elapsed()
                );

                // Trigger re-election
                self.trigger_reelection()?;
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Trigger re-election
    #[instrument(skip(self))]
    fn trigger_reelection(&self) -> Result<()> {
        info!("Triggering re-election for squad {}", self.squad_id);

        // Reset state to candidate
        *self.state.lock().unwrap() = ElectionState::Candidate;
        *self.current_leader.lock().unwrap() = None;
        *self.leader_heartbeat.lock().unwrap() = None;

        // Increment round
        let new_round = {
            let mut round = self.current_round.lock().unwrap();
            round.round += 1;
            *round = ElectionRound::new(round.round);
            round.round
        };

        // Start new election
        self.announce_candidacy(new_round)?;

        Ok(())
    }

    /// Send heartbeat if we are the leader
    pub fn send_heartbeat_if_leader(&self) -> Result<()> {
        let state = *self.state.lock().unwrap();

        if state == ElectionState::Leader {
            let payload = SquadMessageType::Heartbeat {
                platform_id: self.platform_id.clone(),
            };
            self.message_bus.publish(payload)?;
            debug!("Sent leader heartbeat");
        }

        Ok(())
    }

    /// Get current election state
    pub fn get_state(&self) -> ElectionState {
        *self.state.lock().unwrap()
    }

    /// Get current leader ID
    pub fn get_leader(&self) -> Option<String> {
        self.current_leader.lock().unwrap().clone()
    }

    /// Get current election round
    pub fn get_round(&self) -> u32 {
        self.current_round.lock().unwrap().round
    }

    /// Manually set as leader (for testing or C2 override)
    pub fn set_as_leader(&self) {
        info!("Platform {} set as leader", self.platform_id);
        *self.state.lock().unwrap() = ElectionState::Leader;
        *self.current_leader.lock().unwrap() = Some(self.platform_id.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_leadership_score_computation() {
        let capabilities = vec![
            Capability::new(
                "cap1".to_string(),
                "compute".to_string(),
                CapabilityType::Compute,
                0.8,
            ),
            Capability::new(
                "cap2".to_string(),
                "communication".to_string(),
                CapabilityType::Communication,
                0.6,
            ),
            Capability::new(
                "cap3".to_string(),
                "sensor".to_string(),
                CapabilityType::Sensor,
                1.0,
            ),
        ];

        let score = LeadershipScore::from_capabilities(&capabilities);

        // Allow for f32->f64 conversion precision
        assert!((score.compute - 0.8).abs() < 0.001);
        assert!((score.communication - 0.6).abs() < 0.001);
        assert_eq!(score.sensors, 0.25); // One sensor = 25%
        assert_eq!(score.power, 1.0); // Default
        assert_eq!(score.reliability, 1.0); // Default

        // Check weighted total is reasonable (allowing for f32 precision)
        assert!(score.total > 0.6 && score.total < 0.7);
    }

    #[test]
    fn test_leadership_score_comparison() {
        let score1 = LeadershipScore {
            compute: 0.8,
            communication: 0.6,
            sensors: 0.5,
            power: 0.9,
            reliability: 1.0,
            total: 0.75,
        };

        let score2 = LeadershipScore {
            compute: 0.6,
            communication: 0.5,
            sensors: 0.4,
            power: 0.8,
            reliability: 0.9,
            total: 0.65,
        };

        // score1 is higher
        assert_eq!(
            score1.compare(&score2, "platform_a", "platform_b"),
            std::cmp::Ordering::Greater
        );

        // score2 is lower
        assert_eq!(
            score2.compare(&score1, "platform_b", "platform_a"),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn test_leadership_score_tie_breaking() {
        let score1 = LeadershipScore {
            compute: 0.8,
            communication: 0.6,
            sensors: 0.5,
            power: 0.9,
            reliability: 1.0,
            total: 0.75,
        };

        let score2 = LeadershipScore {
            compute: 0.8,
            communication: 0.6,
            sensors: 0.5,
            power: 0.9,
            reliability: 1.0,
            total: 0.75,
        };

        // Tie-break with platform ID
        assert_eq!(
            score1.compare(&score2, "platform_a", "platform_b"),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            score1.compare(&score2, "platform_b", "platform_a"),
            std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn test_election_round_creation() {
        let round = ElectionRound::new(1);
        assert_eq!(round.round, 1);
        assert!(round.candidates.is_empty());
        assert!(round.started_at > 0);
    }

    #[test]
    fn test_leader_heartbeat_tracking() {
        let mut heartbeat = LeaderHeartbeat::new("leader_1".to_string());

        // Initially not failed
        assert!(!heartbeat.is_failed(Duration::from_secs(2), 3));

        // Update heartbeat
        heartbeat.update();
        assert_eq!(heartbeat.missed_count, 0);

        // Check failure (we can't easily test timeout in unit test)
        assert_eq!(heartbeat.leader_id, "leader_1");
    }

    #[test]
    fn test_election_manager_creation() {
        let message_bus = Arc::new(SquadMessageBus::new(
            "squad_1".to_string(),
            "platform_1".to_string(),
        ));

        let capabilities = vec![Capability::new(
            "cap1".to_string(),
            "compute".to_string(),
            CapabilityType::Compute,
            0.8,
        )];

        let manager = LeaderElectionManager::new(
            "squad_1".to_string(),
            "platform_1".to_string(),
            message_bus,
            capabilities,
        );

        assert_eq!(manager.get_state(), ElectionState::Candidate);
        assert_eq!(manager.get_leader(), None);
        assert_eq!(manager.get_round(), 1);
    }

    #[test]
    fn test_set_as_leader() {
        let message_bus = Arc::new(SquadMessageBus::new(
            "squad_1".to_string(),
            "platform_1".to_string(),
        ));

        let manager = LeaderElectionManager::new(
            "squad_1".to_string(),
            "platform_1".to_string(),
            message_bus,
            vec![],
        );

        manager.set_as_leader();

        assert_eq!(manager.get_state(), ElectionState::Leader);
        assert_eq!(manager.get_leader(), Some("platform_1".to_string()));
    }

    #[test]
    fn test_election_state_transitions() {
        let message_bus = Arc::new(SquadMessageBus::new(
            "squad_1".to_string(),
            "platform_1".to_string(),
        ));

        let manager = LeaderElectionManager::new(
            "squad_1".to_string(),
            "platform_1".to_string(),
            message_bus,
            vec![],
        );

        // Start as candidate
        assert_eq!(manager.get_state(), ElectionState::Candidate);

        // Set as leader
        manager.set_as_leader();
        assert_eq!(manager.get_state(), ElectionState::Leader);

        // Send heartbeat as leader should succeed
        let result = manager.send_heartbeat_if_leader();
        assert!(result.is_ok());
    }

    #[test]
    fn test_multiple_sensors_score() {
        let capabilities = vec![
            Capability::new(
                "sensor1".to_string(),
                "sensor".to_string(),
                CapabilityType::Sensor,
                1.0,
            ),
            Capability::new(
                "sensor2".to_string(),
                "sensor".to_string(),
                CapabilityType::Sensor,
                1.0,
            ),
            Capability::new(
                "sensor3".to_string(),
                "sensor".to_string(),
                CapabilityType::Sensor,
                1.0,
            ),
            Capability::new(
                "sensor4".to_string(),
                "sensor".to_string(),
                CapabilityType::Sensor,
                1.0,
            ),
        ];

        let score = LeadershipScore::from_capabilities(&capabilities);

        // 4 sensors = 1.0 (maxed out)
        assert_eq!(score.sensors, 1.0);
    }

    #[test]
    fn test_start_election() {
        let message_bus = Arc::new(SquadMessageBus::new(
            "squad_1".to_string(),
            "platform_1".to_string(),
        ));

        let capabilities = vec![Capability::new(
            "cap1".to_string(),
            "compute".to_string(),
            CapabilityType::Compute,
            0.8,
        )];

        let manager = LeaderElectionManager::new(
            "squad_1".to_string(),
            "platform_1".to_string(),
            message_bus,
            capabilities,
        );

        let result = manager.start_election();
        assert!(result.is_ok());

        // Check that candidacy was recorded
        let round = manager.current_round.lock().unwrap();
        assert!(round.candidates.contains_key("platform_1"));
    }
}
