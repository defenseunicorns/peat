#!/bin/bash
# run-lab5-chaos-suite.sh - Run complete Lab 5 chaos engineering test suite
# Requires: Lab 4 deployment running (make lab4-96 or lab4-384)
#
# Integrates with Lab 5 metrics infrastructure:
#   - chaos-inject.sh: Structured CHAOS_EVENT logging
#   - measure-recovery.sh: Recovery time measurement
#   - verify-consistency.sh: Data integrity verification
#   - lab5-report.sh: Unified results report

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS_BASE="/work/peat-sim-results"
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_DIR="${RESULTS_BASE}/lab5-chaos-${TIMESTAMP}"

# Metrics files
CHAOS_EVENTS="/tmp/lab5-chaos-events.jsonl"
RECOVERY_RESULTS="/tmp/lab5-recovery-results.jsonl"
CONSISTENCY_RESULTS="/tmp/lab5-consistency-results.jsonl"

log_info() { echo "[INFO] $(date +%H:%M:%S) $*"; }
log_success() { echo "[SUCCESS] $(date +%H:%M:%S) $*"; }
log_event() { echo "[EVENT] $(date +%H:%M:%S.%3N) $*"; }

# Clear old results files
clear_results_files() {
    log_info "Clearing previous results files..."
    rm -f "$CHAOS_EVENTS" "$RECOVERY_RESULTS" "$CONSISTENCY_RESULTS"
}

# Helper to emit chaos event (same format as chaos-inject.sh)
emit_chaos_event() {
    local EVENT_TYPE="$1"
    local CHAOS_MODE="$2"
    local DETAILS="${3:-{}}"
    local TIMESTAMP_MS=$(date +%s%3N)

    local EVENT_JSON="{\"type\":\"${EVENT_TYPE}\",\"mode\":\"${CHAOS_MODE}\",\"timestamp_ms\":${TIMESTAMP_MS},\"details\":${DETAILS}}"
    echo "CHAOS_EVENT: $EVENT_JSON"
    echo "$EVENT_JSON" >> "$CHAOS_EVENTS"
}

# Check prerequisites
check_prereqs() {
    local CONTAINERS=$(docker ps --filter "name=clab-lab4" -q | wc -l)
    if [ "$CONTAINERS" -eq 0 ]; then
        echo "ERROR: No Lab 4 deployment found."
        echo "Run 'make lab4-96' or 'make lab4-384' first."
        exit 1
    fi
    log_info "Found $CONTAINERS Lab 4 containers"
}

# Collect baseline metrics before chaos
collect_baseline() {
    log_info "Collecting baseline metrics..."
    mkdir -p "${RESULTS_DIR}/baseline"

    # Container count
    docker ps --filter "name=clab-lab4" -q | wc -l > "${RESULTS_DIR}/baseline/container_count.txt"

    # Sample sync latency from commander
    local COMMANDER=$(docker ps --format '{{.Names}}' | grep "commander" | head -1)
    if [ -n "$COMMANDER" ]; then
        docker logs "$COMMANDER" 2>&1 | grep "AggregationCompleted" | tail -20 > "${RESULTS_DIR}/baseline/aggregation_sample.txt" || true
    fi

    log_success "Baseline collected"
}

# Remove chaos and collect post-chaos metrics
recover_and_collect() {
    local SCENARIO=$1
    local TARGET_MS=${2:-60000}
    log_info "Recovering from $SCENARIO..."

    # Use chaos-inject.sh to properly log the recover event
    "${SCRIPT_DIR}/chaos-inject.sh" recover

    # Wait for recovery
    sleep 30

    # Collect recovery metrics using new infrastructure
    mkdir -p "${RESULTS_DIR}/${SCENARIO}"

    # Measure recovery time
    log_info "Measuring recovery time..."
    "${SCRIPT_DIR}/measure-recovery.sh" --target "$TARGET_MS" 2>&1 | tee "${RESULTS_DIR}/${SCENARIO}/recovery_time.txt" || true

    # Verify data consistency
    log_info "Verifying data consistency..."
    "${SCRIPT_DIR}/verify-consistency.sh" 2>&1 | tee "${RESULTS_DIR}/${SCENARIO}/consistency.txt" || true

    # Collect commander logs
    local COMMANDER=$(docker ps --format '{{.Names}}' | grep "commander" | head -1)
    if [ -n "$COMMANDER" ]; then
        docker logs "$COMMANDER" 2>&1 | grep "AggregationCompleted" | tail -20 > "${RESULTS_DIR}/${SCENARIO}/recovery_aggregation.txt" || true
    fi

    log_success "Recovery complete for $SCENARIO"
}

# Test: Packet loss sweep
test_packet_loss() {
    log_info "=== Lab 5a: Packet Loss Sweep ==="
    mkdir -p "${RESULTS_DIR}/packet-loss"

    for LOSS in 1 5 10 20; do
        log_event "Testing ${LOSS}% packet loss..."

        # Apply packet loss
        for container in $(docker ps --filter "name=clab-lab4" --format '{{.Names}}'); do
            docker exec "$container" tc qdisc replace dev eth0 root netem loss ${LOSS}% 2>/dev/null || \
            docker exec "$container" tc qdisc add dev eth0 root netem loss ${LOSS}% 2>/dev/null || true
        done

        # Let it run for 60s
        sleep 60

        # Collect metrics
        local COMMANDER=$(docker ps --format '{{.Names}}' | grep "commander" | head -1)
        if [ -n "$COMMANDER" ]; then
            docker logs "$COMMANDER" 2>&1 | grep -E "AggregationCompleted|latency_ms" | tail -50 > "${RESULTS_DIR}/packet-loss/loss_${LOSS}pct.txt" || true
        fi

        # Remove packet loss
        for container in $(docker ps --filter "name=clab-lab4" --format '{{.Names}}'); do
            docker exec "$container" tc qdisc del dev eth0 root 2>/dev/null || true
        done

        # Recovery time
        sleep 30
    done

    log_success "Packet loss sweep complete"
}

# Test: Node knockout
test_node_knockout() {
    log_info "=== Lab 5d: Node Knockout ==="
    mkdir -p "${RESULTS_DIR}/knockout"

    # Kill a random soldier
    local VICTIM=$(docker ps --filter "name=clab-lab4" --format '{{.Names}}' | grep "soldier" | grep -v "leader" | shuf -n 1)
    if [ -n "$VICTIM" ]; then
        log_event "Killing soldier: $VICTIM"
        echo "victim: $VICTIM" > "${RESULTS_DIR}/knockout/soldier_kill.txt"
        echo "kill_time: $(date +%H:%M:%S.%3N)" >> "${RESULTS_DIR}/knockout/soldier_kill.txt"

        # Emit chaos event
        emit_chaos_event "inject" "knockout" "{\"node\":\"${VICTIM}\",\"node_type\":\"soldier\"}"

        docker kill --signal=SIGKILL "$VICTIM"

        # Wait for potential detection/recovery
        sleep 30

        # Emit recover event (node is dead but system should adapt)
        emit_chaos_event "recover" "knockout" "{\"node\":\"${VICTIM}\",\"status\":\"killed\"}"

        # Measure recovery and verify consistency
        "${SCRIPT_DIR}/measure-recovery.sh" --target 10000 --chaos-mode knockout 2>&1 | tee -a "${RESULTS_DIR}/knockout/soldier_kill.txt" || true
        "${SCRIPT_DIR}/verify-consistency.sh" 2>&1 | tee -a "${RESULTS_DIR}/knockout/soldier_kill.txt" || true

        # Check remaining containers
        docker ps --filter "name=clab-lab4" -q | wc -l >> "${RESULTS_DIR}/knockout/soldier_kill.txt"
    fi

    log_success "Node knockout test complete"
}

# Test: Leader assassination
test_leader_kill() {
    log_info "=== Lab 5e: Leader Assassination ==="
    mkdir -p "${RESULTS_DIR}/leader-kill"

    # Kill a squad leader
    local VICTIM=$(docker ps --filter "name=clab-lab4" --format '{{.Names}}' | grep -E "squad-[0-9]+-leader" | shuf -n 1)
    if [ -n "$VICTIM" ]; then
        log_event "Killing squad leader: $VICTIM"
        echo "victim: $VICTIM" > "${RESULTS_DIR}/leader-kill/squad_leader_kill.txt"
        echo "kill_time: $(date +%H:%M:%S.%3N)" >> "${RESULTS_DIR}/leader-kill/squad_leader_kill.txt"

        # Emit chaos event
        emit_chaos_event "inject" "leader_kill" "{\"node\":\"${VICTIM}\",\"level\":\"squad\"}"

        docker kill --signal=SIGKILL "$VICTIM"

        # Wait for detection/recovery
        sleep 30

        # Emit recover event
        emit_chaos_event "recover" "leader_kill" "{\"node\":\"${VICTIM}\",\"status\":\"killed\"}"

        # Measure recovery and verify consistency
        "${SCRIPT_DIR}/measure-recovery.sh" --target 10000 --chaos-mode leader_kill 2>&1 | tee -a "${RESULTS_DIR}/leader-kill/squad_leader_kill.txt" || true
        "${SCRIPT_DIR}/verify-consistency.sh" 2>&1 | tee -a "${RESULTS_DIR}/leader-kill/squad_leader_kill.txt" || true

        # Check hierarchy health
        local COMMANDER=$(docker ps --format '{{.Names}}' | grep "commander" | head -1)
        if [ -n "$COMMANDER" ]; then
            docker logs "$COMMANDER" 2>&1 | tail -100 >> "${RESULTS_DIR}/leader-kill/squad_leader_kill.txt"
        fi
    fi

    log_success "Leader assassination test complete"
}

# Test: Blackout
test_blackout() {
    log_info "=== Lab 5g: Blackout Test ==="
    mkdir -p "${RESULTS_DIR}/blackout"

    for DURATION in 5 10 30; do
        log_event "Testing ${DURATION}s blackout..."
        echo "blackout_start: $(date +%H:%M:%S.%3N)" > "${RESULTS_DIR}/blackout/blackout_${DURATION}s.txt"

        # Emit chaos event for metrics
        emit_chaos_event "inject" "blackout" "{\"duration_seconds\":${DURATION}}"

        # Apply 100% loss
        for container in $(docker ps --filter "name=clab-lab4" --format '{{.Names}}'); do
            docker exec "$container" tc qdisc replace dev eth0 root netem loss 100% 2>/dev/null || \
            docker exec "$container" tc qdisc add dev eth0 root netem loss 100% 2>/dev/null || true
        done

        sleep "$DURATION"

        echo "blackout_end: $(date +%H:%M:%S.%3N)" >> "${RESULTS_DIR}/blackout/blackout_${DURATION}s.txt"

        # Emit recover event
        emit_chaos_event "recover" "blackout" "{\"duration_seconds\":${DURATION}}"

        # Restore
        for container in $(docker ps --filter "name=clab-lab4" --format '{{.Names}}'); do
            docker exec "$container" tc qdisc del dev eth0 root 2>/dev/null || true
        done

        # Measure recovery time
        log_info "Measuring recovery time for ${DURATION}s blackout..."
        local TARGET_MS=60000
        if [ "$DURATION" -le 10 ]; then
            TARGET_MS=30000
        fi
        "${SCRIPT_DIR}/measure-recovery.sh" --target "$TARGET_MS" --chaos-mode blackout 2>&1 | tee -a "${RESULTS_DIR}/blackout/blackout_${DURATION}s.txt" || true

        # Verify consistency
        "${SCRIPT_DIR}/verify-consistency.sh" 2>&1 | tee -a "${RESULTS_DIR}/blackout/blackout_${DURATION}s.txt" || true

        # Short pause before next test
        sleep 10
    done

    log_success "Blackout tests complete"
}

# Main
main() {
    log_info "╔════════════════════════════════════════════════════════════╗"
    log_info "║  Lab 5: Chaos Engineering Test Suite                       ║"
    log_info "╚════════════════════════════════════════════════════════════╝"

    check_prereqs

    # Clear old results and prepare
    clear_results_files
    mkdir -p "$RESULTS_DIR"

    log_info "Results will be saved to: $RESULTS_DIR"

    collect_baseline

    # Run tests
    test_packet_loss
    test_node_knockout
    test_leader_kill
    test_blackout

    # Final cleanup and measurements
    recover_and_collect "final" 60000

    # Copy metrics files to results directory
    cp "$CHAOS_EVENTS" "${RESULTS_DIR}/" 2>/dev/null || true
    cp "$RECOVERY_RESULTS" "${RESULTS_DIR}/" 2>/dev/null || true
    cp "$CONSISTENCY_RESULTS" "${RESULTS_DIR}/" 2>/dev/null || true

    # Generate final report
    log_info "Generating final report..."
    "${SCRIPT_DIR}/lab5-report.sh" 2>&1 | tee "${RESULTS_DIR}/lab5-report.txt"

    # Summary
    log_success "╔════════════════════════════════════════════════════════════╗"
    log_success "║  Lab 5 Chaos Suite Complete                                ║"
    log_success "╚════════════════════════════════════════════════════════════╝"
    log_success "Results: $RESULTS_DIR"

    # List results
    find "$RESULTS_DIR" -type f -name "*.txt" | while read f; do
        echo "  - $f"
    done
}

main "$@"
