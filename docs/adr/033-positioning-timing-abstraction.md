# ADR-033: Positioning and Timing System Abstraction

**Status**: Proposed
**Date**: 2025-12-07
**Authors**: Kit Plummer, Claude
**Relates to**: ADR-024 (Flexible Hierarchy Strategies), ADR-032 (Pluggable Transport Abstraction)

---

## Context

### The GPS Dependency Problem

PEAT Protocol currently assumes GPS availability for:
- **Geographic beacons** (ADR-024) - nodes advertise lat/long/altitude
- **Distance-based hierarchy** - parent selection based on proximity
- **Range mode selection** (ADR-032) - transport config based on peer distance
- **CoT integration** - Cursor-on-Target messages require position
- **Geohash-based sharding** - document distribution by location

**This creates critical vulnerabilities:**

| Scenario | GPS Status | Impact |
|----------|-----------|--------|
| Indoor operations | Degraded/None | No position, hierarchy fails |
| GPS jamming | Denied | Complete position loss |
| GPS spoofing | Compromised | False positions, wrong hierarchy |
| Underground/Underwater | None | No mesh organization |
| Urban canyon | Intermittent | Unstable hierarchy |

### The Timing Problem

Beyond positioning, GPS provides precise timing (PPS - Pulse Per Second). PEAT needs time sync for:
- **CRDT ordering** - Automerge uses timestamps for conflict resolution
- **Event sequencing** - Telemetry and command ordering
- **TTL enforcement** - Document expiration
- **Heartbeat timing** - Connection health monitoring
- **Log correlation** - Debugging across nodes

**Without GPS timing:**
- Clock drift causes ordering issues
- TTL becomes unreliable
- Health monitoring false positives

### Alternative Systems

**Positioning Systems:**

| System | Coverage | Accuracy | Availability |
|--------|----------|----------|--------------|
| GPS | Global outdoor | 3-5m | Jammable |
| GLONASS | Global outdoor | 5-10m | Jammable |
| Galileo | Global outdoor | 1-3m | Jammable |
| BeiDou | Global (Asia focus) | 3-10m | Jammable |
| UWB | Indoor/local | 10-30cm | Requires anchors |
| WiFi RTT | Indoor | 1-2m | Requires APs |
| BLE beacons | Indoor | 2-5m | Requires beacons |
| Dead reckoning | Anywhere | Degrades over time | Always available |
| Visual odometry | Line-of-sight | Sub-meter | Requires camera |
| Terrain matching | Known terrain | 10-100m | Requires maps |

**Timing Systems:**

| System | Accuracy | Availability | Notes |
|--------|----------|--------------|-------|
| GPS PPS | 10-50ns | Outdoor only | Gold standard |
| NTP | 1-50ms | Network required | Internet-dependent |
| PTP/IEEE 1588 | 1-100μs | LAN required | High precision |
| Atomic clock | <1ns/day drift | Always | Expensive, power |
| Crystal oscillator | 10-100ppm drift | Always | Needs calibration |
| Mesh consensus | Varies | Mesh required | Self-organizing |

---

## Decision Drivers

### Requirements

1. **Multi-Source Support**: Use best available position/time source
2. **Source Fusion**: Combine multiple sources for improved accuracy
3. **Graceful Degradation**: Continue operating with reduced accuracy
4. **Spoofing Detection**: Identify compromised sources
5. **Indoor Operation**: Function without satellite signals
6. **Platform Abstraction**: Work across different hardware

### Constraints

1. **Existing Geographic Beacon**: Must maintain compatibility with `GeoPosition`
2. **CRDT Timestamps**: Automerge expects monotonic time
3. **Platform Variance**: Android, iOS, Linux have different APIs
4. **Power Budget**: Continuous GPS/IMU drains battery

---

## Decision

### 1. Position Provider Abstraction

```rust
/// Position data from any source
#[derive(Debug, Clone)]
pub struct Position {
    /// Latitude in degrees (-90 to 90)
    pub latitude: f64,
    /// Longitude in degrees (-180 to 180)
    pub longitude: f64,
    /// Altitude in meters (above WGS84 ellipsoid)
    pub altitude: Option<f64>,
    /// Horizontal accuracy in meters (1-sigma)
    pub horizontal_accuracy: f64,
    /// Vertical accuracy in meters (1-sigma)
    pub vertical_accuracy: Option<f64>,
    /// Heading in degrees (0-360, true north)
    pub heading: Option<f64>,
    /// Speed in meters/second
    pub speed: Option<f64>,
    /// Source of this position
    pub source: PositionSource,
    /// When this position was determined
    pub timestamp: Instant,
    /// Confidence level (0.0-1.0)
    pub confidence: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PositionSource {
    /// GPS (US)
    Gps,
    /// GLONASS (Russia)
    Glonass,
    /// Galileo (EU)
    Galileo,
    /// BeiDou (China)
    Beidou,
    /// Fused GNSS (multiple satellite systems)
    GnssFused,
    /// Ultra-Wideband ranging
    Uwb,
    /// WiFi Round-Trip-Time
    WifiRtt,
    /// Bluetooth beacon triangulation
    BleBeacon,
    /// Inertial measurement unit (dead reckoning)
    Imu,
    /// Visual odometry / SLAM
    Visual,
    /// Terrain-relative navigation
    TerrainMatch,
    /// Manual/configured position
    Manual,
    /// Fused from multiple sources
    Fused,
    /// Unknown/unspecified
    Unknown,
}

/// Position provider trait
#[async_trait]
pub trait PositionProvider: Send + Sync {
    /// Get current position
    async fn get_position(&self) -> Result<Position, PositionError>;

    /// Get position source type
    fn source_type(&self) -> PositionSource;

    /// Check if provider is available
    fn is_available(&self) -> bool;

    /// Get provider priority (higher = preferred)
    fn priority(&self) -> u8 {
        match self.source_type() {
            PositionSource::GnssFused => 100,
            PositionSource::Gps => 90,
            PositionSource::Galileo => 90,
            PositionSource::Uwb => 85,
            PositionSource::WifiRtt => 70,
            PositionSource::BleBeacon => 60,
            PositionSource::Imu => 40,
            PositionSource::Visual => 50,
            PositionSource::Manual => 30,
            _ => 20,
        }
    }

    /// Subscribe to position updates
    fn subscribe(&self) -> Option<mpsc::Receiver<Position>> {
        None  // Default: polling only
    }
}

#[derive(Debug)]
pub enum PositionError {
    /// Provider not available
    Unavailable,
    /// Position fix not acquired
    NoFix,
    /// Position too old
    Stale { age: Duration },
    /// Accuracy below threshold
    LowAccuracy { accuracy_m: f64 },
    /// Provider error
    Provider(String),
}
```

### 2. Position Fusion Engine

```rust
/// Fuses positions from multiple providers
pub struct PositionFusion {
    providers: Vec<Arc<dyn PositionProvider>>,
    config: FusionConfig,
    last_fused: RwLock<Option<Position>>,
    history: RwLock<VecDeque<Position>>,
}

#[derive(Debug, Clone)]
pub struct FusionConfig {
    /// Maximum age before position is considered stale
    pub max_age: Duration,
    /// Minimum accuracy required (meters)
    pub min_accuracy: f64,
    /// Weight GNSS vs local sources
    pub gnss_weight: f64,
    /// Enable dead reckoning interpolation
    pub dead_reckoning: bool,
    /// Spoofing detection enabled
    pub spoof_detection: bool,
    /// Maximum velocity for sanity check (m/s)
    pub max_velocity: f64,
}

impl PositionFusion {
    /// Get best current position
    pub async fn get_position(&self) -> Result<Position, PositionError> {
        let mut candidates = Vec::new();

        // Collect positions from all available providers
        for provider in &self.providers {
            if provider.is_available() {
                if let Ok(pos) = provider.get_position().await {
                    if self.validate_position(&pos) {
                        candidates.push(pos);
                    }
                }
            }
        }

        if candidates.is_empty() {
            // Try dead reckoning from last known position
            if self.config.dead_reckoning {
                return self.dead_reckon();
            }
            return Err(PositionError::Unavailable);
        }

        // Fuse candidates using weighted average
        let fused = self.fuse_positions(&candidates);
        *self.last_fused.write().unwrap() = Some(fused.clone());
        Ok(fused)
    }

    /// Weighted fusion of multiple positions
    fn fuse_positions(&self, positions: &[Position]) -> Position {
        // Weight by inverse of accuracy (more accurate = higher weight)
        let weights: Vec<f64> = positions.iter()
            .map(|p| 1.0 / p.horizontal_accuracy.max(0.1))
            .collect();

        let total_weight: f64 = weights.iter().sum();

        let lat = positions.iter().zip(&weights)
            .map(|(p, w)| p.latitude * w)
            .sum::<f64>() / total_weight;

        let lon = positions.iter().zip(&weights)
            .map(|(p, w)| p.longitude * w)
            .sum::<f64>() / total_weight;

        // Combined accuracy (simplified)
        let accuracy = 1.0 / total_weight.sqrt();

        Position {
            latitude: lat,
            longitude: lon,
            altitude: positions.iter().filter_map(|p| p.altitude).next(),
            horizontal_accuracy: accuracy,
            vertical_accuracy: None,
            heading: positions.iter().filter_map(|p| p.heading).next(),
            speed: positions.iter().filter_map(|p| p.speed).next(),
            source: PositionSource::Fused,
            timestamp: Instant::now(),
            confidence: (1.0 - accuracy / 100.0).max(0.0).min(1.0),
        }
    }

    /// Validate position against sanity checks
    fn validate_position(&self, pos: &Position) -> bool {
        // Check accuracy
        if pos.horizontal_accuracy > self.config.min_accuracy * 10.0 {
            return false;
        }

        // Check age
        if pos.timestamp.elapsed() > self.config.max_age {
            return false;
        }

        // Spoofing detection: check for impossible jumps
        if self.config.spoof_detection {
            if let Some(last) = self.last_fused.read().unwrap().as_ref() {
                let distance = haversine_distance(
                    last.latitude, last.longitude,
                    pos.latitude, pos.longitude,
                );
                let elapsed = pos.timestamp.duration_since(last.timestamp).as_secs_f64();
                let velocity = distance / elapsed.max(0.001);

                if velocity > self.config.max_velocity {
                    tracing::warn!(
                        "Possible position spoofing: velocity {} m/s exceeds max {}",
                        velocity, self.config.max_velocity
                    );
                    return false;
                }
            }
        }

        true
    }

    /// Dead reckoning from last known position using IMU
    fn dead_reckon(&self) -> Result<Position, PositionError> {
        let last = self.last_fused.read().unwrap()
            .clone()
            .ok_or(PositionError::Unavailable)?;

        let age = last.timestamp.elapsed();
        if age > Duration::from_secs(300) {  // 5 minute limit
            return Err(PositionError::Stale { age });
        }

        // Project position based on heading and speed
        if let (Some(heading), Some(speed)) = (last.heading, last.speed) {
            let distance = speed * age.as_secs_f64();
            let (new_lat, new_lon) = project_position(
                last.latitude, last.longitude,
                heading, distance,
            );

            return Ok(Position {
                latitude: new_lat,
                longitude: new_lon,
                horizontal_accuracy: last.horizontal_accuracy + distance * 0.1, // Growing uncertainty
                source: PositionSource::Imu,
                timestamp: Instant::now(),
                confidence: (last.confidence - age.as_secs_f64() / 300.0).max(0.1),
                ..last
            });
        }

        // Can't dead reckon without velocity
        Err(PositionError::Stale { age })
    }
}
```

### 3. Time Provider Abstraction

```rust
/// Time data with source and accuracy information
#[derive(Debug, Clone)]
pub struct SyncedTime {
    /// Current time (UTC)
    pub time: SystemTime,
    /// Estimated accuracy
    pub accuracy: TimeAccuracy,
    /// Source of time
    pub source: TimeSource,
    /// Offset from local system clock
    pub offset_ns: i64,
    /// Last sync time
    pub last_sync: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeAccuracy {
    /// Nanosecond-level (atomic clock, GPS PPS)
    Nanoseconds(u32),
    /// Microsecond-level (PTP)
    Microseconds(u32),
    /// Millisecond-level (NTP, mesh sync)
    Milliseconds(u32),
    /// Second-level (degraded)
    Seconds(u32),
    /// Unknown accuracy
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeSource {
    /// GPS Pulse-Per-Second
    GpsPps,
    /// Network Time Protocol
    Ntp,
    /// Precision Time Protocol (IEEE 1588)
    Ptp,
    /// Atomic clock (onboard)
    AtomicClock,
    /// Mesh network consensus
    MeshConsensus,
    /// Local system clock (unsynced)
    LocalClock,
    /// Manual/configured offset
    Manual,
}

/// Time provider trait
#[async_trait]
pub trait TimeProvider: Send + Sync {
    /// Get synchronized time
    fn get_time(&self) -> SyncedTime;

    /// Get time source type
    fn source_type(&self) -> TimeSource;

    /// Check if time is considered synchronized
    fn is_synchronized(&self) -> bool;

    /// Force resync
    async fn resync(&self) -> Result<(), TimeError>;

    /// Get provider priority
    fn priority(&self) -> u8 {
        match self.source_type() {
            TimeSource::AtomicClock => 100,
            TimeSource::GpsPps => 95,
            TimeSource::Ptp => 90,
            TimeSource::Ntp => 70,
            TimeSource::MeshConsensus => 50,
            TimeSource::LocalClock => 10,
            TimeSource::Manual => 5,
        }
    }
}

#[derive(Debug)]
pub enum TimeError {
    /// No time source available
    NoSource,
    /// Sync failed
    SyncFailed(String),
    /// Clock jumped unexpectedly
    ClockJump { delta: Duration },
}
```

### 4. Mesh Time Consensus

When no external time source is available, nodes can establish consensus:

```rust
/// Mesh-based time synchronization
pub struct MeshTimeSync {
    /// Local offset from nominal time
    offset: RwLock<i64>,
    /// Offsets reported by peers
    peer_offsets: RwLock<HashMap<NodeId, PeerTimeInfo>>,
    /// Configuration
    config: MeshTimeSyncConfig,
}

#[derive(Debug, Clone)]
pub struct PeerTimeInfo {
    /// Peer's reported offset from our clock
    offset_ns: i64,
    /// Round-trip time to peer
    rtt_ns: u64,
    /// When we last synced with this peer
    last_sync: Instant,
    /// Peer's claimed accuracy
    accuracy: TimeAccuracy,
}

#[derive(Debug, Clone)]
pub struct MeshTimeSyncConfig {
    /// Minimum peers for consensus
    pub min_peers: usize,
    /// Sync interval
    pub sync_interval: Duration,
    /// Maximum acceptable RTT for sync
    pub max_rtt: Duration,
    /// Outlier rejection threshold (sigma)
    pub outlier_sigma: f64,
}

impl MeshTimeSync {
    /// Calculate time offset via ping-pong with peer
    pub async fn sync_with_peer(
        &self,
        peer_id: &NodeId,
        transport: &dyn Transport,
    ) -> Result<PeerTimeInfo, TimeError> {
        // Send time request with our timestamp
        let t1 = Instant::now();
        let request = TimeRequest { sender_time: t1 };

        // Receive response with peer's timestamp
        let response = transport.request_time(peer_id, request).await?;
        let t4 = Instant::now();

        // Classic NTP algorithm:
        // t1 = request sent (local)
        // t2 = request received (peer) - from response
        // t3 = response sent (peer) - from response
        // t4 = response received (local)
        //
        // RTT = (t4 - t1) - (t3 - t2)
        // Offset = ((t2 - t1) + (t3 - t4)) / 2

        let rtt = (t4 - t1) - response.processing_time;
        let offset = ((response.receive_time - t1) + (response.send_time - t4)) / 2;

        Ok(PeerTimeInfo {
            offset_ns: offset.as_nanos() as i64,
            rtt_ns: rtt.as_nanos() as u64,
            last_sync: Instant::now(),
            accuracy: TimeAccuracy::Milliseconds(
                (rtt.as_millis() / 2) as u32
            ),
        })
    }

    /// Calculate consensus offset from all peers
    pub fn calculate_consensus(&self) -> Option<i64> {
        let peers = self.peer_offsets.read().unwrap();

        if peers.len() < self.config.min_peers {
            return None;
        }

        // Filter stale entries
        let fresh: Vec<_> = peers.values()
            .filter(|p| p.last_sync.elapsed() < self.config.sync_interval * 3)
            .collect();

        if fresh.len() < self.config.min_peers {
            return None;
        }

        // Calculate weighted median (weight by inverse RTT)
        let mut weighted: Vec<(i64, f64)> = fresh.iter()
            .map(|p| (p.offset_ns, 1.0 / (p.rtt_ns as f64).max(1.0)))
            .collect();

        weighted.sort_by_key(|(offset, _)| *offset);

        // Find weighted median
        let total_weight: f64 = weighted.iter().map(|(_, w)| w).sum();
        let mut cumulative = 0.0;
        for (offset, weight) in &weighted {
            cumulative += weight;
            if cumulative >= total_weight / 2.0 {
                return Some(*offset);
            }
        }

        None
    }
}
```

### 5. Integration with PEAT Components

```rust
/// Central positioning and timing service
pub struct LocationTimeService {
    /// Position fusion engine
    position: Arc<PositionFusion>,
    /// Time synchronization
    time: Arc<dyn TimeProvider>,
    /// Mesh time sync (fallback)
    mesh_time: Arc<MeshTimeSync>,
    /// Event channel for updates
    events: broadcast::Sender<LocationTimeEvent>,
}

pub enum LocationTimeEvent {
    /// Position updated
    PositionUpdate(Position),
    /// Position source changed
    PositionSourceChange { old: PositionSource, new: PositionSource },
    /// Time sync established
    TimeSynced { source: TimeSource, accuracy: TimeAccuracy },
    /// Time source lost
    TimeLost { old_source: TimeSource },
    /// Possible spoofing detected
    SpoofingAlert { source: PositionSource, reason: String },
}

impl LocationTimeService {
    /// Get current position for geographic beacon
    pub async fn get_position(&self) -> Result<GeoPosition, PositionError> {
        let pos = self.position.get_position().await?;
        Ok(GeoPosition::new(pos.latitude, pos.longitude))
    }

    /// Get synchronized time for CRDT operations
    pub fn get_time(&self) -> SystemTime {
        let synced = self.time.get_time();
        synced.time
    }

    /// Get distance to peer (for transport range mode)
    pub async fn distance_to(&self, peer_pos: &Position) -> Result<f64, PositionError> {
        let our_pos = self.position.get_position().await?;
        Ok(haversine_distance(
            our_pos.latitude, our_pos.longitude,
            peer_pos.latitude, peer_pos.longitude,
        ))
    }

    /// Check if we have a position fix
    pub fn has_position(&self) -> bool {
        self.position.get_position().now_or_never()
            .map(|r| r.is_ok())
            .unwrap_or(false)
    }

    /// Check if time is synchronized
    pub fn is_time_synced(&self) -> bool {
        self.time.is_synchronized()
    }
}
```

---

## GPS-Denied Operation Modes

### Mode 1: Full Degradation (No Position)

When no position is available:
- Geographic hierarchy disabled
- Fall back to static topology or broadcast
- CoT messages use "unknown" position
- Range mode defaults to maximum

```rust
impl TopologyManager {
    fn select_parent_gps_denied(&self) -> Option<NodeId> {
        // Option 1: Use configured static hierarchy
        if let Some(parent) = self.config.static_parent {
            return Some(parent);
        }

        // Option 2: Use lowest latency peer
        self.peers.iter()
            .min_by_key(|(_, info)| info.rtt_ms)
            .map(|(id, _)| id.clone())
    }
}
```

### Mode 2: Relative Positioning (UWB/BLE)

Indoor scenarios with local positioning:
- Use UWB or BLE for relative distances
- Hierarchy based on hop count + signal strength
- Position in local coordinate frame

```rust
impl LocalPositionHierarchy {
    fn select_parent(&self, position: &Position) -> Option<NodeId> {
        // Find peers with strong signal (close proximity)
        let close_peers: Vec<_> = self.peers.iter()
            .filter(|(_, info)| info.signal_strength > -60) // dBm
            .collect();

        // Select highest-level peer nearby
        close_peers.iter()
            .max_by_key(|(_, info)| info.hierarchy_level)
            .map(|(id, _)| (*id).clone())
    }
}
```

### Mode 3: Hybrid (GNSS + Local)

Combine global and local positioning:
- Use GNSS when available
- Supplement with UWB for indoor/accuracy
- Seamless transitions

---

## Implementation Plan

### Phase 1: Core Abstractions

- [ ] Define `Position`, `PositionSource`, `PositionProvider` trait
- [ ] Define `SyncedTime`, `TimeSource`, `TimeProvider` trait
- [ ] Create `PositionError` and `TimeError` types
- [ ] Unit tests for types

**Estimated scope**: ~400 lines

### Phase 2: Position Fusion

- [ ] Implement `PositionFusion` engine
- [ ] Weighted position averaging
- [ ] Spoofing detection
- [ ] Dead reckoning support
- [ ] Integration tests

**Estimated scope**: ~600 lines

### Phase 3: Platform Providers

- [ ] GPS provider (platform-specific)
- [ ] NTP time provider
- [ ] Local clock provider
- [ ] Platform abstraction layer

**Estimated scope**: ~800 lines per platform

### Phase 4: Mesh Time Sync

- [ ] Implement `MeshTimeSync`
- [ ] NTP-style offset calculation
- [ ] Consensus algorithm
- [ ] Integration with transport layer

**Estimated scope**: ~500 lines

### Phase 5: Integration

- [ ] Create `LocationTimeService`
- [ ] Integrate with geographic beacons
- [ ] Integrate with transport range mode
- [ ] Integrate with CoT translation
- [ ] Update CRDT timestamp handling

**Estimated scope**: ~600 lines

---

## Open Questions

1. **Coordinate Systems**: Should we support local coordinate frames (ENU) in addition to WGS84?
   - Use case: Indoor positioning returns x/y/z in building coordinates
   - Option A: Always convert to WGS84
   - Option B: Support both with transformation API

2. **Clock Monotonicity**: How to handle backward time jumps?
   - Automerge expects monotonic timestamps
   - Option A: Use logical clocks (Lamport/vector) for ordering
   - Option B: Delay operations until wall clock catches up
   - Option C: Hybrid logical clocks (HLC)

3. **Altitude Handling**: Many sources don't provide altitude. How to handle?
   - Option A: Require altitude for 3D distance calculations
   - Option B: Use 2D distance when altitude unavailable
   - Option C: Terrain database lookup

4. **Privacy Implications**: Position data is sensitive. How to protect?
   - Option A: Local-only, never share raw position
   - Option B: Fuzzing/rounding for broadcast
   - Option C: Encryption of position in transit

5. **IMU Integration**: How tightly should we integrate with IMU for dead reckoning?
   - Platform IMU APIs vary significantly
   - Drift compensation is complex
   - May need sensor fusion library

---

## Security Considerations

### GPS Spoofing Mitigation

1. **Velocity checks**: Reject impossible position jumps
2. **Multi-constellation**: Cross-check GPS vs Galileo vs GLONASS
3. **Signal strength monitoring**: Detect anomalously strong signals
4. **Inertial cross-check**: Compare GPS delta with IMU
5. **Peer consensus**: Compare position with nearby peers

### Time Spoofing Mitigation

1. **Multi-source validation**: Compare GPS time with NTP
2. **Rate limiting**: Reject large time jumps
3. **Mesh consensus**: Use peer agreement as sanity check
4. **Monotonicity enforcement**: Never allow backward jumps

---

## References

1. [WGS84 Coordinate System](https://en.wikipedia.org/wiki/World_Geodetic_System)
2. [NTP RFC 5905](https://datatracker.ietf.org/doc/html/rfc5905)
3. [IEEE 1588 PTP](https://standards.ieee.org/ieee/1588/6825/)
4. [Hybrid Logical Clocks Paper](https://cse.buffalo.edu/tech-reports/2014-04.pdf)
5. [GPS Spoofing Detection Survey](https://ieeexplore.ieee.org/document/8447371)
6. ADR-024: Flexible Hierarchy Strategies
7. ADR-032: Pluggable Transport Abstraction

---

**Last Updated**: 2025-12-07
**Status**: PROPOSED - Awaiting discussion
