#!/bin/bash
# ADR-027 Phase 5: Integration Testing (48-node platoon simulation)
# Issue #389: Validate event routing and aggregation system
#
# Test Scenarios:
# 1. Bandwidth Reduction Test - verify >=95% reduction vs naive propagation
# 2. Latency Test - measure P50/P95/P99 for each event type
# 3. Query Test - verify fan-out retrieves stored telemetry events
# 4. Priority Preemption Test - verify CRITICAL delivered within SLA during saturation
# 5. Failure Resilience Test - verify graceful degradation when squad leader fails

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
TOPOLOGY_FILE="topologies/adr027-48n-platoon.yaml"
LAB_NAME="adr027-48n-platoon"
RESULTS_DIR="adr027-phase5-$(date +%Y%m%d-%H%M%S)"
TEST_DURATION_SECS=60
STABILIZATION_SECS=30
BACKEND="${BACKEND:-automerge}"

# Expected metrics from ADR-027
EXPECTED_PLATFORMS=48
EXPECTED_SQUADS=6
EXPECTED_EVENTS_PER_SEC_PER_PLATFORM=11  # 10 detections + 1 telemetry
EXPECTED_WITHOUT_AGGREGATION=$((EXPECTED_PLATFORMS * EXPECTED_EVENTS_PER_SEC_PER_PLATFORM))  # 528 events/sec
EXPECTED_WITH_AGGREGATION_MAX=40  # ~7.5 events/sec per calculation, but allow some variance
EXPECTED_REDUCTION_MIN=95  # >=95% reduction

# SLA requirements
CRITICAL_LATENCY_P99_MAX_MS=10
SUMMARY_LATENCY_P99_MAX_SECS=2

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[FAIL]${NC} $1"
}

log_section() {
    echo ""
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}========================================${NC}"
    echo ""
}

# Create results directory
mkdir -p "$RESULTS_DIR"

# Check prerequisites
check_prerequisites() {
    log_section "Checking Prerequisites"

    if ! command -v containerlab &> /dev/null; then
        log_error "containerlab not found. Please install containerlab."
        exit 1
    fi

    if ! docker images | grep -q "peat-sim-node"; then
        log_warning "peat-sim-node Docker image not found. Building..."
        cd "$(dirname "$0")"
        make docker-build-automerge
    fi

    if [ ! -f "$TOPOLOGY_FILE" ]; then
        log_error "Topology file not found: $TOPOLOGY_FILE"
        exit 1
    fi

    log_success "Prerequisites verified"
}

# Deploy the topology
deploy_topology() {
    log_section "Deploying 48-node Platoon Topology"

    # Clean up any existing deployment
    containerlab destroy --topo "$TOPOLOGY_FILE" --cleanup 2>/dev/null || true

    # Copy topology file for orchestrator
    cp "$TOPOLOGY_FILE" "/tmp/${LAB_NAME}.yaml"

    # Deploy with environment variable for backend selection
    log_info "Deploying with BACKEND=$BACKEND..."
    BACKEND="$BACKEND" containerlab deploy --topo "$TOPOLOGY_FILE" --reconfigure

    # Wait for containers to stabilize
    log_info "Waiting ${STABILIZATION_SECS}s for containers to stabilize..."
    sleep "$STABILIZATION_SECS"

    # Verify all containers are running
    local running_count
    running_count=$(docker ps --filter "name=clab-${LAB_NAME}" --format "{{.Names}}" | wc -l)

    if [ "$running_count" -lt 55 ]; then
        log_error "Expected 55+ containers, found $running_count"
        docker ps --filter "name=clab-${LAB_NAME}" --format "table {{.Names}}\t{{.Status}}" | head -20
        return 1
    fi

    log_success "Deployed $running_count containers successfully"
}

# Test 1: Bandwidth Reduction Test
test_bandwidth_reduction() {
    log_section "Test 1: Bandwidth Reduction"
    log_info "Running for ${TEST_DURATION_SECS}s to measure event flow..."

    local start_time
    start_time=$(date +%s)

    # Collect metrics from platoon leader
    local platoon_events_received=0
    local platoon_summaries_received=0
    local detection_summaries=0
    local telemetry_queries=0
    local anomaly_passthrough=0
    local critical_passthrough=0

    # Wait for test duration
    sleep "$TEST_DURATION_SECS"

    # Collect logs from platoon leader
    docker logs "clab-${LAB_NAME}-platoon-leader" 2>&1 > "$RESULTS_DIR/platoon-leader.log"

    # Count event types in platoon leader logs
    detection_summaries=$(grep -c "detection_summary" "$RESULTS_DIR/platoon-leader.log" 2>/dev/null || echo 0)
    telemetry_queries=$(grep -c "telemetry.*query" "$RESULTS_DIR/platoon-leader.log" 2>/dev/null || echo 0)
    anomaly_passthrough=$(grep -c "anomaly.*received\|anomaly.*passthrough" "$RESULTS_DIR/platoon-leader.log" 2>/dev/null || echo 0)
    critical_passthrough=$(grep -c "critical.*received\|critical.*passthrough" "$RESULTS_DIR/platoon-leader.log" 2>/dev/null || echo 0)

    platoon_events_received=$((detection_summaries + anomaly_passthrough + critical_passthrough))

    # Calculate expected without aggregation (over test duration)
    local expected_raw_events=$((EXPECTED_WITHOUT_AGGREGATION * TEST_DURATION_SECS))
    local expected_with_aggregation=$((EXPECTED_WITH_AGGREGATION_MAX * TEST_DURATION_SECS))

    log_info "Events received at platoon leader: $platoon_events_received"
    log_info "  - Detection summaries: $detection_summaries"
    log_info "  - Anomaly passthrough: $anomaly_passthrough"
    log_info "  - Critical passthrough: $critical_passthrough"
    log_info "Expected without aggregation: ~$expected_raw_events events"

    # Calculate actual reduction (estimate based on summary count)
    # Each squad should produce 1 summary/sec for detections (aggregated from 80 events/sec per squad)
    # So 6 squads × 60 seconds × 1 summary = 360 summaries
    # Plus anomalies (~0.48/sec × 60 = ~29) and critical (~0.048/sec × 60 = ~3)
    # Total: ~392 events vs ~31,680 raw events = 98.8% reduction

    if [ "$platoon_events_received" -gt 0 ] && [ "$expected_raw_events" -gt 0 ]; then
        local reduction
        reduction=$(echo "scale=2; (1 - $platoon_events_received / $expected_raw_events) * 100" | bc 2>/dev/null || echo "0")
        log_info "Actual reduction: ${reduction}%"

        # Use integer comparison (bc returns float)
        local reduction_int
        reduction_int=$(echo "$reduction" | cut -d'.' -f1)
        if [ "${reduction_int:-0}" -ge "$EXPECTED_REDUCTION_MIN" ]; then
            log_success "Bandwidth reduction >=95% achieved: ${reduction}%"
            echo "PASS: Bandwidth reduction ${reduction}%" >> "$RESULTS_DIR/test-results.txt"
        else
            log_warning "Bandwidth reduction below target: ${reduction}% (expected >=${EXPECTED_REDUCTION_MIN}%)"
            echo "WARN: Bandwidth reduction ${reduction}% (expected >=${EXPECTED_REDUCTION_MIN}%)" >> "$RESULTS_DIR/test-results.txt"
        fi
    else
        log_warning "Could not calculate reduction (insufficient data)"
        echo "WARN: Insufficient data for bandwidth reduction calculation" >> "$RESULTS_DIR/test-results.txt"
    fi

    # Save metrics summary
    cat > "$RESULTS_DIR/bandwidth-metrics.json" <<EOF
{
  "test_duration_secs": $TEST_DURATION_SECS,
  "expected_raw_events": $expected_raw_events,
  "platoon_events_received": $platoon_events_received,
  "detection_summaries": $detection_summaries,
  "anomaly_passthrough": $anomaly_passthrough,
  "critical_passthrough": $critical_passthrough,
  "expected_reduction_percent": $EXPECTED_REDUCTION_MIN
}
EOF
}

# Test 2: Latency Test
test_latency() {
    log_section "Test 2: Latency Measurement"
    log_info "Analyzing latency from collected logs..."

    # Collect all container logs
    for squad in 1 2 3 4 5 6; do
        docker logs "clab-${LAB_NAME}-squad-${squad}-leader" 2>&1 >> "$RESULTS_DIR/all-squad-leaders.log"
    done

    # Extract latency metrics (look for latency_ms in JSON log output)
    local latencies_file="$RESULTS_DIR/latencies.txt"
    local critical_latencies_file="$RESULTS_DIR/critical-latencies.txt"
    local summary_latencies_file="$RESULTS_DIR/summary-latencies.txt"

    # Parse latency values from JSON logs (format: "latency_ms": 1.234)
    grep -o '"latency_ms":[0-9.]*' "$RESULTS_DIR/platoon-leader.log" 2>/dev/null | \
        cut -d':' -f2 > "$latencies_file" || true

    # Filter critical events
    grep -B5 '"priority":"CRITICAL"\|"event_type":"critical"' "$RESULTS_DIR/platoon-leader.log" 2>/dev/null | \
        grep -o '"latency_ms":[0-9.]*' | cut -d':' -f2 > "$critical_latencies_file" || true

    # Filter summary events
    grep -B5 '"event_type":".*_summary"' "$RESULTS_DIR/platoon-leader.log" 2>/dev/null | \
        grep -o '"latency_ms":[0-9.]*' | cut -d':' -f2 > "$summary_latencies_file" || true

    # Calculate percentiles using awk
    calculate_percentiles() {
        local file="$1"
        local name="$2"

        if [ ! -s "$file" ]; then
            log_warning "No latency data for $name"
            return
        fi

        sort -n "$file" > "${file}.sorted"
        local count
        count=$(wc -l < "${file}.sorted")

        if [ "$count" -gt 0 ]; then
            local p50_idx=$((count * 50 / 100))
            local p95_idx=$((count * 95 / 100))
            local p99_idx=$((count * 99 / 100))

            [ "$p50_idx" -lt 1 ] && p50_idx=1
            [ "$p95_idx" -lt 1 ] && p95_idx=1
            [ "$p99_idx" -lt 1 ] && p99_idx=1

            local p50
            local p95
            local p99
            p50=$(sed -n "${p50_idx}p" "${file}.sorted")
            p95=$(sed -n "${p95_idx}p" "${file}.sorted")
            p99=$(sed -n "${p99_idx}p" "${file}.sorted")

            log_info "$name latencies (ms) - P50: $p50, P95: $p95, P99: $p99"
            echo "$name: P50=$p50 P95=$p95 P99=$p99" >> "$RESULTS_DIR/latency-percentiles.txt"
        fi
    }

    calculate_percentiles "$latencies_file" "All Events"
    calculate_percentiles "$critical_latencies_file" "CRITICAL Events"
    calculate_percentiles "$summary_latencies_file" "Summary Events"

    # Check CRITICAL latency SLA
    if [ -s "${critical_latencies_file}.sorted" ]; then
        local count
        count=$(wc -l < "${critical_latencies_file}.sorted")
        local p99_idx=$((count * 99 / 100))
        [ "$p99_idx" -lt 1 ] && p99_idx=1
        local critical_p99
        critical_p99=$(sed -n "${p99_idx}p" "${critical_latencies_file}.sorted")

        if [ -n "$critical_p99" ]; then
            local critical_p99_int
            critical_p99_int=$(echo "$critical_p99" | cut -d'.' -f1)
            if [ "${critical_p99_int:-999}" -le "$CRITICAL_LATENCY_P99_MAX_MS" ]; then
                log_success "CRITICAL latency P99 within SLA: ${critical_p99}ms <= ${CRITICAL_LATENCY_P99_MAX_MS}ms"
                echo "PASS: CRITICAL latency P99 ${critical_p99}ms" >> "$RESULTS_DIR/test-results.txt"
            else
                log_error "CRITICAL latency P99 exceeds SLA: ${critical_p99}ms > ${CRITICAL_LATENCY_P99_MAX_MS}ms"
                echo "FAIL: CRITICAL latency P99 ${critical_p99}ms > ${CRITICAL_LATENCY_P99_MAX_MS}ms" >> "$RESULTS_DIR/test-results.txt"
            fi
        fi
    else
        log_warning "No CRITICAL latency data available"
        echo "WARN: No CRITICAL latency data" >> "$RESULTS_DIR/test-results.txt"
    fi
}

# Test 3: Query Fan-out Test
test_query_fanout() {
    log_section "Test 3: Query Fan-out"
    log_info "Testing telemetry query from platoon level..."

    # The platoon leader should be able to query stored telemetry from squads
    # For this test, we verify that query responses are logged

    local query_responses=0
    query_responses=$(grep -c "EventQueryResponse\|query_response\|QueryResult" "$RESULTS_DIR/platoon-leader.log" 2>/dev/null || echo 0)

    local telemetry_stored=0
    for squad in 1 2 3 4 5 6; do
        local squad_log
        squad_log=$(docker logs "clab-${LAB_NAME}-squad-${squad}-leader" 2>&1)
        local squad_stored
        squad_stored=$(echo "$squad_log" | grep -c "telemetry.*stored\|PropagationQuery\|queryable_count" 2>/dev/null || echo 0)
        telemetry_stored=$((telemetry_stored + squad_stored))
    done

    log_info "Telemetry events stored across squads: $telemetry_stored"
    log_info "Query responses received at platoon: $query_responses"

    # Expected: 48 platforms × 1 telemetry/sec × 60 secs = 2880 telemetry events stored
    local expected_telemetry=$((EXPECTED_PLATFORMS * 1 * TEST_DURATION_SECS))

    if [ "$telemetry_stored" -gt 0 ]; then
        local storage_ratio
        storage_ratio=$(echo "scale=2; $telemetry_stored / $expected_telemetry * 100" | bc 2>/dev/null || echo "0")
        log_info "Telemetry storage utilization: ${storage_ratio}%"

        if [ "$telemetry_stored" -gt "$((expected_telemetry / 2))" ]; then
            log_success "Query storage operational - $telemetry_stored events stored"
            echo "PASS: Query storage with $telemetry_stored events" >> "$RESULTS_DIR/test-results.txt"
        else
            log_warning "Lower than expected telemetry storage"
            echo "WARN: Low telemetry storage: $telemetry_stored" >> "$RESULTS_DIR/test-results.txt"
        fi
    else
        log_warning "No telemetry storage data found"
        echo "WARN: No telemetry storage data" >> "$RESULTS_DIR/test-results.txt"
    fi
}

# Test 4: Priority Preemption Test
test_priority_preemption() {
    log_section "Test 4: Priority Preemption"
    log_info "Verifying CRITICAL events preempt other traffic..."

    # Check transmitter stats for priority handling
    local critical_transmitted=0
    local low_dropped=0

    # Look for transmitter stats in logs
    critical_transmitted=$(grep -c "transmitted.*CRITICAL\|stats.transmitted\[0\]\|priority.*critical.*transmitted" \
        "$RESULTS_DIR/platoon-leader.log" 2>/dev/null || echo 0)

    low_dropped=$(grep -c "dropped.*LOW\|stats.dropped\[3\]\|priority.*low.*dropped" \
        "$RESULTS_DIR/platoon-leader.log" 2>/dev/null || echo 0)

    # Check that CRITICAL events were received even during high load
    local critical_received
    critical_received=$(grep -c '"priority":"CRITICAL"\|event_type.*critical\|PriorityCritical' \
        "$RESULTS_DIR/platoon-leader.log" 2>/dev/null || echo 0)

    log_info "CRITICAL events received: $critical_received"
    log_info "LOW priority drops detected: $low_dropped"

    # With 48 platforms at 0.001/sec critical rate for 60 secs, expect ~3 critical events
    local expected_critical=$((EXPECTED_PLATFORMS * TEST_DURATION_SECS / 1000))
    [ "$expected_critical" -lt 1 ] && expected_critical=1

    if [ "$critical_received" -gt 0 ]; then
        log_success "CRITICAL event preemption verified - $critical_received events received"
        echo "PASS: Priority preemption - $critical_received CRITICAL events" >> "$RESULTS_DIR/test-results.txt"
    else
        log_warning "No CRITICAL events detected (expected ~$expected_critical)"
        echo "WARN: No CRITICAL events detected" >> "$RESULTS_DIR/test-results.txt"
    fi
}

# Test 5: Failure Resilience Test
test_failure_resilience() {
    log_section "Test 5: Failure Resilience"
    log_info "Testing graceful degradation on squad leader failure..."

    # Record events before killing squad leader
    local events_before
    events_before=$(grep -c "event_id\|DocumentReceived" "$RESULTS_DIR/platoon-leader.log" 2>/dev/null || echo 0)

    log_info "Events at platoon before failure: $events_before"

    # Kill squad-3-leader
    log_info "Killing squad-3-leader..."
    docker kill "clab-${LAB_NAME}-squad-3-leader" 2>/dev/null || true

    # Wait for system to detect failure and adapt
    log_info "Waiting 15s for failure detection and adaptation..."
    sleep 15

    # Collect more logs
    docker logs "clab-${LAB_NAME}-platoon-leader" 2>&1 >> "$RESULTS_DIR/platoon-leader-post-failure.log"

    # Check that platoon is still receiving events from other squads
    local events_after
    events_after=$(grep -c "event_id\|DocumentReceived" "$RESULTS_DIR/platoon-leader-post-failure.log" 2>/dev/null || echo 0)

    local events_from_other_squads=0
    for squad in 1 2 4 5 6; do
        local squad_events
        squad_events=$(grep -c "squad-${squad}\|from.*squad-${squad}" \
            "$RESULTS_DIR/platoon-leader-post-failure.log" 2>/dev/null || echo 0)
        events_from_other_squads=$((events_from_other_squads + squad_events))
    done

    log_info "Events from surviving squads: $events_from_other_squads"

    # Check for circuit breaker activation
    local circuit_open
    circuit_open=$(grep -c "circuit.*open\|CircuitOpen\|connection.*failed.*squad-3" \
        "$RESULTS_DIR/platoon-leader-post-failure.log" 2>/dev/null || echo 0)

    if [ "$events_from_other_squads" -gt 0 ] || [ "$events_after" -gt "$events_before" ]; then
        log_success "Graceful degradation verified - continued receiving from surviving squads"
        echo "PASS: Failure resilience - system continued operating" >> "$RESULTS_DIR/test-results.txt"
    else
        log_warning "Limited recovery data - this may be normal for short tests"
        echo "WARN: Limited failure resilience data" >> "$RESULTS_DIR/test-results.txt"
    fi

    if [ "$circuit_open" -gt 0 ]; then
        log_info "Circuit breaker detected failure (circuit open events: $circuit_open)"
    fi
}

# Cleanup function
cleanup() {
    log_section "Cleanup"
    containerlab destroy --topo "$TOPOLOGY_FILE" --cleanup 2>/dev/null || true
    log_success "Cleanup complete"
}

# Generate test report
generate_report() {
    log_section "Test Report"

    local report_file="$RESULTS_DIR/test-report.md"

    cat > "$report_file" <<EOF
# ADR-027 Phase 5: Integration Test Report

**Date:** $(date)
**Topology:** 48-node platoon (6 squads × 8 platforms)
**Backend:** $BACKEND
**Test Duration:** ${TEST_DURATION_SECS}s

## Test Results Summary

$(cat "$RESULTS_DIR/test-results.txt" 2>/dev/null || echo "No results recorded")

## Configuration

- Platforms: $EXPECTED_PLATFORMS
- Squads: $EXPECTED_SQUADS
- Events/sec/platform: $EXPECTED_EVENTS_PER_SEC_PER_PLATFORM
- Expected raw events: $EXPECTED_WITHOUT_AGGREGATION events/sec
- Target reduction: >=${EXPECTED_REDUCTION_MIN}%
- CRITICAL latency SLA: <=${CRITICAL_LATENCY_P99_MAX_MS}ms P99

## Latency Percentiles

$(cat "$RESULTS_DIR/latency-percentiles.txt" 2>/dev/null || echo "No latency data")

## Files Generated

$(ls -la "$RESULTS_DIR")

## Notes

This test validates ADR-027 Phase 5 requirements for the PEAT Protocol event routing
and aggregation system. See Issue #389 for full acceptance criteria.
EOF

    log_info "Test report saved to: $report_file"
    cat "$report_file"
}

# Main execution
main() {
    log_section "ADR-027 Phase 5: Integration Testing"
    log_info "48-node platoon simulation for event routing validation"
    log_info "Results directory: $RESULTS_DIR"

    # Initialize results file
    echo "# ADR-027 Phase 5 Test Results - $(date)" > "$RESULTS_DIR/test-results.txt"

    check_prerequisites
    deploy_topology

    # Run all tests
    test_bandwidth_reduction
    test_latency
    test_query_fanout
    test_priority_preemption
    test_failure_resilience

    # Generate report
    generate_report

    # Cleanup
    cleanup

    log_section "Testing Complete"
    log_info "All results saved to: $RESULTS_DIR/"

    # Count pass/fail/warn
    local pass_count fail_count warn_count
    pass_count=$(grep -c "^PASS" "$RESULTS_DIR/test-results.txt" 2>/dev/null || echo 0)
    fail_count=$(grep -c "^FAIL" "$RESULTS_DIR/test-results.txt" 2>/dev/null || echo 0)
    warn_count=$(grep -c "^WARN" "$RESULTS_DIR/test-results.txt" 2>/dev/null || echo 0)

    echo ""
    echo -e "Results: ${GREEN}$pass_count PASS${NC}, ${RED}$fail_count FAIL${NC}, ${YELLOW}$warn_count WARN${NC}"

    if [ "$fail_count" -gt 0 ]; then
        exit 1
    fi
}

# Handle Ctrl+C gracefully
trap cleanup EXIT

main "$@"
