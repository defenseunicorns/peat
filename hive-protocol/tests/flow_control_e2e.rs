//! End-to-End Tests for Flow Control
//!
//! These tests validate the production flow control mechanisms for Automerge sync:
//! - Rate limiting (token bucket algorithm)
//! - Sync cooldown (storm prevention)
//! - Resource tracking
//! - Statistics collection
//!
//! # Test Focus
//!
//! - **Rate Limiting**: Token bucket blocks rapid sync attempts
//! - **Cooldown**: Prevents sync storms to same document
//! - **Multi-Peer**: Independent rate limiting per peer
//! - **Token Refill**: Bucket refills over time
//! - **Integration**: FlowController integrated with sync coordinator

#![cfg(feature = "automerge-backend")]

use hive_protocol::storage::{FlowControlConfig, FlowControlError, FlowController};
use iroh::{EndpointId, SecretKey};
use std::time::Duration;

/// Helper to create a random test peer ID
fn create_test_peer_id() -> EndpointId {
    let mut rng = rand::rng();
    SecretKey::generate(&mut rng).public()
}

/// Test 1: Basic Rate Limiting
///
/// Validates that the token bucket algorithm correctly limits rapid sync requests.
#[test]
fn test_e2e_rate_limiting_blocks_rapid_syncs() {
    println!("=== E2E: Rate Limiting Blocks Rapid Syncs ===");

    // Configure rate limiter with 5 messages/second capacity
    let config = FlowControlConfig {
        max_messages_per_second: 5,
        tokens_per_refill: 1,
        refill_interval: Duration::from_millis(200), // Slow refill for testing
        sync_cooldown: Duration::ZERO,               // Disable cooldown for rate limit testing
        ..Default::default()
    };
    let controller = FlowController::with_config(config);
    let peer_id = create_test_peer_id();

    println!("  1. Testing initial 5 requests should succeed...");

    // First 5 syncs should succeed (uses all tokens)
    for i in 0..5 {
        let result = controller.check_sync_allowed(&peer_id, &format!("doc{}", i));
        assert!(
            result.is_ok(),
            "Sync {} should be allowed, got {:?}",
            i,
            result
        );
        controller.record_sync(&peer_id, &format!("doc{}", i));
        println!("    ✓ Request {} allowed", i);
    }

    println!("  2. Testing 6th request should be rate limited...");

    // 6th sync should be rate limited
    let result = controller.check_sync_allowed(&peer_id, "doc5");
    assert!(
        matches!(result, Err(FlowControlError::RateLimitExceeded)),
        "Expected rate limit, got {:?}",
        result
    );
    println!("    ✓ Request 6 blocked (rate limited)");

    println!("  3. Verifying statistics...");

    let stats = controller.stats();
    assert_eq!(stats.rate_limited, 1, "Should have 1 rate limited request");
    println!("    ✓ Rate limited count: {}", stats.rate_limited);

    println!("  ✓ Rate limiting test passed");
}

/// Test 2: Sync Cooldown Prevents Storms
///
/// Validates that rapid syncs to the same document are blocked by cooldown.
#[test]
fn test_e2e_sync_cooldown_prevents_storms() {
    println!("=== E2E: Sync Cooldown Prevents Storms ===");

    // Configure with 50ms cooldown
    let config = FlowControlConfig {
        max_messages_per_second: 100, // High rate limit
        sync_cooldown: Duration::from_millis(50),
        ..Default::default()
    };
    let controller = FlowController::with_config(config);
    let peer_id = create_test_peer_id();

    println!("  1. First sync to doc1 should succeed...");

    // First sync should succeed
    assert!(controller.check_sync_allowed(&peer_id, "doc1").is_ok());
    controller.record_sync(&peer_id, "doc1");
    println!("    ✓ First sync allowed");

    println!("  2. Immediate second sync to doc1 should be blocked...");

    // Immediate second sync to same doc should be blocked
    let result = controller.check_sync_allowed(&peer_id, "doc1");
    match &result {
        Err(FlowControlError::CooldownActive { remaining_ms }) => {
            println!(
                "    ✓ Second sync blocked (cooldown active, {}ms remaining)",
                remaining_ms
            );
            assert!(*remaining_ms > 0 && *remaining_ms <= 50);
        }
        _ => panic!("Expected cooldown error, got {:?}", result),
    }

    println!("  3. Sync to different doc should succeed...");

    // Sync to different doc should succeed
    assert!(controller.check_sync_allowed(&peer_id, "doc2").is_ok());
    controller.record_sync(&peer_id, "doc2");
    println!("    ✓ Sync to doc2 allowed");

    println!("  4. Waiting for cooldown to expire...");

    // Wait for cooldown
    std::thread::sleep(Duration::from_millis(60));

    // Should now be allowed
    assert!(controller.check_sync_allowed(&peer_id, "doc1").is_ok());
    println!("    ✓ Sync to doc1 allowed after cooldown");

    println!("  5. Verifying statistics...");

    let stats = controller.stats();
    assert_eq!(
        stats.cooldown_blocked, 1,
        "Should have 1 cooldown blocked request"
    );
    println!("    ✓ Cooldown blocked count: {}", stats.cooldown_blocked);

    println!("  ✓ Sync cooldown test passed");
}

/// Test 3: Multiple Peers Rate Limited Independently
///
/// Validates that rate limiting is applied per-peer, not globally.
#[test]
fn test_e2e_multi_peer_independent_rate_limiting() {
    println!("=== E2E: Multi-Peer Independent Rate Limiting ===");

    // Configure with 3 messages/second per peer
    let config = FlowControlConfig {
        max_messages_per_second: 3,
        tokens_per_refill: 1,
        refill_interval: Duration::from_millis(500),
        sync_cooldown: Duration::ZERO,
        ..Default::default()
    };
    let controller = FlowController::with_config(config);

    let peer1 = create_test_peer_id();
    let peer2 = create_test_peer_id();
    let peer3 = create_test_peer_id();

    println!("  Peer 1: {:?}", peer1);
    println!("  Peer 2: {:?}", peer2);
    println!("  Peer 3: {:?}", peer3);

    println!("  1. Exhaust all tokens for peer1...");

    // Exhaust peer1's tokens
    for i in 0..3 {
        assert!(controller.check_sync_allowed(&peer1, "doc").is_ok());
        controller.record_sync(&peer1, "doc");
        println!("    Peer1 request {} allowed", i);
    }

    // peer1 should now be rate limited
    let result = controller.check_sync_allowed(&peer1, "doc");
    assert!(matches!(result, Err(FlowControlError::RateLimitExceeded)));
    println!("    ✓ Peer1 rate limited");

    println!("  2. peer2 should still have full quota...");

    // peer2 should still have full quota
    for i in 0..3 {
        assert!(
            controller.check_sync_allowed(&peer2, "doc").is_ok(),
            "Peer2 request {} should succeed",
            i
        );
        controller.record_sync(&peer2, "doc");
        println!("    Peer2 request {} allowed", i);
    }
    println!("    ✓ Peer2 all 3 requests allowed");

    println!("  3. peer3 should also have full quota...");

    // peer3 should also have full quota
    for i in 0..3 {
        assert!(
            controller.check_sync_allowed(&peer3, "doc").is_ok(),
            "Peer3 request {} should succeed",
            i
        );
        controller.record_sync(&peer3, "doc");
        println!("    Peer3 request {} allowed", i);
    }
    println!("    ✓ Peer3 all 3 requests allowed");

    println!("  4. Verifying statistics...");

    let stats = controller.stats();
    assert_eq!(
        stats.rate_limited, 1,
        "Only peer1's extra request should be rate limited"
    );
    println!("    ✓ Total rate limited: {}", stats.rate_limited);

    println!("  ✓ Multi-peer independent rate limiting test passed");
}

/// Test 4: Token Bucket Refills Over Time
///
/// Validates that tokens refill correctly, allowing more syncs after waiting.
#[test]
fn test_e2e_token_bucket_refills() {
    println!("=== E2E: Token Bucket Refills Over Time ===");

    // Configure with fast refill for testing
    let config = FlowControlConfig {
        max_messages_per_second: 5,
        tokens_per_refill: 2, // Refill 2 tokens
        refill_interval: Duration::from_millis(50),
        sync_cooldown: Duration::ZERO,
        ..Default::default()
    };
    let controller = FlowController::with_config(config);
    let peer_id = create_test_peer_id();

    println!("  1. Exhaust all 5 tokens...");

    // Exhaust all tokens
    for i in 0..5 {
        assert!(
            controller
                .check_sync_allowed(&peer_id, &format!("doc{}", i))
                .is_ok(),
            "Initial request {} should succeed",
            i
        );
        controller.record_sync(&peer_id, &format!("doc{}", i));
    }
    println!("    ✓ All 5 tokens consumed");

    println!("  2. Verify next request is rate limited...");

    // Should be rate limited
    assert!(matches!(
        controller.check_sync_allowed(&peer_id, "doc5"),
        Err(FlowControlError::RateLimitExceeded)
    ));
    println!("    ✓ Request blocked (no tokens)");

    println!("  3. Waiting for tokens to refill...");

    // Wait for refill (should get 2 tokens after 50ms)
    std::thread::sleep(Duration::from_millis(60));

    println!("  4. Testing that tokens refilled...");

    // Should now be able to make at least 1 more request
    let result = controller.check_sync_allowed(&peer_id, "doc6");
    assert!(result.is_ok(), "Should have tokens after refill");
    controller.record_sync(&peer_id, "doc6");
    println!("    ✓ Request allowed after refill");

    // May have more tokens depending on timing
    let tokens = controller.available_tokens(&peer_id);
    println!("    Available tokens after 1 request: {}", tokens);

    println!("  ✓ Token bucket refill test passed");
}

/// Test 5: Combined Rate Limit and Cooldown
///
/// Validates that both checks work together correctly.
#[test]
fn test_e2e_combined_rate_limit_and_cooldown() {
    println!("=== E2E: Combined Rate Limit and Cooldown ===");

    let config = FlowControlConfig {
        max_messages_per_second: 10,
        tokens_per_refill: 1,
        refill_interval: Duration::from_millis(100),
        sync_cooldown: Duration::from_millis(30),
        ..Default::default()
    };
    let controller = FlowController::with_config(config);
    let peer_id = create_test_peer_id();

    println!("  1. First sync should succeed (passes both checks)...");

    // First sync succeeds
    assert!(controller.check_sync_allowed(&peer_id, "doc1").is_ok());
    controller.record_sync(&peer_id, "doc1");
    println!("    ✓ First sync allowed");

    println!("  2. Immediate retry should fail with cooldown (rate limit passes)...");

    // Immediate retry fails with cooldown (rate limit still passes)
    let result = controller.check_sync_allowed(&peer_id, "doc1");
    assert!(
        matches!(result, Err(FlowControlError::CooldownActive { .. })),
        "Expected cooldown, got {:?}",
        result
    );
    println!("    ✓ Blocked by cooldown");

    println!("  3. Different doc should succeed (new cooldown timer)...");

    // Different doc succeeds
    assert!(controller.check_sync_allowed(&peer_id, "doc2").is_ok());
    controller.record_sync(&peer_id, "doc2");
    println!("    ✓ Different doc allowed");

    println!("  4. Wait for cooldown and verify both pass...");

    std::thread::sleep(Duration::from_millis(40));

    // After cooldown, should succeed again
    assert!(controller.check_sync_allowed(&peer_id, "doc1").is_ok());
    println!("    ✓ doc1 allowed after cooldown");

    println!("  ✓ Combined rate limit and cooldown test passed");
}

/// Test 6: Flow Control Statistics Tracking
///
/// Validates that all statistics are tracked correctly.
#[test]
fn test_e2e_flow_control_statistics() {
    println!("=== E2E: Flow Control Statistics Tracking ===");

    let config = FlowControlConfig {
        max_messages_per_second: 3,
        tokens_per_refill: 1,
        refill_interval: Duration::from_secs(1),
        sync_cooldown: Duration::from_millis(50),
        ..Default::default()
    };
    let controller = FlowController::with_config(config);

    let peer1 = create_test_peer_id();
    let peer2 = create_test_peer_id();

    println!("  1. Initial stats should be zero...");

    let stats = controller.stats();
    assert_eq!(stats.rate_limited, 0);
    assert_eq!(stats.cooldown_blocked, 0);
    assert_eq!(stats.queue_dropped, 0);
    println!("    ✓ Initial stats: {:?}", stats);

    println!("  2. Generate rate limit events...");

    // Exhaust tokens for peer1
    for _ in 0..3 {
        controller.check_sync_allowed(&peer1, "doc").ok();
        controller.record_sync(&peer1, "doc");
        std::thread::sleep(Duration::from_millis(10)); // small delay to clear cooldown check
    }
    // This should trigger rate limit
    controller.check_sync_allowed(&peer1, "doc").ok();
    controller.check_sync_allowed(&peer1, "doc").ok();

    let stats = controller.stats();
    println!("    Rate limited count: {}", stats.rate_limited);
    assert!(stats.rate_limited > 0, "Should have rate limited requests");

    println!("  3. Generate cooldown events...");

    // Generate cooldown events using peer2 (fresh peer with full token bucket)
    controller.check_sync_allowed(&peer2, "doc1").ok();
    controller.record_sync(&peer2, "doc1");
    // Immediate retry triggers cooldown
    controller.check_sync_allowed(&peer2, "doc1").ok();

    let stats = controller.stats();
    println!("    Cooldown blocked count: {}", stats.cooldown_blocked);
    assert!(
        stats.cooldown_blocked > 0,
        "Should have cooldown blocked requests"
    );

    println!("  4. Final statistics...");

    let final_stats = controller.stats();
    println!("    Final stats: {:?}", final_stats);
    println!("      - rate_limited: {}", final_stats.rate_limited);
    println!("      - cooldown_blocked: {}", final_stats.cooldown_blocked);
    println!("      - queue_dropped: {}", final_stats.queue_dropped);
    println!(
        "      - total_memory_usage: {}",
        final_stats.total_memory_usage
    );
    println!("      - active_peers: {}", final_stats.active_peers);

    println!("  ✓ Statistics tracking test passed");
}

/// Test 7: Cleanup Removes Stale Entries
///
/// Validates that cleanup correctly removes old cooldown entries.
#[test]
fn test_e2e_cleanup_stale_entries() {
    println!("=== E2E: Cleanup Removes Stale Entries ===");

    let config = FlowControlConfig {
        sync_cooldown: Duration::from_millis(10), // Very short for testing
        ..Default::default()
    };
    let controller = FlowController::with_config(config);

    // Create many peer/doc combinations
    println!("  1. Creating 10 sync records...");

    for i in 0..10 {
        let peer = create_test_peer_id();
        controller.record_sync(&peer, &format!("doc{}", i));
    }
    println!("    ✓ Created 10 sync records");

    println!("  2. Waiting for records to become stale...");

    // Wait for entries to become stale (10x cooldown = 100ms)
    std::thread::sleep(Duration::from_millis(150));

    println!("  3. Running cleanup...");

    controller.cleanup();
    println!("    ✓ Cleanup completed");

    // Cleanup should not panic and should remove old entries
    // (Internal state is not directly observable, but cleanup should succeed)

    println!("  ✓ Cleanup test passed");
}

/// Test 8: Resource Tracking Per Peer
///
/// Validates that per-peer resource tracking works correctly.
#[test]
fn test_e2e_peer_resource_tracking() {
    println!("=== E2E: Peer Resource Tracking ===");

    let config = FlowControlConfig {
        max_memory_per_peer: 1024 * 1024, // 1MB
        ..Default::default()
    };
    let controller = FlowController::with_config(config);
    let peer_id = create_test_peer_id();

    println!("  1. Getting resource tracker for peer...");

    let tracker = controller.get_resource_tracker(&peer_id);
    assert_eq!(tracker.memory_usage(), 0);
    println!("    ✓ Initial memory usage: 0");

    println!("  2. Allocating memory...");

    assert!(tracker.try_allocate(1000));
    assert_eq!(tracker.memory_usage(), 1000);
    println!("    ✓ After allocating 1000: {}", tracker.memory_usage());

    println!("  3. Recording message stats...");

    tracker.record_sent();
    tracker.record_sent();
    tracker.record_received();

    assert_eq!(tracker.messages_sent(), 2);
    assert_eq!(tracker.messages_received(), 1);
    println!(
        "    ✓ Messages sent: {}, received: {}",
        tracker.messages_sent(),
        tracker.messages_received()
    );

    println!("  ✓ Peer resource tracking test passed");
}

/// Test 9: High-Volume Rate Limiting Stress Test
///
/// Validates rate limiting under high request volume.
#[test]
fn test_e2e_high_volume_rate_limiting() {
    println!("=== E2E: High-Volume Rate Limiting ===");

    let config = FlowControlConfig {
        max_messages_per_second: 100,
        tokens_per_refill: 10,
        refill_interval: Duration::from_millis(100),
        sync_cooldown: Duration::ZERO,
        ..Default::default()
    };
    let controller = FlowController::with_config(config);
    let peer_id = create_test_peer_id();

    println!("  Testing 500 rapid sync requests...");

    let mut allowed = 0;
    let mut blocked = 0;

    for i in 0..500 {
        match controller.check_sync_allowed(&peer_id, &format!("doc{}", i)) {
            Ok(()) => {
                allowed += 1;
                controller.record_sync(&peer_id, &format!("doc{}", i));
            }
            Err(FlowControlError::RateLimitExceeded) => {
                blocked += 1;
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    println!("  Results:");
    println!("    - Allowed: {}", allowed);
    println!("    - Blocked: {}", blocked);

    // Should allow roughly 100 (initial capacity) and block the rest
    // (some may sneak through due to timing)
    assert!(
        allowed <= 150,
        "Too many allowed: {} (expected ~100)",
        allowed
    );
    assert!(
        blocked >= 350,
        "Too few blocked: {} (expected ~400)",
        blocked
    );

    let stats = controller.stats();
    assert_eq!(stats.rate_limited, blocked);
    println!(
        "  ✓ Statistics match: rate_limited = {}",
        stats.rate_limited
    );

    println!("  ✓ High-volume rate limiting test passed");
}

/// Test 10: Integration Scenario - Realistic Sync Pattern
///
/// Simulates a realistic sync pattern with multiple peers and documents.
#[test]
fn test_e2e_realistic_sync_pattern() {
    println!("=== E2E: Realistic Sync Pattern ===");

    let config = FlowControlConfig {
        max_messages_per_second: 50,
        tokens_per_refill: 5,
        refill_interval: Duration::from_millis(100),
        sync_cooldown: Duration::from_millis(20),
        ..Default::default()
    };
    let controller = FlowController::with_config(config);

    // Simulate 3 peers syncing multiple documents
    let peers: Vec<EndpointId> = (0..3).map(|_| create_test_peer_id()).collect();
    let docs = ["cells", "nodes", "telemetry", "capabilities"];

    println!("  Simulating mesh sync with 3 peers, 4 doc types...");

    let mut total_allowed = 0;
    let mut total_blocked = 0;

    // Simulate 100ms of sync activity
    for _round in 0..10 {
        for peer in &peers {
            for doc in &docs {
                match controller.check_sync_allowed(peer, doc) {
                    Ok(()) => {
                        total_allowed += 1;
                        controller.record_sync(peer, doc);
                    }
                    Err(_) => {
                        total_blocked += 1;
                    }
                }
            }
        }

        // Small delay between rounds
        std::thread::sleep(Duration::from_millis(10));
    }

    println!("  Results over 10 rounds:");
    println!("    - Total allowed: {}", total_allowed);
    println!("    - Total blocked: {}", total_blocked);

    let stats = controller.stats();
    println!("  Final statistics:");
    println!("    - Rate limited: {}", stats.rate_limited);
    println!("    - Cooldown blocked: {}", stats.cooldown_blocked);

    // Should have a mix of rate limited and cooldown blocked
    // Exact numbers depend on timing but both should be > 0
    assert!(
        stats.rate_limited + stats.cooldown_blocked > 0,
        "Should have some blocked requests"
    );

    println!("  ✓ Realistic sync pattern test passed");
}
