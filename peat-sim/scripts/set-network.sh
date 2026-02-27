#!/bin/bash
################################################################################
# Network Scenario Configuration Script
#
# Configures different network conditions for PEAT Protocol testing using
# containerlab's netem integration and direct tc manipulation.
#
# Usage:
#   ./set-network.sh <scenario> [lab_name_filter]
#
# Scenarios:
#   nominal         - Clear all network constraints (baseline)
#   contested-light - 30% packet loss, 200ms latency
#   contested-heavy - 50% packet loss, 500ms latency
#   bandwidth-limited - 100 Kbps bandwidth limit
#   tactical-radio  - 500 Kbps, 100ms latency, 5% loss (vignette target)
#   degraded        - 256 Kbps, 200ms latency, 10% loss
#   partition-alpha - Isolate "alpha" containers from network
#
# Examples:
#   ./set-network.sh nominal                    # Clear all constraints
#   ./set-network.sh contested-light            # Apply to all clab containers
#   ./set-network.sh bandwidth-limited cap-platoon  # Apply only to matching lab
#
# Verification:
#   ./set-network.sh verify                     # Show current network state
#
# Issue: #295
################################################################################

set -euo pipefail

# Colors for output
CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BOLD='\033[1m'
NC='\033[0m'

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

log_info() {
    echo -e "${CYAN}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[OK]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_header() {
    echo ""
    echo -e "${BOLD}════════════════════════════════════════════════════════════${NC}"
    echo -e "${BOLD}  $1${NC}"
    echo -e "${BOLD}════════════════════════════════════════════════════════════${NC}"
    echo ""
}

# Get containerlab containers, optionally filtered by lab name
get_containers() {
    local filter="${1:-}"
    if [ -n "$filter" ]; then
        docker ps --format '{{.Names}}' | grep "^clab-.*${filter}" || true
    else
        docker ps --format '{{.Names}}' | grep '^clab-' || true
    fi
}

# Get all interfaces in a container
get_interfaces() {
    local container="$1"
    docker exec "${container}" ip link show 2>/dev/null | grep -oP 'eth\d+' | sort -u || true
}

# Clear all network constraints from a container
clear_constraints() {
    local container="$1"
    local interfaces=$(get_interfaces "$container")

    for iface in $interfaces; do
        # Remove any existing tc rules
        docker exec "${container}" tc qdisc del dev "${iface}" root 2>/dev/null || true
    done
}

# Apply netem constraints to a container
# Usage: apply_netem container rate_kbps delay_ms loss_percent jitter_ms
apply_netem() {
    local container="$1"
    local rate_kbps="${2:-0}"       # 0 = no rate limit
    local delay_ms="${3:-0}"        # 0 = no delay
    local loss_percent="${4:-0}"    # 0 = no loss
    local jitter_ms="${5:-0}"       # 0 = no jitter

    local interfaces=$(get_interfaces "$container")

    for iface in $interfaces; do
        # Clear existing rules first
        docker exec "${container}" tc qdisc del dev "${iface}" root 2>/dev/null || true

        # Build netem command
        local netem_opts=""

        if [ "$delay_ms" -gt 0 ]; then
            if [ "$jitter_ms" -gt 0 ]; then
                netem_opts="${netem_opts} delay ${delay_ms}ms ${jitter_ms}ms distribution normal"
            else
                netem_opts="${netem_opts} delay ${delay_ms}ms"
            fi
        fi

        if [ "$loss_percent" -gt 0 ]; then
            netem_opts="${netem_opts} loss ${loss_percent}%"
        fi

        if [ "$rate_kbps" -gt 0 ]; then
            netem_opts="${netem_opts} rate ${rate_kbps}kbit"
        fi

        if [ -n "$netem_opts" ]; then
            docker exec "${container}" tc qdisc add dev "${iface}" root netem ${netem_opts} 2>/dev/null || true
        fi
    done
}

# Isolate a container (99% packet loss = effectively disconnected)
isolate_container() {
    local container="$1"
    apply_netem "$container" 0 0 99 0
}

# Scenario: Nominal (clear all constraints)
scenario_nominal() {
    local filter="${1:-}"
    log_header "Scenario: NOMINAL (Baseline)"
    log_info "Clearing all network constraints..."

    local containers=($(get_containers "$filter"))
    local count=0

    for container in "${containers[@]}"; do
        clear_constraints "$container"
        count=$((count + 1))
    done

    log_success "Cleared constraints on ${count} containers"
}

# Scenario: Contested Light (30% loss, 200ms latency)
scenario_contested_light() {
    local filter="${1:-}"
    log_header "Scenario: CONTESTED-LIGHT"
    log_info "Applying: 30% packet loss, 200ms latency, 50ms jitter"

    local containers=($(get_containers "$filter"))
    local count=0

    for container in "${containers[@]}"; do
        apply_netem "$container" 0 200 30 50
        count=$((count + 1))
    done

    log_success "Applied contested-light conditions to ${count} containers"
    echo ""
    echo "Parameters:"
    echo "  - Packet Loss: 30%"
    echo "  - Latency: 200ms (+/- 50ms jitter)"
    echo "  - Bandwidth: Unrestricted"
}

# Scenario: Contested Heavy (50% loss, 500ms latency)
scenario_contested_heavy() {
    local filter="${1:-}"
    log_header "Scenario: CONTESTED-HEAVY"
    log_info "Applying: 50% packet loss, 500ms latency, 100ms jitter"

    local containers=($(get_containers "$filter"))
    local count=0

    for container in "${containers[@]}"; do
        apply_netem "$container" 0 500 50 100
        count=$((count + 1))
    done

    log_success "Applied contested-heavy conditions to ${count} containers"
    echo ""
    echo "Parameters:"
    echo "  - Packet Loss: 50%"
    echo "  - Latency: 500ms (+/- 100ms jitter)"
    echo "  - Bandwidth: Unrestricted"
}

# Scenario: Bandwidth Limited (100 Kbps)
scenario_bandwidth_limited() {
    local filter="${1:-}"
    log_header "Scenario: BANDWIDTH-LIMITED"
    log_info "Applying: 100 Kbps bandwidth limit"

    local containers=($(get_containers "$filter"))
    local count=0

    for container in "${containers[@]}"; do
        apply_netem "$container" 100 10 0 0
        count=$((count + 1))
    done

    log_success "Applied bandwidth-limited conditions to ${count} containers"
    echo ""
    echo "Parameters:"
    echo "  - Bandwidth: 100 Kbps"
    echo "  - Latency: 10ms (minimal)"
    echo "  - Packet Loss: 0%"
}

# Scenario: Tactical Radio (500 Kbps, 100ms, 5% loss - vignette target)
scenario_tactical_radio() {
    local filter="${1:-}"
    log_header "Scenario: TACTICAL-RADIO"
    log_info "Applying: 500 Kbps, 100ms latency, 5% loss (vignette P5 target)"

    local containers=($(get_containers "$filter"))
    local count=0

    for container in "${containers[@]}"; do
        apply_netem "$container" 500 100 5 25
        count=$((count + 1))
    done

    log_success "Applied tactical-radio conditions to ${count} containers"
    echo ""
    echo "Parameters (matches vignette P5 requirement):"
    echo "  - Bandwidth: 500 Kbps"
    echo "  - Latency: 100ms (+/- 25ms jitter)"
    echo "  - Packet Loss: 5%"
}

# Scenario: Degraded (256 Kbps, 200ms, 10% loss)
scenario_degraded() {
    local filter="${1:-}"
    log_header "Scenario: DEGRADED"
    log_info "Applying: 256 Kbps, 200ms latency, 10% loss"

    local containers=($(get_containers "$filter"))
    local count=0

    for container in "${containers[@]}"; do
        apply_netem "$container" 256 200 10 50
        count=$((count + 1))
    done

    log_success "Applied degraded conditions to ${count} containers"
    echo ""
    echo "Parameters:"
    echo "  - Bandwidth: 256 Kbps"
    echo "  - Latency: 200ms (+/- 50ms jitter)"
    echo "  - Packet Loss: 10%"
}

# Scenario: Partition Alpha (isolate alpha team)
scenario_partition_alpha() {
    local filter="${1:-}"
    log_header "Scenario: PARTITION-ALPHA"
    log_info "Isolating containers with 'alpha' in name"

    local all_containers=($(get_containers "$filter"))
    local alpha_count=0
    local other_count=0

    for container in "${all_containers[@]}"; do
        if [[ "$container" == *"alpha"* ]]; then
            isolate_container "$container"
            alpha_count=$((alpha_count + 1))
        else
            # Keep other containers at nominal
            clear_constraints "$container"
            other_count=$((other_count + 1))
        fi
    done

    log_success "Isolated ${alpha_count} alpha containers"
    log_info "${other_count} other containers remain at nominal"
    echo ""
    echo "Parameters:"
    echo "  - Alpha containers: 99% packet loss (isolated)"
    echo "  - Other containers: Nominal (no constraints)"
}

# Verify current network state
verify_network() {
    local filter="${1:-}"
    log_header "Network State Verification"

    local containers=($(get_containers "$filter"))

    if [ ${#containers[@]} -eq 0 ]; then
        log_warn "No containerlab containers found"
        return 1
    fi

    echo "Found ${#containers[@]} containers"
    echo ""

    for container in "${containers[@]}"; do
        echo -e "${BOLD}Container: ${container}${NC}"
        local interfaces=$(get_interfaces "$container")

        for iface in $interfaces; do
            echo "  Interface: ${iface}"
            local qdisc_info=$(docker exec "${container}" tc qdisc show dev "${iface}" 2>/dev/null || echo "No qdisc info")

            if echo "$qdisc_info" | grep -q "netem"; then
                echo "    $(echo "$qdisc_info" | grep netem | head -1)"
            else
                echo "    No netem constraints (nominal)"
            fi
        done
        echo ""
    done

    # Suggest verification commands
    echo -e "${BOLD}Verification Commands:${NC}"
    echo ""
    echo "# Ping test between containers (latency check):"
    echo "docker exec <container1> ping -c 5 <container2_ip>"
    echo ""
    echo "# Bandwidth test with iperf3 (if installed in containers):"
    echo "# On server: docker exec <server> iperf3 -s"
    echo "# On client: docker exec <client> iperf3 -c <server_ip> -t 10"
    echo ""
    echo "# Check packet loss with extended ping:"
    echo "docker exec <container> ping -c 100 <target_ip>"
}

# Print usage
usage() {
    cat << 'EOF'
PEAT Network Scenario Configuration

Usage: ./set-network.sh <scenario> [lab_name_filter]

Scenarios:
  nominal           Clear all network constraints (baseline)
  contested-light   30% packet loss, 200ms latency, 50ms jitter
  contested-heavy   50% packet loss, 500ms latency, 100ms jitter
  bandwidth-limited 100 Kbps bandwidth limit
  tactical-radio    500 Kbps, 100ms latency, 5% loss (vignette P5)
  degraded          256 Kbps, 200ms latency, 10% loss
  partition-alpha   Isolate "alpha" containers (99% loss)
  verify            Show current network state

Options:
  lab_name_filter   Only apply to containers matching this name pattern
                    (e.g., "cap-platoon" to match clab-cap-platoon-*)

Examples:
  ./set-network.sh nominal                     # Clear all constraints
  ./set-network.sh contested-light             # Apply to all clab containers
  ./set-network.sh tactical-radio cap-platoon  # Apply to cap-platoon lab only
  ./set-network.sh verify                      # Show current state

Scenario Summary:
  ┌──────────────────┬──────────┬─────────┬──────┬────────┐
  │ Scenario         │ Bandwidth│ Latency │ Loss │ Jitter │
  ├──────────────────┼──────────┼─────────┼──────┼────────┤
  │ nominal          │ -        │ -       │ -    │ -      │
  │ contested-light  │ -        │ 200ms   │ 30%  │ 50ms   │
  │ contested-heavy  │ -        │ 500ms   │ 50%  │ 100ms  │
  │ bandwidth-limited│ 100 Kbps │ 10ms    │ 0%   │ -      │
  │ tactical-radio   │ 500 Kbps │ 100ms   │ 5%   │ 25ms   │
  │ degraded         │ 256 Kbps │ 200ms   │ 10%  │ 50ms   │
  │ partition-alpha  │ -        │ -       │ 99%* │ -      │
  └──────────────────┴──────────┴─────────┴──────┴────────┘
  * partition-alpha only affects containers with "alpha" in name

EOF
}

# Main entry point
main() {
    local scenario="${1:-}"
    local filter="${2:-}"

    if [ -z "$scenario" ]; then
        usage
        exit 1
    fi

    case "$scenario" in
        nominal)
            scenario_nominal "$filter"
            ;;
        contested-light)
            scenario_contested_light "$filter"
            ;;
        contested-heavy)
            scenario_contested_heavy "$filter"
            ;;
        bandwidth-limited)
            scenario_bandwidth_limited "$filter"
            ;;
        tactical-radio)
            scenario_tactical_radio "$filter"
            ;;
        degraded)
            scenario_degraded "$filter"
            ;;
        partition-alpha)
            scenario_partition_alpha "$filter"
            ;;
        verify)
            verify_network "$filter"
            ;;
        -h|--help|help)
            usage
            ;;
        *)
            log_error "Unknown scenario: $scenario"
            echo ""
            usage
            exit 1
            ;;
    esac
}

main "$@"
