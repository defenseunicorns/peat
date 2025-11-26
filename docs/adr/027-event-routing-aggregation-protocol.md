# ADR-027: Event Routing and Aggregation Protocol

**Status**: Proposed  
**Date**: 2025-11-25  
**Authors**: Codex, Kit Plummer  
**Relates to**: ADR-012 (Schema Definition), ADR-009 (Bidirectional Flows), ADR-019 (QoS and Data Prioritization)

## Context

### The Event Flow Problem

HIVE Protocol enables distributed autonomous systems to coordinate through hierarchical state synchronization. ADR-012 defines the **schemas** for events, capabilities, and commands. This ADR defines the **protocol behavior** - how events flow through the hierarchy and how aggregation policies are enforced.

**Core Challenge:**

In a 1000-node company formation with 4 echelons (platform → squad → platoon → company):
- Each platform may produce 10-100 events/second (detections, telemetry, anomalies)
- Naive propagation: 1000 × 100 = 100,000 events/second reaching company C2
- Tactical bandwidth: Often 9.6Kbps - 1Mbps shared across all traffic

**Without intelligent routing:**
- Bandwidth exhaustion within seconds
- Critical events (adversarial detection) lost in noise
- Operators overwhelmed with undifferentiated data
- Higher echelons have no situational awareness

**HIVE's Solution:**

Events carry `AggregationPolicy` metadata that tells HIVE *how* to route them:
- **Critical anomalies**: Immediate propagation, preempt other traffic
- **Routine detections**: Aggregate into summaries at squad level
- **Telemetry**: Store locally, respond to queries
- **Debug data**: Never propagate

This ADR specifies the protocol behavior that enforces these policies.

### Relationship to Other ADRs

| ADR | Defines | This ADR's Relationship |
|-----|---------|------------------------|
| ADR-012 | Event schemas (HiveEvent, AggregationPolicy) | **Uses** these schemas |
| ADR-009 | Bidirectional flow concepts | **Implements** upward event flow |
| ADR-019 | QoS framework | **Integrates** priority enforcement |
| ADR-001 | Hierarchical architecture | **Operates within** this structure |

### Design Principles

1. **Policy-Driven**: Event producers declare routing intent; HIVE enforces
2. **Hierarchical**: Events flow through formation structure, not arbitrary mesh
3. **Bandwidth-Aware**: Aggregation reduces traffic at each echelon
4. **Priority-Respecting**: Critical events preempt routine traffic
5. **Queryable**: Non-propagated events remain accessible on-demand

## Decision

### Event Routing Model

```
┌─────────────────────────────────────────────────────────────────┐
│                        COMPANY C2                                │
│    Receives: Summaries + Critical events from platoons          │
└─────────────────────────────┬───────────────────────────────────┘
                              │ Aggregated summaries
                              │ Critical events (immediate)
        ┌─────────────────────┼─────────────────────┐
        ▼                     ▼                     ▼
┌───────────────┐     ┌───────────────┐     ┌───────────────┐
│  PLATOON 1    │     │  PLATOON 2    │     │  PLATOON 3    │
│  Aggregates   │     │  Aggregates   │     │  Aggregates   │
│  squad events │     │  squad events │     │  squad events │
└───────┬───────┘     └───────┬───────┘     └───────┬───────┘
        │                     │                     │
   ┌────┴────┐           ┌────┴────┐           ┌────┴────┐
   ▼         ▼           ▼         ▼           ▼         ▼
┌─────┐   ┌─────┐     ┌─────┐   ┌─────┐     ┌─────┐   ┌─────┐
│SQD 1│   │SQD 2│     │SQD 3│   │SQD 4│     │SQD 5│   │SQD 6│
│     │   │     │     │     │   │     │     │     │   │     │
└──┬──┘   └──┬──┘     └──┬──┘   └──┬──┘     └──┬──┘   └──┬──┘
   │         │           │         │           │         │
┌──┴──┐   ┌──┴──┐     ┌──┴──┐   ┌──┴──┐     ┌──┴──┐   ┌──┴──┐
│8 plt│   │8 plt│     │8 plt│   │8 plt│     │8 plt│   │8 plt│
│forms│   │forms│     │forms│   │forms│     │forms│   │forms│
└─────┘   └─────┘     └─────┘   └─────┘     └─────┘   └─────┘

Events flow UPWARD through this structure.
Each echelon applies aggregation policies.
```

### Protocol Components

#### 1. Event Emission

When software on a platform produces an event:

```rust
/// Event emission at the source platform
pub struct EventEmitter {
    node_id: String,
    formation_id: String,
    outbound_queue: PriorityQueue<HiveEvent>,
}

impl EventEmitter {
    /// Emit an event with routing policy
    pub fn emit(&mut self, event: HiveEvent) {
        // Validate event has required fields
        assert!(!event.event_id.is_empty());
        assert!(event.routing.is_some());
        
        let routing = event.routing.as_ref().unwrap();
        
        // Check if event should propagate at all
        if routing.propagation == PropagationMode::Local {
            // Store locally only, do not queue for transmission
            self.store_local(&event);
            return;
        }
        
        // Assign to priority queue based on policy
        let priority = routing.priority;
        self.outbound_queue.push(event, priority);
    }
    
    /// Store event locally (for QUERY mode or LOCAL mode)
    fn store_local(&self, event: &HiveEvent) {
        // Store in local event log with TTL
        // Queryable by higher echelons
    }
}
```

#### 2. Priority Queue Transmission

Events are transmitted based on priority:

```rust
/// Priority-based event transmission
pub struct EventTransmitter {
    /// Events queued by priority
    queues: [VecDeque<HiveEvent>; 4],  // CRITICAL, HIGH, NORMAL, LOW
    
    /// Bandwidth allocation per priority (configurable)
    bandwidth_allocation: BandwidthAllocation,
    
    /// Connection to parent echelon
    parent_connection: Box<dyn Transport>,
}

#[derive(Clone)]
pub struct BandwidthAllocation {
    /// Minimum guaranteed bandwidth per priority (bytes/sec)
    /// CRITICAL always gets through; others share remainder
    pub critical_reserved: u64,
    pub high_min: u64,
    pub normal_min: u64,
    pub low_min: u64,
    
    /// Total available bandwidth
    pub total_available: u64,
}

impl EventTransmitter {
    /// Transmit events respecting priority
    pub async fn transmit_cycle(&mut self) {
        // CRITICAL: Always transmit immediately, preempt others
        while let Some(event) = self.queues[0].pop_front() {
            self.parent_connection.send(&event).await;
        }
        
        // HIGH/NORMAL/LOW: Fair-share with minimums
        let remaining_bandwidth = self.bandwidth_allocation.total_available
            - self.bandwidth_allocation.critical_reserved;
        
        // Weighted fair queuing across remaining priorities
        self.weighted_fair_transmit(remaining_bandwidth).await;
    }
    
    async fn weighted_fair_transmit(&mut self, bandwidth: u64) {
        // Implementation of weighted fair queuing
        // HIGH gets 50%, NORMAL gets 35%, LOW gets 15% of remaining
        // Unused allocation rolls to lower priorities
    }
}
```

#### 3. Aggregation at Echelon Boundaries

Each echelon (squad leader, platoon leader, company C2) runs an aggregator:

```rust
/// Event aggregator at echelon boundary
pub struct EchelonAggregator {
    echelon_id: String,
    echelon_type: EchelonType,  // Squad, Platoon, Company
    
    /// Aggregation windows by event type
    windows: HashMap<(EventClass, String), AggregationWindow>,
    
    /// Events to forward without aggregation
    passthrough_queue: PriorityQueue<HiveEvent>,
    
    /// Outbound to parent echelon
    parent_emitter: EventEmitter,
}

pub struct AggregationWindow {
    event_class: EventClass,
    event_type: String,
    window_duration: Duration,
    window_start: Instant,
    
    /// Events collected in this window
    events: Vec<HiveEvent>,
    
    /// Source nodes that contributed
    source_nodes: HashSet<String>,
}

impl EchelonAggregator {
    /// Process incoming event from subordinate
    pub fn receive(&mut self, event: HiveEvent) {
        let routing = event.routing.as_ref().unwrap();
        
        match routing.propagation {
            PropagationMode::Full => {
                // Forward immediately without aggregation
                self.passthrough_queue.push(event, routing.priority);
            }
            
            PropagationMode::Summary => {
                // Add to aggregation window
                let key = (event.event_class, event.event_type.clone());
                let window = self.windows
                    .entry(key)
                    .or_insert_with(|| AggregationWindow::new(
                        event.event_class,
                        &event.event_type,
                        Duration::from_millis(routing.aggregation_window_ms as u64),
                    ));
                
                window.add(event);
                
                // Check if window should flush
                if window.should_flush() {
                    let summary = window.generate_summary();
                    self.parent_emitter.emit(summary);
                    window.reset();
                }
            }
            
            PropagationMode::Query => {
                // Store locally, do not forward
                // Will be available for query from parent
                self.store_queryable(&event);
            }
            
            PropagationMode::Local => {
                // Should not reach aggregator, but handle gracefully
                // Do nothing
            }
        }
    }
    
    /// Generate summary from aggregation window
    fn generate_summary(window: &AggregationWindow) -> HiveEvent {
        HiveEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            timestamp: Some(chrono::Utc::now().into()),
            source_node_id: self.echelon_id.clone(),
            source_formation_id: self.echelon_id.clone(),
            event_class: window.event_class,
            event_type: format!("{}_summary", window.event_type),
            routing: Some(AggregationPolicy {
                propagation: PropagationMode::Full,  // Summaries propagate fully
                priority: EventPriority::Normal,
                ttl_seconds: 300,
                aggregation_window_ms: 0,
            }),
            payload: Some(EventSummary {
                formation_id: self.echelon_id.clone(),
                window_start: window.window_start.into(),
                window_end: Instant::now().into(),
                event_class: window.event_class,
                event_type: window.event_type.clone(),
                event_count: window.events.len() as u32,
                source_node_ids: window.source_nodes.iter().cloned().collect(),
                summary_payload: window.compute_summary_payload(),
            }.into()),
        }
    }
}
```

#### 4. Summary Generation Strategies

Different event types require different summarization:

```rust
/// Strategy for summarizing events of a given type
pub trait SummaryStrategy: Send + Sync {
    /// Event type this strategy handles
    fn event_type(&self) -> &str;
    
    /// Generate summary payload from collected events
    fn summarize(&self, events: &[HiveEvent]) -> prost_types::Any;
}

/// Detection event summary: count by type, confidence histogram
pub struct DetectionSummaryStrategy;

impl SummaryStrategy for DetectionSummaryStrategy {
    fn event_type(&self) -> &str {
        "detection"
    }
    
    fn summarize(&self, events: &[HiveEvent]) -> prost_types::Any {
        let mut counts_by_type: HashMap<String, u32> = HashMap::new();
        let mut confidence_histogram = [0u32; 10];
        
        for event in events {
            // Parse detection from payload
            if let Some(detection) = parse_detection(&event.payload) {
                *counts_by_type.entry(detection.object_type.clone()).or_default() += 1;
                
                let bucket = (detection.confidence * 10.0).min(9.0) as usize;
                confidence_histogram[bucket] += 1;
            }
        }
        
        // Encode as Any
        DetectionSummary {
            counts_by_type,
            confidence_histogram: confidence_histogram.to_vec(),
            total_detections: events.len() as u32,
        }.into()
    }
}

/// Telemetry summary: min/max/avg for numeric metrics
pub struct TelemetrySummaryStrategy;

impl SummaryStrategy for TelemetrySummaryStrategy {
    fn event_type(&self) -> &str {
        "telemetry"
    }
    
    fn summarize(&self, events: &[HiveEvent]) -> prost_types::Any {
        let mut metrics: HashMap<String, MetricStats> = HashMap::new();
        
        for event in events {
            if let Some(telemetry) = parse_telemetry(&event.payload) {
                for (name, value) in telemetry.metrics {
                    let stats = metrics.entry(name).or_default();
                    stats.update(value);
                }
            }
        }
        
        TelemetrySummary {
            metrics: metrics.into_iter().map(|(k, v)| (k, v.finalize())).collect(),
            sample_count: events.len() as u32,
        }.into()
    }
}

#[derive(Default)]
struct MetricStats {
    min: f64,
    max: f64,
    sum: f64,
    count: u32,
}

impl MetricStats {
    fn update(&mut self, value: f64) {
        if self.count == 0 {
            self.min = value;
            self.max = value;
        } else {
            self.min = self.min.min(value);
            self.max = self.max.max(value);
        }
        self.sum += value;
        self.count += 1;
    }
    
    fn finalize(&self) -> MetricSummaryStats {
        MetricSummaryStats {
            min: self.min,
            max: self.max,
            avg: if self.count > 0 { self.sum / self.count as f64 } else { 0.0 },
            count: self.count,
        }
    }
}
```

#### 5. Query Protocol for Non-Propagated Events

Events with `PropagationMode::Query` are stored locally and accessible via query:

```rust
/// Query for events stored at subordinate echelons
pub struct EventQuery {
    /// Query ID for correlation
    pub query_id: String,
    
    /// Requesting node
    pub requester_id: String,
    
    /// Target scope
    pub scope: QueryScope,
    
    /// Event filters
    pub filters: EventFilters,
    
    /// Maximum events to return
    pub limit: u32,
}

pub enum QueryScope {
    /// Query specific node
    Node { node_id: String },
    /// Query entire formation
    Formation { formation_id: String },
    /// Query all subordinates of requesting echelon
    Subordinates,
}

pub struct EventFilters {
    /// Filter by event class
    pub event_class: Option<EventClass>,
    /// Filter by event type
    pub event_type: Option<String>,
    /// Filter by time range
    pub after: Option<chrono::DateTime<chrono::Utc>>,
    pub before: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter by source instance
    pub source_instance_id: Option<String>,
}

/// Query response
pub struct EventQueryResponse {
    pub query_id: String,
    pub responder_id: String,
    pub events: Vec<HiveEvent>,
    pub total_matching: u32,  // May be > events.len() if limited
    pub truncated: bool,
}

/// Query handler at each node
pub struct EventQueryHandler {
    /// Local event store
    event_store: Arc<dyn EventStore>,
    
    /// Connection to subordinates (for forwarding queries)
    subordinate_connections: HashMap<String, Box<dyn Transport>>,
}

impl EventQueryHandler {
    /// Handle incoming query
    pub async fn handle_query(&self, query: EventQuery) -> EventQueryResponse {
        match &query.scope {
            QueryScope::Node { node_id } if node_id == &self.node_id => {
                // Query local store
                self.query_local(&query)
            }
            
            QueryScope::Node { node_id } => {
                // Forward to specific subordinate
                if let Some(conn) = self.subordinate_connections.get(node_id) {
                    conn.send_query(&query).await
                } else {
                    EventQueryResponse::not_found(&query)
                }
            }
            
            QueryScope::Formation { formation_id } => {
                // Fan out to all subordinates in formation
                self.fan_out_query(&query, formation_id).await
            }
            
            QueryScope::Subordinates => {
                // Query all direct subordinates
                self.query_all_subordinates(&query).await
            }
        }
    }
    
    fn query_local(&self, query: &EventQuery) -> EventQueryResponse {
        let events = self.event_store.query(
            query.filters.event_class,
            query.filters.event_type.as_deref(),
            query.filters.after,
            query.filters.before,
            query.limit,
        );
        
        EventQueryResponse {
            query_id: query.query_id.clone(),
            responder_id: self.node_id.clone(),
            events,
            total_matching: events.len() as u32,
            truncated: false,
        }
    }
}
```

#### 6. TTL Enforcement

Events expire based on their TTL:

```rust
/// TTL enforcement for stored events
pub struct EventTTLEnforcer {
    event_store: Arc<dyn EventStore>,
    check_interval: Duration,
}

impl EventTTLEnforcer {
    /// Run TTL enforcement loop
    pub async fn run(&self) {
        let mut interval = tokio::time::interval(self.check_interval);
        
        loop {
            interval.tick().await;
            self.enforce_ttl().await;
        }
    }
    
    async fn enforce_ttl(&self) {
        let now = chrono::Utc::now();
        
        // Get all events and check TTL
        let expired: Vec<String> = self.event_store
            .list_all()
            .filter(|event| {
                if let Some(routing) = &event.routing {
                    if routing.ttl_seconds > 0 {
                        let event_time = event.timestamp.as_ref()
                            .map(|t| chrono::DateTime::from(*t))
                            .unwrap_or(now);
                        let expiry = event_time + chrono::Duration::seconds(routing.ttl_seconds as i64);
                        return now > expiry;
                    }
                }
                false
            })
            .map(|e| e.event_id.clone())
            .collect();
        
        // Delete expired events
        for event_id in expired {
            self.event_store.delete(&event_id).await;
        }
    }
}
```

### Wire Protocol

Events are transmitted using the standard HIVE transport (ADR-010):

```protobuf
syntax = "proto3";
package hive.event.wire.v1;

import "hive/event/v1/event.proto";

// Event transmission message
message EventTransmission {
  oneof payload {
    // Single event
    hive.event.v1.HiveEvent event = 1;
    
    // Batch of events (efficiency optimization)
    EventBatch batch = 2;
    
    // Event query
    EventQuery query = 3;
    
    // Query response
    EventQueryResponse query_response = 4;
  }
}

message EventBatch {
  repeated hive.event.v1.HiveEvent events = 1;
}

message EventQuery {
  string query_id = 1;
  string requester_id = 2;
  QueryScope scope = 3;
  EventFilters filters = 4;
  uint32 limit = 5;
}

message QueryScope {
  oneof target {
    string node_id = 1;
    string formation_id = 2;
    bool subordinates = 3;
  }
}

message EventFilters {
  optional hive.event.v1.EventClass event_class = 1;
  optional string event_type = 2;
  optional google.protobuf.Timestamp after = 3;
  optional google.protobuf.Timestamp before = 4;
  optional string source_instance_id = 5;
}

message EventQueryResponse {
  string query_id = 1;
  string responder_id = 2;
  repeated hive.event.v1.HiveEvent events = 3;
  uint32 total_matching = 4;
  bool truncated = 5;
}
```

### Bandwidth Calculations

**Example: 48-node platoon with 6 squads**

| Event Type | Rate/Platform | Policy | Squad Output | Platoon Output |
|------------|---------------|--------|--------------|----------------|
| Detections | 10/sec | Summary (1s window) | 1 summary/sec × 6 = 6/sec | 1 summary/sec |
| Telemetry | 1/sec | Query | 0 (stored locally) | 0 |
| Anomalies | 0.01/sec | Full | 0.08/sec × 6 = 0.5/sec | 0.5/sec |
| Critical | 0.001/sec | Full + Critical | 0.008/sec × 6 = 0.05/sec | 0.05/sec |

**Without aggregation:** 48 × 11 = 528 events/sec to platoon  
**With aggregation:** ~7.5 events/sec to platoon  
**Reduction:** 98.6%

### Integration with Capability Advertisement

Capability advertisements (ADR-012) follow the same routing:

```rust
impl From<CapabilityAdvertisement> for HiveEvent {
    fn from(cap: CapabilityAdvertisement) -> Self {
        HiveEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            timestamp: cap.advertised_at,
            source_node_id: cap.node_id.clone(),
            source_formation_id: cap.formation_id.clone(),
            event_class: EventClass::Telemetry,
            event_type: "capability_advertisement".to_string(),
            routing: Some(AggregationPolicy {
                propagation: PropagationMode::Summary,
                priority: EventPriority::Normal,
                ttl_seconds: 60,
                aggregation_window_ms: 5000,  // 5s aggregation windows
            }),
            payload: Some(cap.into()),
        }
    }
}
```

Capability summaries are generated at each echelon using `FormationCapabilitySummary` (ADR-012).

## Consequences

### Positive

**1. Bandwidth Efficiency:**
- 95-99% reduction in event traffic through aggregation
- Critical events still propagate immediately
- Sustainable on tactical networks

**2. Operator Focus:**
- Higher echelons see summaries, not noise
- Can drill down via queries when needed
- Priority ensures important events visible

**3. Flexibility:**
- Event producers control routing via policy
- New event types work without protocol changes
- Configurable aggregation windows

**4. Scalability:**
- O(log n) event propagation vs O(n) naive
- Each echelon handles constant load regardless of subordinate count
- Query fan-out bounded by hierarchy depth

### Negative

**1. Latency for Summarized Events:**
- Summary events delayed by aggregation window
- Trade-off: bandwidth vs. timeliness
- Mitigation: Use FULL propagation for time-critical events

**2. Query Complexity:**
- Distributed queries require fan-out
- Potential for inconsistent results during churn
- Mitigation: Query timeouts, best-effort semantics

**3. Summary Information Loss:**
- Summaries discard individual event details
- Cannot reconstruct original events from summary
- Mitigation: Keep originals locally, query when needed

### Risks and Mitigations

**Risk: Aggregation window too long, miss time-critical patterns**
- Mitigation: Configurable per-event-type windows
- Mitigation: Separate CRITICAL priority bypasses aggregation

**Risk: Query storms from curious operators**
- Mitigation: Rate limiting on queries
- Mitigation: Query result caching at echelons

**Risk: TTL expires before query retrieves important event**
- Mitigation: Long TTL for queryable events (hours, not seconds)
- Mitigation: Anomalies propagate immediately, not query-only

## Implementation Phases

### Phase 1: Basic Event Flow (Week 1-2)
- EventEmitter with priority queues
- Simple passthrough (no aggregation)
- Wire protocol implementation

### Phase 2: Aggregation (Week 3-4)
- EchelonAggregator implementation
- Summary strategies for detection/telemetry
- Aggregation window management

### Phase 3: Query Protocol (Week 5-6)
- EventQueryHandler implementation
- Query fan-out and response aggregation
- Local event storage with TTL

### Phase 4: Priority Enforcement (Week 7)
- Weighted fair queuing implementation
- Bandwidth allocation configuration
- Integration with ADR-019 QoS

### Phase 5: Integration Testing (Week 8)
- 48-node platoon simulation
- Bandwidth measurement validation
- Latency profiling

## References

### Related ADRs
- ADR-012: Schema Definition (HiveEvent, AggregationPolicy schemas)
- ADR-009: Bidirectional Hierarchical Flows
- ADR-019: QoS and Data Prioritization
- ADR-001: HIVE Protocol PoC

### Algorithms
- Weighted Fair Queuing (WFQ)
- Hierarchical Token Bucket (HTB)
- Leaky Bucket rate limiting

### Prior Art
- MQTT QoS levels
- Kafka topic partitioning and consumer groups
- Military message precedence (FLASH, IMMEDIATE, PRIORITY, ROUTINE)

---

**This ADR specifies how HIVE Protocol routes events through the hierarchy, enforces aggregation policies, and enables bandwidth-efficient distributed coordination.**
