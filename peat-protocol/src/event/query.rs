//! Event Query Protocol (ADR-027 Phase 3)
//!
//! Events with `PropagationMode::Query` are stored locally and accessible via query.
//! Higher echelons can query subordinates for events that were not automatically propagated.
//!
//! ## Query Flow
//!
//! ```text
//! Company → EventQuery → Platoon → Forward to Squad(s) → Collect Results
//!    ↑                      ↓
//!    └──────── EventQueryResponse ←──────────────────────┘
//! ```
//!
//! ## Query Scopes
//!
//! - `Node { node_id }`: Query specific node's local store
//! - `Formation { formation_id }`: Query all nodes in a formation
//! - `Subordinates`: Query all direct subordinates of requester

use peat_schema::event::v1::{
    EventClass, EventFilters, EventQuery, EventQueryResponse, PeatEvent, QueryScope,
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

/// Trait for event storage backends
pub trait EventStore: Send + Sync {
    /// Query events matching the given filters
    fn query(
        &self,
        event_class: Option<EventClass>,
        event_type: Option<&str>,
        after_seconds: Option<u64>,
        before_seconds: Option<u64>,
        source_instance_id: Option<&str>,
        limit: u32,
    ) -> Vec<PeatEvent>;

    /// Store an event
    fn store(&self, event: PeatEvent);

    /// Get count of stored events
    fn count(&self) -> usize;

    /// Remove expired events (TTL enforcement)
    fn remove_expired(&self);
}

/// In-memory event store implementation
#[derive(Debug, Default)]
pub struct InMemoryEventStore {
    events: RwLock<HashMap<String, PeatEvent>>,
}

impl InMemoryEventStore {
    /// Create a new in-memory event store
    pub fn new() -> Self {
        Self {
            events: RwLock::new(HashMap::new()),
        }
    }
}

impl EventStore for InMemoryEventStore {
    fn query(
        &self,
        event_class: Option<EventClass>,
        event_type: Option<&str>,
        after_seconds: Option<u64>,
        before_seconds: Option<u64>,
        source_instance_id: Option<&str>,
        limit: u32,
    ) -> Vec<PeatEvent> {
        let events = self.events.read().unwrap();

        let mut results: Vec<_> = events
            .values()
            .filter(|e| {
                // Filter by event class
                if let Some(class) = event_class {
                    if e.event_class != class as i32 {
                        return false;
                    }
                }

                // Filter by event type
                if let Some(et) = event_type {
                    if !e.event_type.starts_with(et) {
                        return false;
                    }
                }

                // Filter by time range
                if let Some(ts) = &e.timestamp {
                    if let Some(after) = after_seconds {
                        if ts.seconds < after {
                            return false;
                        }
                    }
                    if let Some(before) = before_seconds {
                        if ts.seconds > before {
                            return false;
                        }
                    }
                }

                // Filter by source instance
                if let Some(sid) = source_instance_id {
                    if e.source_instance_id.as_deref() != Some(sid) {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect();

        // Sort by timestamp (newest first)
        results.sort_by(|a, b| {
            let ts_a = a.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0);
            let ts_b = b.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0);
            ts_b.cmp(&ts_a)
        });

        // Apply limit
        if limit > 0 && results.len() > limit as usize {
            results.truncate(limit as usize);
        }

        results
    }

    fn store(&self, event: PeatEvent) {
        let mut events = self.events.write().unwrap();
        events.insert(event.event_id.clone(), event);
    }

    fn count(&self) -> usize {
        let events = self.events.read().unwrap();
        events.len()
    }

    fn remove_expired(&self) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut events = self.events.write().unwrap();
        events.retain(|_, event| {
            if let Some(routing) = &event.routing {
                if routing.ttl_seconds > 0 {
                    if let Some(ts) = &event.timestamp {
                        let expiry = ts.seconds + routing.ttl_seconds as u64;
                        return now < expiry;
                    }
                }
            }
            true // Keep events without TTL or timestamp
        });
    }
}

/// Handler for event queries
///
/// Processes incoming queries, applying filters to the local event store
/// and optionally forwarding queries to subordinates.
pub struct EventQueryHandler {
    /// Node ID for this handler
    node_id: String,

    /// Formation ID this node belongs to
    formation_id: String,

    /// Local event store
    event_store: Arc<dyn EventStore>,

    /// Subordinate node IDs (for Subordinates scope queries)
    subordinate_ids: RwLock<Vec<String>>,
}

impl EventQueryHandler {
    /// Create a new query handler
    pub fn new(node_id: String, formation_id: String, event_store: Arc<dyn EventStore>) -> Self {
        Self {
            node_id,
            formation_id,
            event_store,
            subordinate_ids: RwLock::new(Vec::new()),
        }
    }

    /// Create a new query handler with a default in-memory store
    pub fn with_memory_store(node_id: String, formation_id: String) -> Self {
        Self::new(node_id, formation_id, Arc::new(InMemoryEventStore::new()))
    }

    /// Register a subordinate node
    pub fn add_subordinate(&self, node_id: &str) {
        let mut subs = self.subordinate_ids.write().unwrap();
        if !subs.contains(&node_id.to_string()) {
            subs.push(node_id.to_string());
        }
    }

    /// Remove a subordinate node
    pub fn remove_subordinate(&self, node_id: &str) {
        let mut subs = self.subordinate_ids.write().unwrap();
        subs.retain(|id| id != node_id);
    }

    /// Get the node ID
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Get the formation ID
    pub fn formation_id(&self) -> &str {
        &self.formation_id
    }

    /// Store an event in the local store
    pub fn store_event(&self, event: PeatEvent) {
        self.event_store.store(event);
    }

    /// Get count of stored events
    pub fn event_count(&self) -> usize {
        self.event_store.count()
    }

    /// Handle an incoming query
    ///
    /// For local queries (node_id matches), returns results from local store.
    /// For subordinate queries, returns information about which nodes should be queried.
    pub fn handle_query(&self, query: &EventQuery) -> QueryResult {
        let scope = query.scope.as_ref();

        // Determine query target
        if let Some(scope) = scope {
            if let Some(target) = &scope.target {
                match target {
                    peat_schema::event::v1::query_scope::Target::NodeId(node_id) => {
                        if node_id == &self.node_id {
                            // Query local store
                            return QueryResult::Local(self.query_local(query));
                        } else {
                            // Forward to specific node
                            return QueryResult::Forward(vec![node_id.clone()]);
                        }
                    }
                    peat_schema::event::v1::query_scope::Target::FormationId(formation_id) => {
                        if formation_id == &self.formation_id {
                            // Query local store (we're in this formation)
                            // Plus forward to subordinates
                            let local_result = self.query_local(query);
                            let subs = self.subordinate_ids.read().unwrap();
                            if subs.is_empty() {
                                return QueryResult::Local(local_result);
                            } else {
                                return QueryResult::LocalPlusForward(local_result, subs.clone());
                            }
                        } else {
                            // Not our formation, forward to subordinates
                            let subs = self.subordinate_ids.read().unwrap();
                            return QueryResult::Forward(subs.clone());
                        }
                    }
                    peat_schema::event::v1::query_scope::Target::Subordinates(_) => {
                        // Query all subordinates
                        let subs = self.subordinate_ids.read().unwrap();
                        if subs.is_empty() {
                            // No subordinates, return empty
                            return QueryResult::Local(self.empty_response(query));
                        } else {
                            return QueryResult::Forward(subs.clone());
                        }
                    }
                }
            }
        }

        // Default: query local
        QueryResult::Local(self.query_local(query))
    }

    /// Query the local event store
    pub fn query_local(&self, query: &EventQuery) -> EventQueryResponse {
        let filters = query.filters.as_ref();

        let event_class = filters.and_then(|f| {
            f.event_class
                .map(|ec| EventClass::try_from(ec).unwrap_or(EventClass::Unspecified))
        });
        let event_type = filters.and_then(|f| f.event_type.as_deref());
        let after_seconds = filters.and_then(|f| f.after_seconds);
        let before_seconds = filters.and_then(|f| f.before_seconds);
        let source_instance_id = filters.and_then(|f| f.source_instance_id.as_deref());
        let limit = query.limit;

        let events = self.event_store.query(
            event_class,
            event_type,
            after_seconds,
            before_seconds,
            source_instance_id,
            limit,
        );

        let total_matching = events.len() as u32;
        let truncated = limit > 0 && total_matching >= limit;

        EventQueryResponse {
            query_id: query.query_id.clone(),
            responder_id: self.node_id.clone(),
            events,
            total_matching,
            truncated,
        }
    }

    /// Create an empty response for queries with no results
    fn empty_response(&self, query: &EventQuery) -> EventQueryResponse {
        EventQueryResponse {
            query_id: query.query_id.clone(),
            responder_id: self.node_id.clone(),
            events: vec![],
            total_matching: 0,
            truncated: false,
        }
    }

    /// Merge multiple query responses into one
    pub fn merge_responses(
        query_id: &str,
        responder_id: &str,
        responses: Vec<EventQueryResponse>,
        limit: u32,
    ) -> EventQueryResponse {
        let mut all_events: Vec<PeatEvent> = responses
            .into_iter()
            .flat_map(|r| r.events.into_iter())
            .collect();

        // Sort by timestamp (newest first)
        all_events.sort_by(|a, b| {
            let ts_a = a.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0);
            let ts_b = b.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0);
            ts_b.cmp(&ts_a)
        });

        let total_matching = all_events.len() as u32;
        let truncated = limit > 0 && total_matching > limit;

        if limit > 0 && all_events.len() > limit as usize {
            all_events.truncate(limit as usize);
        }

        EventQueryResponse {
            query_id: query_id.to_string(),
            responder_id: responder_id.to_string(),
            events: all_events,
            total_matching,
            truncated,
        }
    }

    /// Create a query for a specific node
    pub fn create_node_query(
        requester_id: &str,
        node_id: &str,
        filters: Option<EventFilters>,
        limit: u32,
    ) -> EventQuery {
        EventQuery {
            query_id: generate_query_id(),
            requester_id: requester_id.to_string(),
            scope: Some(QueryScope {
                target: Some(peat_schema::event::v1::query_scope::Target::NodeId(
                    node_id.to_string(),
                )),
            }),
            filters,
            limit,
        }
    }

    /// Create a query for a formation
    pub fn create_formation_query(
        requester_id: &str,
        formation_id: &str,
        filters: Option<EventFilters>,
        limit: u32,
    ) -> EventQuery {
        EventQuery {
            query_id: generate_query_id(),
            requester_id: requester_id.to_string(),
            scope: Some(QueryScope {
                target: Some(peat_schema::event::v1::query_scope::Target::FormationId(
                    formation_id.to_string(),
                )),
            }),
            filters,
            limit,
        }
    }

    /// Create a query for all subordinates
    pub fn create_subordinates_query(
        requester_id: &str,
        filters: Option<EventFilters>,
        limit: u32,
    ) -> EventQuery {
        EventQuery {
            query_id: generate_query_id(),
            requester_id: requester_id.to_string(),
            scope: Some(QueryScope {
                target: Some(peat_schema::event::v1::query_scope::Target::Subordinates(
                    true,
                )),
            }),
            filters,
            limit,
        }
    }

    /// Remove expired events from the store
    pub fn enforce_ttl(&self) {
        self.event_store.remove_expired();
    }
}

/// Result of handling a query
#[derive(Debug)]
pub enum QueryResult {
    /// Query was handled locally, response ready
    Local(EventQueryResponse),

    /// Query should be forwarded to these node IDs
    Forward(Vec<String>),

    /// Query handled locally AND should be forwarded
    LocalPlusForward(EventQueryResponse, Vec<String>),
}

/// Generate a unique query ID
fn generate_query_id() -> String {
    format!(
        "qry-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

/// Create event filters
pub fn create_filters(
    event_class: Option<EventClass>,
    event_type: Option<&str>,
    after_seconds: Option<u64>,
    before_seconds: Option<u64>,
    source_instance_id: Option<&str>,
) -> EventFilters {
    EventFilters {
        event_class: event_class.map(|ec| ec as i32),
        event_type: event_type.map(|s| s.to_string()),
        after_seconds,
        before_seconds,
        source_instance_id: source_instance_id.map(|s| s.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peat_schema::common::v1::Timestamp;
    use peat_schema::event::v1::{AggregationPolicy, EventPriority, PropagationMode};

    fn make_event(id: &str, event_type: &str, timestamp_seconds: u64) -> PeatEvent {
        PeatEvent {
            event_id: id.to_string(),
            timestamp: Some(Timestamp {
                seconds: timestamp_seconds,
                nanos: 0,
            }),
            source_node_id: "node-1".to_string(),
            source_formation_id: "squad-1".to_string(),
            source_instance_id: Some("instance-1".to_string()),
            event_class: EventClass::Product as i32,
            event_type: event_type.to_string(),
            routing: Some(AggregationPolicy {
                propagation: PropagationMode::PropagationQuery as i32,
                priority: EventPriority::PriorityNormal as i32,
                ttl_seconds: 300,
                aggregation_window_ms: 0,
            }),
            payload_type_url: String::new(),
            payload_value: vec![],
        }
    }

    #[test]
    fn test_in_memory_store_basic() {
        let store = InMemoryEventStore::new();

        let event = make_event("evt-1", "detection", 1000);
        store.store(event);

        assert_eq!(store.count(), 1);

        let results = store.query(None, None, None, None, None, 0);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_id, "evt-1");
    }

    #[test]
    fn test_in_memory_store_filter_by_type() {
        let store = InMemoryEventStore::new();

        store.store(make_event("evt-1", "detection.vehicle", 1000));
        store.store(make_event("evt-2", "telemetry.cpu", 1001));
        store.store(make_event("evt-3", "detection.person", 1002));

        let results = store.query(None, Some("detection"), None, None, None, 0);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_in_memory_store_filter_by_time() {
        let store = InMemoryEventStore::new();

        store.store(make_event("evt-1", "detection", 1000));
        store.store(make_event("evt-2", "detection", 2000));
        store.store(make_event("evt-3", "detection", 3000));

        let results = store.query(None, None, Some(1500), Some(2500), None, 0);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_id, "evt-2");
    }

    #[test]
    fn test_in_memory_store_limit() {
        let store = InMemoryEventStore::new();

        for i in 0..10 {
            store.store(make_event(&format!("evt-{}", i), "detection", 1000 + i));
        }

        let results = store.query(None, None, None, None, None, 5);
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_query_handler_local_query() {
        let handler =
            EventQueryHandler::with_memory_store("node-1".to_string(), "squad-1".to_string());

        handler.store_event(make_event("evt-1", "detection", 1000));
        handler.store_event(make_event("evt-2", "detection", 1001));

        let query = EventQueryHandler::create_node_query("requester-1", "node-1", None, 0);

        match handler.handle_query(&query) {
            QueryResult::Local(response) => {
                assert_eq!(response.events.len(), 2);
                assert_eq!(response.responder_id, "node-1");
            }
            _ => panic!("Expected Local result"),
        }
    }

    #[test]
    fn test_query_handler_forward_to_node() {
        let handler =
            EventQueryHandler::with_memory_store("platoon-1".to_string(), "platoon-1".to_string());

        handler.add_subordinate("squad-1");
        handler.add_subordinate("squad-2");

        let query = EventQueryHandler::create_node_query("requester-1", "squad-1", None, 0);

        match handler.handle_query(&query) {
            QueryResult::Forward(nodes) => {
                assert_eq!(nodes.len(), 1);
                assert_eq!(nodes[0], "squad-1");
            }
            _ => panic!("Expected Forward result"),
        }
    }

    #[test]
    fn test_query_handler_subordinates_query() {
        let handler =
            EventQueryHandler::with_memory_store("platoon-1".to_string(), "platoon-1".to_string());

        handler.add_subordinate("squad-1");
        handler.add_subordinate("squad-2");
        handler.add_subordinate("squad-3");

        let query = EventQueryHandler::create_subordinates_query("requester-1", None, 0);

        match handler.handle_query(&query) {
            QueryResult::Forward(nodes) => {
                assert_eq!(nodes.len(), 3);
                assert!(nodes.contains(&"squad-1".to_string()));
                assert!(nodes.contains(&"squad-2".to_string()));
                assert!(nodes.contains(&"squad-3".to_string()));
            }
            _ => panic!("Expected Forward result"),
        }
    }

    #[test]
    fn test_query_handler_formation_query_local() {
        let handler =
            EventQueryHandler::with_memory_store("node-1".to_string(), "squad-1".to_string());

        handler.store_event(make_event("evt-1", "detection", 1000));

        // Query for our own formation
        let query = EventQueryHandler::create_formation_query("requester-1", "squad-1", None, 0);

        match handler.handle_query(&query) {
            QueryResult::Local(response) => {
                assert_eq!(response.events.len(), 1);
            }
            _ => panic!("Expected Local result"),
        }
    }

    #[test]
    fn test_merge_responses() {
        let resp1 = EventQueryResponse {
            query_id: "qry-1".to_string(),
            responder_id: "node-1".to_string(),
            events: vec![make_event("evt-1", "detection", 1000)],
            total_matching: 1,
            truncated: false,
        };

        let resp2 = EventQueryResponse {
            query_id: "qry-1".to_string(),
            responder_id: "node-2".to_string(),
            events: vec![
                make_event("evt-2", "detection", 2000),
                make_event("evt-3", "detection", 1500),
            ],
            total_matching: 2,
            truncated: false,
        };

        let merged =
            EventQueryHandler::merge_responses("qry-1", "platoon-1", vec![resp1, resp2], 0);

        assert_eq!(merged.events.len(), 3);
        assert_eq!(merged.total_matching, 3);
        assert!(!merged.truncated);

        // Should be sorted by timestamp descending
        assert_eq!(merged.events[0].event_id, "evt-2"); // 2000
        assert_eq!(merged.events[1].event_id, "evt-3"); // 1500
        assert_eq!(merged.events[2].event_id, "evt-1"); // 1000
    }

    #[test]
    fn test_merge_responses_with_limit() {
        let resp1 = EventQueryResponse {
            query_id: "qry-1".to_string(),
            responder_id: "node-1".to_string(),
            events: vec![
                make_event("evt-1", "detection", 1000),
                make_event("evt-2", "detection", 2000),
            ],
            total_matching: 2,
            truncated: false,
        };

        let resp2 = EventQueryResponse {
            query_id: "qry-1".to_string(),
            responder_id: "node-2".to_string(),
            events: vec![
                make_event("evt-3", "detection", 3000),
                make_event("evt-4", "detection", 4000),
            ],
            total_matching: 2,
            truncated: false,
        };

        let merged =
            EventQueryHandler::merge_responses("qry-1", "platoon-1", vec![resp1, resp2], 2);

        assert_eq!(merged.events.len(), 2);
        assert_eq!(merged.total_matching, 4);
        assert!(merged.truncated);

        // Should have the two most recent events
        assert_eq!(merged.events[0].event_id, "evt-4"); // 4000
        assert_eq!(merged.events[1].event_id, "evt-3"); // 3000
    }

    #[test]
    fn test_create_filters() {
        let filters = create_filters(
            Some(EventClass::Product),
            Some("detection"),
            Some(1000),
            Some(2000),
            Some("instance-1"),
        );

        assert_eq!(filters.event_class, Some(EventClass::Product as i32));
        assert_eq!(filters.event_type, Some("detection".to_string()));
        assert_eq!(filters.after_seconds, Some(1000));
        assert_eq!(filters.before_seconds, Some(2000));
        assert_eq!(filters.source_instance_id, Some("instance-1".to_string()));
    }

    #[test]
    fn test_ttl_enforcement() {
        let store = InMemoryEventStore::new();

        // Event with short TTL (already expired)
        let mut event = make_event("evt-1", "detection", 1); // Very old timestamp
        event.routing.as_mut().unwrap().ttl_seconds = 10;
        store.store(event);

        // Event without TTL (should be kept)
        let mut event2 = make_event("evt-2", "detection", 1);
        event2.routing.as_mut().unwrap().ttl_seconds = 0;
        store.store(event2);

        // Recent event with TTL (should be kept)
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut event3 = make_event("evt-3", "detection", now);
        event3.routing.as_mut().unwrap().ttl_seconds = 3600;
        store.store(event3);

        assert_eq!(store.count(), 3);
        store.remove_expired();
        assert_eq!(store.count(), 2); // evt-1 should be removed
    }
}
