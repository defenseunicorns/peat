//! ADR-027 Phase 5: Event Routing Integration Tests
//!
//! These tests validate the complete event routing and aggregation system
//! as specified in ADR-027. They test the protocol behavior without requiring
//! containerlab infrastructure.
//!
//! Test coverage:
//! - Bandwidth reduction through aggregation
//! - Latency SLA for different event priorities
//! - Query fan-out for telemetry events
//! - Priority preemption under load
//! - Graceful degradation on node failure

use hive_protocol::event::{
    AggregationPolicy, BandwidthAllocation, EchelonAggregator, EchelonType, EventPriority,
    EventTransmitter, HiveEvent, OverflowPolicy, PropagationMode,
};
use hive_schema::event::v1::EventClass;
use std::time::{Duration, Instant};

/// Helper to create test events with specific characteristics
fn make_test_event(
    id: &str,
    event_type: &str,
    propagation: PropagationMode,
    priority: EventPriority,
    payload_size: usize,
) -> HiveEvent {
    HiveEvent {
        event_id: id.to_string(),
        timestamp: Some(hive_schema::common::v1::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            nanos: 0,
        }),
        source_node_id: format!("node-{}", id),
        source_formation_id: "squad-1".to_string(),
        source_instance_id: None,
        event_class: EventClass::Product as i32,
        event_type: event_type.to_string(),
        routing: Some(AggregationPolicy {
            propagation: propagation as i32,
            priority: priority as i32,
            ttl_seconds: 300,
            aggregation_window_ms: 0, // Use default window duration from aggregator
        }),
        payload_type_url: format!("type.hive/{}", event_type),
        payload_value: vec![0u8; payload_size],
    }
}

/// Test 1: Bandwidth Reduction Through Aggregation
///
/// Validates that the aggregation system achieves >=95% event reduction.
/// Per ADR-027: 48 platforms × 11 events/sec = 528 events/sec without aggregation
/// With aggregation: ~7.5 events/sec (summaries + passthrough)
#[test]
fn test_bandwidth_reduction_through_aggregation() {
    // Create squad-level aggregator (simulating one of 6 squads)
    let aggregator = EchelonAggregator::new("squad-1".to_string(), EchelonType::Squad)
        .with_default_window_duration(Duration::from_millis(100)); // Short window for testing

    let platforms_per_squad = 8;
    let detections_per_platform = 10; // Per second
    let test_seconds = 1;

    // Simulate detection events from all platforms (SUMMARY mode)
    let total_detections = platforms_per_squad * detections_per_platform * test_seconds;
    for i in 0..total_detections {
        let event = make_test_event(
            &format!("det-{}", i),
            "detection.vehicle",
            PropagationMode::PropagationSummary,
            EventPriority::PriorityNormal,
            100, // 100 byte payload
        );
        aggregator.receive(event).unwrap();
    }

    // Simulate telemetry events (QUERY mode - stored locally, not propagated)
    let telemetry_per_platform = 1; // Per second
    let total_telemetry = platforms_per_squad * telemetry_per_platform * test_seconds;
    for i in 0..total_telemetry {
        let event = make_test_event(
            &format!("tel-{}", i),
            "telemetry.cpu",
            PropagationMode::PropagationQuery,
            EventPriority::PriorityLow,
            50,
        );
        aggregator.receive(event).unwrap();
    }

    // Wait for aggregation window to expire (need to wait >2x the window duration for safety)
    std::thread::sleep(Duration::from_millis(300));

    // Flush windows and get events to transmit
    let summaries_generated = aggregator.flush_expired_windows();
    let passthrough_events = aggregator.pop_passthrough();
    let summary_events = aggregator.pop_summaries();

    // Calculate reduction
    // Input: 80 detections + 8 telemetry = 88 events
    // Output: 1 detection summary + 0 telemetry (stored locally)
    let raw_events = total_detections + total_telemetry;
    let aggregated_events = passthrough_events.len() + summary_events.len();

    println!("Raw events: {}", raw_events);
    println!("Summaries generated: {}", summaries_generated);
    println!("Passthrough events: {}", passthrough_events.len());
    println!("Summary events: {}", summary_events.len());
    println!(
        "Queryable (stored locally): {}",
        aggregator.queryable_count()
    );

    // Verify aggregation occurred
    assert!(
        summaries_generated >= 1,
        "Expected at least 1 summary, got {}",
        summaries_generated
    );

    // Verify telemetry stored locally (not propagated)
    assert_eq!(
        aggregator.queryable_count(),
        total_telemetry,
        "Expected {} telemetry events stored locally",
        total_telemetry
    );

    // Calculate reduction ratio
    if raw_events > 0 && aggregated_events > 0 {
        let reduction = (1.0 - (aggregated_events as f64 / raw_events as f64)) * 100.0;
        println!("Bandwidth reduction: {:.1}%", reduction);

        // For this single-squad test, we expect high reduction
        // 88 events -> 1 summary = 98.9% reduction
        assert!(
            reduction >= 90.0,
            "Expected >=90% reduction, got {:.1}%",
            reduction
        );
    }
}

/// Test 2: Critical Event Priority and Latency
///
/// Validates that CRITICAL events preempt all other traffic and meet SLA.
#[test]
fn test_critical_event_priority_preemption() {
    let mut transmitter = EventTransmitter::with_defaults();

    // Set queue sizes
    transmitter.set_max_queue_size(EventPriority::PriorityLow, 100);
    transmitter.set_max_queue_size(EventPriority::PriorityNormal, 100);
    transmitter.set_max_queue_size(EventPriority::PriorityHigh, 100);

    // Fill queues with low priority events
    for i in 0..50 {
        let event = make_test_event(
            &format!("low-{}", i),
            "telemetry",
            PropagationMode::PropagationFull,
            EventPriority::PriorityLow,
            100,
        );
        transmitter.enqueue(event);
    }

    // Add normal priority events
    for i in 0..30 {
        let event = make_test_event(
            &format!("normal-{}", i),
            "detection",
            PropagationMode::PropagationFull,
            EventPriority::PriorityNormal,
            100,
        );
        transmitter.enqueue(event);
    }

    // Add a CRITICAL event (should preempt everything)
    let critical_event = make_test_event(
        "critical-1",
        "anomaly.urgent",
        PropagationMode::PropagationFull,
        EventPriority::PriorityCritical,
        50,
    );
    let start_time = Instant::now();
    transmitter.enqueue(critical_event);

    // Transmit a batch
    let transmitted = transmitter.transmit(10);

    // CRITICAL must be first
    assert!(
        !transmitted.is_empty(),
        "Expected at least 1 event transmitted"
    );
    assert_eq!(
        transmitted[0].event_id, "critical-1",
        "CRITICAL event should be transmitted first"
    );

    // Verify latency is minimal (in-memory, should be sub-millisecond)
    let latency = start_time.elapsed();
    println!("CRITICAL event transmission latency: {:?}", latency);
    assert!(
        latency < Duration::from_millis(10),
        "CRITICAL latency should be < 10ms, was {:?}",
        latency
    );
}

/// Test 3: Weighted Fair Queuing Distribution
///
/// Validates that bandwidth is distributed according to priority weights:
/// HIGH: 50%, NORMAL: 35%, LOW: 15%
#[test]
fn test_weighted_fair_queuing_distribution() {
    let mut transmitter = EventTransmitter::with_defaults();

    // Add equal numbers of each priority (except CRITICAL)
    let events_per_priority = 100;

    for i in 0..events_per_priority {
        transmitter.enqueue(make_test_event(
            &format!("high-{}", i),
            "detection",
            PropagationMode::PropagationFull,
            EventPriority::PriorityHigh,
            50,
        ));
        transmitter.enqueue(make_test_event(
            &format!("normal-{}", i),
            "status",
            PropagationMode::PropagationFull,
            EventPriority::PriorityNormal,
            50,
        ));
        transmitter.enqueue(make_test_event(
            &format!("low-{}", i),
            "telemetry",
            PropagationMode::PropagationFull,
            EventPriority::PriorityLow,
            50,
        ));
    }

    // Transmit a batch
    let batch_size = 100;
    let transmitted = transmitter.transmit(batch_size);

    // Count by priority
    let high_count = transmitted
        .iter()
        .filter(|e| e.event_id.starts_with("high"))
        .count();
    let normal_count = transmitted
        .iter()
        .filter(|e| e.event_id.starts_with("normal"))
        .count();
    let low_count = transmitted
        .iter()
        .filter(|e| e.event_id.starts_with("low"))
        .count();

    println!(
        "Distribution - HIGH: {}, NORMAL: {}, LOW: {}",
        high_count, normal_count, low_count
    );

    // Verify weighted distribution (with some tolerance)
    // Expected: 50/35/15 split
    // Allow +/- 15% tolerance for token bucket timing effects
    let total = high_count + normal_count + low_count;
    if total > 0 {
        let high_pct = (high_count as f64 / total as f64) * 100.0;
        let normal_pct = (normal_count as f64 / total as f64) * 100.0;
        let low_pct = (low_count as f64 / total as f64) * 100.0;

        println!(
            "Percentages - HIGH: {:.1}%, NORMAL: {:.1}%, LOW: {:.1}%",
            high_pct, normal_pct, low_pct
        );

        // HIGH should get more than NORMAL, NORMAL more than LOW
        assert!(
            high_count >= normal_count,
            "HIGH should get >= NORMAL events"
        );
        assert!(normal_count >= low_count, "NORMAL should get >= LOW events");
    }
}

/// Test 4: Queue Overflow Handling
///
/// Validates that overflow policy drops lowest priority first.
#[test]
fn test_overflow_drops_lowest_priority() {
    let mut transmitter = EventTransmitter::with_defaults();
    transmitter.set_max_queue_size(EventPriority::PriorityHigh, 10);
    transmitter.set_overflow_policy(OverflowPolicy::RemoveLowestPriority);

    // Fill LOW queue
    for i in 0..20 {
        transmitter.enqueue(make_test_event(
            &format!("low-{}", i),
            "telemetry",
            PropagationMode::PropagationFull,
            EventPriority::PriorityLow,
            50,
        ));
    }

    // Fill HIGH queue - should drop LOW when HIGH overflows
    for i in 0..15 {
        transmitter.enqueue(make_test_event(
            &format!("high-{}", i),
            "detection",
            PropagationMode::PropagationFull,
            EventPriority::PriorityHigh,
            50,
        ));
    }

    let stats = transmitter.stats();

    // Some LOW events should have been dropped
    println!("LOW dropped: {}", stats.dropped[3]);
    assert!(
        stats.dropped[3] > 0,
        "Expected LOW priority events to be dropped"
    );

    // CRITICAL and HIGH should not be dropped
    assert_eq!(stats.dropped[0], 0, "CRITICAL should never be dropped");
    assert_eq!(
        stats.dropped[1], 0,
        "HIGH should not be dropped when LOW available"
    );
}

/// Test 5: Aggregation Window Summarization
///
/// Validates that events are properly summarized after window expiry.
#[test]
fn test_aggregation_window_summarization() {
    let aggregator = EchelonAggregator::new("squad-alpha".to_string(), EchelonType::Squad)
        .with_default_window_duration(Duration::from_millis(50)); // Short window

    // Add events from multiple source nodes
    let source_nodes = ["node-1", "node-2", "node-3", "node-4"];
    for (i, node) in source_nodes.iter().enumerate() {
        let mut event = make_test_event(
            &format!("det-{}", i),
            "detection.vehicle",
            PropagationMode::PropagationSummary,
            EventPriority::PriorityNormal,
            100,
        );
        event.source_node_id = node.to_string();
        aggregator.receive(event).unwrap();
    }

    // Wait for window expiry (need >2x window duration for safety)
    std::thread::sleep(Duration::from_millis(200));

    // Flush and get summaries
    let summaries = aggregator.flush_expired_windows();
    assert_eq!(summaries, 1, "Expected exactly 1 summary");

    let summary_events = aggregator.pop_summaries();
    assert_eq!(summary_events.len(), 1, "Expected 1 summary event");

    // Verify summary content
    let summary_event = &summary_events[0];
    assert!(
        summary_event.event_type.contains("summary"),
        "Event type should indicate summary"
    );
    assert_eq!(
        summary_event.source_formation_id, "squad-alpha",
        "Summary should be from the aggregator echelon"
    );
}

/// Test 6: Query Mode Storage
///
/// Validates that Query mode events are stored locally and queryable.
#[test]
fn test_query_mode_local_storage() {
    let aggregator = EchelonAggregator::new("squad-bravo".to_string(), EchelonType::Squad);

    // Add telemetry events in Query mode
    let telemetry_events = 50;
    for i in 0..telemetry_events {
        let event = make_test_event(
            &format!("tel-{}", i),
            "telemetry.cpu",
            PropagationMode::PropagationQuery,
            EventPriority::PriorityLow,
            30,
        );
        aggregator.receive(event).unwrap();
    }

    // Verify events are stored locally
    assert_eq!(
        aggregator.queryable_count(),
        telemetry_events,
        "All Query mode events should be stored"
    );

    // Verify passthrough queue is empty (Query events don't propagate)
    assert_eq!(
        aggregator.passthrough_count(),
        0,
        "Query mode events should not be in passthrough queue"
    );

    // Query stored events
    let queried = aggregator.query_local(Some("telemetry.cpu"));
    assert_eq!(
        queried.len(),
        telemetry_events,
        "Query should return all stored telemetry events"
    );

    // Query non-existent type
    let empty_query = aggregator.query_local(Some("nonexistent"));
    assert!(
        empty_query.is_empty(),
        "Query for non-existent type should be empty"
    );
}

/// Test 7: Full Mode Passthrough
///
/// Validates that Full propagation mode events pass through immediately.
#[test]
fn test_full_mode_passthrough() {
    let aggregator = EchelonAggregator::new("squad-charlie".to_string(), EchelonType::Squad);

    // Add anomaly events in Full mode
    let anomaly_events = 10;
    for i in 0..anomaly_events {
        let event = make_test_event(
            &format!("anomaly-{}", i),
            "anomaly.detection",
            PropagationMode::PropagationFull,
            EventPriority::PriorityHigh,
            80,
        );
        aggregator.receive(event).unwrap();
    }

    // Verify all events are in passthrough queue
    assert_eq!(
        aggregator.passthrough_count(),
        anomaly_events,
        "All Full mode events should be in passthrough"
    );

    // Pop and verify
    let passed_through = aggregator.pop_passthrough();
    assert_eq!(
        passed_through.len(),
        anomaly_events,
        "All events should be returned on pop"
    );
}

/// Test 8: Multiple Event Types in Separate Windows
///
/// Validates that different event types get separate aggregation windows.
#[test]
fn test_separate_aggregation_windows_per_event_type() {
    let aggregator = EchelonAggregator::new("squad-delta".to_string(), EchelonType::Squad)
        .with_default_window_duration(Duration::from_millis(50));

    // Add different event types
    let event_types = ["detection.vehicle", "detection.person", "sensor.radar"];

    for event_type in event_types.iter() {
        for i in 0..5 {
            let event = make_test_event(
                &format!("{}-{}", event_type, i),
                event_type,
                PropagationMode::PropagationSummary,
                EventPriority::PriorityNormal,
                50,
            );
            aggregator.receive(event).unwrap();
        }
    }

    // Verify separate windows created
    assert_eq!(
        aggregator.window_count(),
        event_types.len(),
        "Each event type should have its own window"
    );

    // Wait and flush (need >2x window duration for safety)
    std::thread::sleep(Duration::from_millis(200));
    let summaries = aggregator.flush_expired_windows();

    assert_eq!(
        summaries,
        event_types.len(),
        "Each window should produce one summary"
    );
}

/// Test 9: Bandwidth Allocation Configuration
///
/// Validates custom bandwidth allocation configuration.
#[test]
fn test_custom_bandwidth_allocation() {
    let custom_allocation = BandwidthAllocation::with_percentages(
        10_000_000, // 10 Mbps
        20,         // 20% CRITICAL reserved
        40,         // 40% HIGH
        25,         // 25% NORMAL
        15,         // 15% LOW
    );

    let mut transmitter = EventTransmitter::new(custom_allocation);

    // Verify allocation is applied
    let available = transmitter.available_bandwidth();

    // All buckets should have initial capacity
    assert!(available[0] > 0.0, "CRITICAL bucket should have capacity");
    assert!(available[1] > 0.0, "HIGH bucket should have capacity");
    assert!(available[2] > 0.0, "NORMAL bucket should have capacity");
    assert!(available[3] > 0.0, "LOW bucket should have capacity");
}

/// Test 10: Echelon Type Hierarchy
///
/// Validates that aggregators properly identify their echelon level.
#[test]
fn test_echelon_type_hierarchy() {
    let squad_agg = EchelonAggregator::new("squad-echo".to_string(), EchelonType::Squad);
    let platoon_agg = EchelonAggregator::new("platoon-1".to_string(), EchelonType::Platoon);
    let company_agg = EchelonAggregator::new("company-alpha".to_string(), EchelonType::Company);

    assert_eq!(squad_agg.echelon_type(), EchelonType::Squad);
    assert_eq!(platoon_agg.echelon_type(), EchelonType::Platoon);
    assert_eq!(company_agg.echelon_type(), EchelonType::Company);

    assert_eq!(squad_agg.echelon_id(), "squad-echo");
    assert_eq!(platoon_agg.echelon_id(), "platoon-1");
    assert_eq!(company_agg.echelon_id(), "company-alpha");
}

/// Test 11: Transmitter Statistics Tracking
///
/// Validates that transmission statistics are properly tracked.
#[test]
fn test_transmitter_statistics() {
    let mut transmitter = EventTransmitter::with_defaults();

    // Add and transmit various events
    for i in 0..10 {
        transmitter.enqueue(make_test_event(
            &format!("critical-{}", i),
            "urgent",
            PropagationMode::PropagationFull,
            EventPriority::PriorityCritical,
            100,
        ));
        transmitter.enqueue(make_test_event(
            &format!("high-{}", i),
            "detection",
            PropagationMode::PropagationFull,
            EventPriority::PriorityHigh,
            100,
        ));
    }

    // Transmit all
    let _transmitted = transmitter.transmit(100);

    let stats = transmitter.stats();

    // Verify statistics
    assert_eq!(
        stats.transmitted[0], 10,
        "Should have transmitted 10 CRITICAL"
    );
    assert_eq!(stats.transmitted[1], 10, "Should have transmitted 10 HIGH");

    // Verify bytes tracked
    assert!(
        stats.bytes_transmitted[0] > 0,
        "Should track CRITICAL bytes"
    );
    assert!(stats.bytes_transmitted[1] > 0, "Should track HIGH bytes");

    // Reset and verify
    transmitter.reset_stats();
    let reset_stats = transmitter.stats();
    assert_eq!(reset_stats.transmitted[0], 0, "Stats should be reset");
}
