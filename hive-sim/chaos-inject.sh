#!/bin/bash
# chaos-inject.sh - Chaos injection utilities for Lab 5 testing
# Usage: ./chaos-inject.sh <mode> [options]
#
# Modes:
#   intermittent --cycle <seconds>    Toggle network on/off
#   churn --rate <seconds>            Kill/restart nodes periodically
#   partition --groups <n>            Split network into n isolated groups

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LAB_FILTER="clab-lab4"
CHAOS_EVENTS_FILE="/tmp/lab5-chaos-events.jsonl"

log_info() { echo "[INFO] $(date +%H:%M:%S) $*"; }
log_warn() { echo "[WARN] $(date +%H:%M:%S) $*"; }
log_event() { echo "[EVENT] $(date +%H:%M:%S.%3N) $*"; }

# Emit structured chaos event for metrics collection
# Usage: log_chaos_event <type> <mode> [details]
#   type: "inject" or "recover"
#   mode: "intermittent", "churn", "partition", "blackout", "packet_loss", "knockout"
#   details: optional JSON object with additional info
log_chaos_event() {
    local EVENT_TYPE="$1"
    local CHAOS_MODE="$2"
    local DETAILS="${3:-{}}"
    local TIMESTAMP_MS=$(date +%s%3N)

    local EVENT_JSON="{\"type\":\"${EVENT_TYPE}\",\"mode\":\"${CHAOS_MODE}\",\"timestamp_ms\":${TIMESTAMP_MS},\"details\":${DETAILS}}"
    echo "CHAOS_EVENT: $EVENT_JSON"
    echo "$EVENT_JSON" >> "$CHAOS_EVENTS_FILE"
}

get_containers() {
    docker ps --filter "name=${LAB_FILTER}" --format '{{.Names}}'
}

get_container_count() {
    docker ps --filter "name=${LAB_FILTER}" -q | wc -l
}

# Mode: intermittent - Toggle network connectivity on/off
mode_intermittent() {
    local CYCLE=${1:-30}

    log_info "Starting intermittent connectivity simulation"
    log_info "Cycle: ${CYCLE}s connected, ${CYCLE}s disconnected"
    log_info "Press Ctrl+C to stop"

    trap 'log_info "Stopping..."; mode_recover; exit 0' INT TERM

    while true; do
        # Disconnect phase
        log_event "DISCONNECT - applying 100% packet loss"
        log_chaos_event "inject" "intermittent" "{\"action\":\"disconnect\",\"cycle_seconds\":${CYCLE}}"
        for container in $(get_containers); do
            docker exec "$container" tc qdisc replace dev eth0 root netem loss 100% 2>/dev/null || \
            docker exec "$container" tc qdisc add dev eth0 root netem loss 100% 2>/dev/null || true
        done
        sleep "$CYCLE"

        # Reconnect phase
        log_event "RECONNECT - removing packet loss"
        log_chaos_event "recover" "intermittent" "{\"action\":\"reconnect\",\"cycle_seconds\":${CYCLE}}"
        for container in $(get_containers); do
            docker exec "$container" tc qdisc del dev eth0 root 2>/dev/null || true
        done
        sleep "$CYCLE"
    done
}

# Mode: churn - Kill and restart nodes randomly
mode_churn() {
    local RATE=${1:-10}

    log_info "Starting node churn simulation"
    log_info "Rate: 1 node killed every ${RATE}s"
    log_info "Press Ctrl+C to stop"

    trap 'log_info "Stopping churn simulation"; exit 0' INT TERM

    while true; do
        # Only target soldiers (not leaders) to avoid breaking hierarchy
        local SOLDIERS=$(get_containers | grep "soldier" | grep -v "leader")
        if [ -z "$SOLDIERS" ]; then
            log_warn "No soldier nodes available for churn"
            sleep "$RATE"
            continue
        fi

        local VICTIM=$(echo "$SOLDIERS" | shuf -n 1)
        log_event "KILL node: $VICTIM"
        log_chaos_event "inject" "churn" "{\"action\":\"kill\",\"node\":\"${VICTIM}\"}"
        docker kill --signal=SIGKILL "$VICTIM" 2>/dev/null || true

        # Wait half the rate, then restart
        sleep $((RATE / 2))

        log_event "RESTART node: $VICTIM"
        log_chaos_event "recover" "churn" "{\"action\":\"restart\",\"node\":\"${VICTIM}\"}"
        docker start "$VICTIM" 2>/dev/null || true

        sleep $((RATE / 2))
    done
}

# Mode: partition - Split network into isolated groups
mode_partition() {
    local GROUPS=${1:-2}

    log_info "Creating ${GROUPS}-way network partition"

    # Get all containers and their IPs
    local CONTAINERS=($(get_containers))
    local TOTAL=${#CONTAINERS[@]}

    if [ "$TOTAL" -lt "$GROUPS" ]; then
        log_warn "Not enough containers ($TOTAL) for $GROUPS groups"
        exit 1
    fi

    local PER_GROUP=$((TOTAL / GROUPS))
    log_info "Partitioning $TOTAL containers into $GROUPS groups of ~$PER_GROUP each"

    # Assign containers to groups
    declare -A CONTAINER_GROUP
    local GROUP_ID=0
    local COUNT=0

    for container in "${CONTAINERS[@]}"; do
        CONTAINER_GROUP[$container]=$GROUP_ID
        COUNT=$((COUNT + 1))
        if [ "$COUNT" -ge "$PER_GROUP" ] && [ "$GROUP_ID" -lt "$((GROUPS - 1))" ]; then
            GROUP_ID=$((GROUP_ID + 1))
            COUNT=0
        fi
    done

    # Log group assignments
    for ((g=0; g<GROUPS; g++)); do
        local GROUP_MEMBERS=""
        for container in "${CONTAINERS[@]}"; do
            if [ "${CONTAINER_GROUP[$container]}" -eq "$g" ]; then
                GROUP_MEMBERS="$GROUP_MEMBERS $container"
            fi
        done
        log_info "Group $g:$GROUP_MEMBERS"
    done

    # Apply iptables rules to block cross-group traffic
    log_event "PARTITION - blocking cross-group traffic"
    log_chaos_event "inject" "partition" "{\"groups\":${GROUPS},\"total_nodes\":${TOTAL}}"

    for container in "${CONTAINERS[@]}"; do
        local MY_GROUP=${CONTAINER_GROUP[$container]}
        local MY_IP=$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$container" 2>/dev/null)

        if [ -z "$MY_IP" ]; then
            continue
        fi

        # Block traffic to/from containers in other groups
        for other in "${CONTAINERS[@]}"; do
            if [ "$container" == "$other" ]; then
                continue
            fi

            local OTHER_GROUP=${CONTAINER_GROUP[$other]}
            if [ "$MY_GROUP" != "$OTHER_GROUP" ]; then
                local OTHER_IP=$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$other" 2>/dev/null)
                if [ -n "$OTHER_IP" ]; then
                    docker exec "$container" iptables -A OUTPUT -d "$OTHER_IP" -j DROP 2>/dev/null || true
                    docker exec "$container" iptables -A INPUT -s "$OTHER_IP" -j DROP 2>/dev/null || true
                fi
            fi
        done
    done

    log_info "Partition applied. Run 'make lab5-recover' to restore connectivity."
}

# Mode: recover - Remove all chaos injections
mode_recover() {
    log_info "Removing all chaos injections..."
    log_chaos_event "recover" "all" "{\"action\":\"cleanup\"}"

    for container in $(get_containers); do
        # Remove tc qdisc rules
        docker exec "$container" tc qdisc del dev eth0 root 2>/dev/null || true

        # Flush iptables rules
        docker exec "$container" iptables -F INPUT 2>/dev/null || true
        docker exec "$container" iptables -F OUTPUT 2>/dev/null || true
    done

    log_info "Chaos removed. Network restored."
}

# Parse arguments
MODE=${1:-help}
shift || true

case "$MODE" in
    intermittent)
        CYCLE=30
        while [[ $# -gt 0 ]]; do
            case $1 in
                --cycle) CYCLE="$2"; shift 2 ;;
                *) shift ;;
            esac
        done
        mode_intermittent "$CYCLE"
        ;;
    churn)
        RATE=10
        while [[ $# -gt 0 ]]; do
            case $1 in
                --rate) RATE="$2"; shift 2 ;;
                *) shift ;;
            esac
        done
        mode_churn "$RATE"
        ;;
    partition)
        GROUPS=2
        while [[ $# -gt 0 ]]; do
            case $1 in
                --groups) GROUPS="$2"; shift 2 ;;
                *) shift ;;
            esac
        done
        mode_partition "$GROUPS"
        ;;
    recover)
        mode_recover
        ;;
    *)
        echo "Usage: $0 <mode> [options]"
        echo ""
        echo "Modes:"
        echo "  intermittent --cycle <seconds>    Toggle network on/off (default: 30s)"
        echo "  churn --rate <seconds>            Kill/restart nodes periodically (default: 10s)"
        echo "  partition --groups <n>            Split network into n isolated groups (default: 2)"
        echo "  recover                           Remove all chaos injections"
        exit 1
        ;;
esac
