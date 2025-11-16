#!/bin/bash
################################################################################
# Connection Churn Injection Script
#
# Simulates tactical radio network challenges by randomly dropping and
# restoring connections between containerlab nodes during test execution.
#
# Usage:
#   ./inject-connection-churn.sh <duration_seconds> <churn_interval_seconds> <log_file>
#
# Example:
#   ./inject-connection-churn.sh 60 10 churn-events.log
#
# Parameters:
#   - duration_seconds: How long to run churn injection (should match test duration)
#   - churn_interval_seconds: Average time between churn events (default: 10)
#   - log_file: Where to log churn events (default: connection-churn.log)
#
# Churn Behavior:
#   - Randomly selects 1-3 containers from running clab topology
#   - Introduces 99% packet loss on all interfaces (simulates connection drop)
#   - Maintains outage for 5-15 seconds (random)
#   - Restores connection (removes packet loss)
#   - Logs all events with timestamps for correlation with metrics
#
# Network Impairment Method:
#   Uses tc netem to add packet loss to container interfaces:
#   - 99% loss = effectively disconnected
#   - Applied to all eth interfaces in container
#   - Can be removed cleanly without container restart
################################################################################

set -euo pipefail

# Colors for output
CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

# Parameters
DURATION=${1:-60}
CHURN_INTERVAL=${2:-10}
LOG_FILE=${3:-"connection-churn.log"}

# Churn configuration
MIN_CONTAINERS=1
MAX_CONTAINERS=3
MIN_OUTAGE_DURATION=5
MAX_OUTAGE_DURATION=15

log_event() {
    local timestamp=$(date +%Y-%m-%d\ %H:%M:%S.%3N)
    local message="$1"
    echo "[${timestamp}] ${message}" | tee -a "${LOG_FILE}"
}

log_info() {
    echo -e "${CYAN}→ $1${NC}"
}

log_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

log_warn() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

log_error() {
    echo -e "${RED}✗ $1${NC}"
}

# Get list of running containerlab containers
get_clab_containers() {
    docker ps --format '{{.Names}}' | grep '^clab-' || true
}

# Apply connection drop to a container (99% packet loss on all interfaces)
drop_connection() {
    local container="$1"

    # Get all eth interfaces in container
    local interfaces=$(docker exec "${container}" ip link show | grep -oP 'eth\d+' | sort -u || true)

    if [ -z "$interfaces" ]; then
        log_warn "No interfaces found in ${container}"
        return 1
    fi

    for iface in $interfaces; do
        # Add 99% packet loss to simulate connection drop
        docker exec "${container}" tc qdisc add dev "${iface}" root netem loss 99% 2>/dev/null || true
    done

    log_event "DROP,${container},99% packet loss applied"
    return 0
}

# Restore connection to a container (remove packet loss)
restore_connection() {
    local container="$1"

    # Get all eth interfaces in container
    local interfaces=$(docker exec "${container}" ip link show | grep -oP 'eth\d+' | sort -u || true)

    if [ -z "$interfaces" ]; then
        log_warn "No interfaces found in ${container}"
        return 1
    fi

    for iface in $interfaces; do
        # Remove tc qdisc to restore connection
        docker exec "${container}" tc qdisc del dev "${iface}" root 2>/dev/null || true
    done

    log_event "RESTORE,${container},connection restored"
    return 0
}

# Main churn loop
main() {
    log_info "Starting connection churn injection"
    log_info "Duration: ${DURATION}s, Churn interval: ${CHURN_INTERVAL}s"
    log_info "Outage duration: ${MIN_OUTAGE_DURATION}-${MAX_OUTAGE_DURATION}s"
    log_info "Logging to: ${LOG_FILE}"

    # Initialize log file
    echo "# Connection Churn Event Log" > "${LOG_FILE}"
    echo "# Format: [timestamp] EVENT_TYPE,container,details" >> "${LOG_FILE}"
    log_event "START,churn_injection,duration=${DURATION}s,interval=${CHURN_INTERVAL}s"

    local start_time=$(date +%s)
    local end_time=$((start_time + DURATION))
    local churn_count=0

    while [ $(date +%s) -lt ${end_time} ]; do
        # Get current containers
        local containers=($(get_clab_containers))

        if [ ${#containers[@]} -eq 0 ]; then
            log_warn "No containerlab containers found, waiting..."
            sleep 5
            continue
        fi

        # Randomly select 1-3 containers for churn event
        local num_to_churn=$((RANDOM % (MAX_CONTAINERS - MIN_CONTAINERS + 1) + MIN_CONTAINERS))
        num_to_churn=$((num_to_churn > ${#containers[@]} ? ${#containers[@]} : num_to_churn))

        # Shuffle containers and select subset
        local selected_containers=($(printf '%s\n' "${containers[@]}" | shuf -n ${num_to_churn}))

        churn_count=$((churn_count + 1))
        log_info "Churn event #${churn_count}: Dropping ${num_to_churn} container(s)"

        # Drop connections
        for container in "${selected_containers[@]}"; do
            drop_connection "${container}" &
        done
        wait

        # Random outage duration
        local outage_duration=$((RANDOM % (MAX_OUTAGE_DURATION - MIN_OUTAGE_DURATION + 1) + MIN_OUTAGE_DURATION))
        log_info "Maintaining outage for ${outage_duration}s"
        sleep ${outage_duration}

        # Restore connections
        log_info "Restoring connections"
        for container in "${selected_containers[@]}"; do
            restore_connection "${container}" &
        done
        wait

        log_success "Churn event #${churn_count} complete"

        # Wait until next churn event
        local time_to_wait=$((CHURN_INTERVAL + RANDOM % 5 - 2))  # Add some randomness ±2s
        time_to_wait=$((time_to_wait > 0 ? time_to_wait : 1))

        local remaining_time=$((end_time - $(date +%s)))
        if [ ${time_to_wait} -gt ${remaining_time} ]; then
            time_to_wait=${remaining_time}
        fi

        if [ ${time_to_wait} -gt 0 ]; then
            log_info "Next churn event in ${time_to_wait}s"
            sleep ${time_to_wait}
        fi
    done

    # Cleanup: Ensure all connections are restored
    log_info "Churn injection complete, restoring all connections..."
    local containers=($(get_clab_containers))
    for container in "${containers[@]}"; do
        restore_connection "${container}" 2>/dev/null || true
    done

    log_event "END,churn_injection,total_events=${churn_count}"
    log_success "Connection churn injection finished: ${churn_count} churn events"
}

# Cleanup on exit
cleanup() {
    log_warn "Caught interrupt, restoring all connections..."
    local containers=($(get_clab_containers))
    for container in "${containers[@]}"; do
        restore_connection "${container}" 2>/dev/null || true
    done
    log_event "INTERRUPT,churn_injection,cleanup_complete"
    exit 0
}

trap cleanup SIGINT SIGTERM

main "$@"
