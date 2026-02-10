#!/usr/bin/env bash
# Apply network impairments per zone for port terminal topology
# Usage: ./apply-port-terminal-impairments.sh [topology-name]
#
# Zone profiles:
#   berth: 50 Mbps, 15ms delay, 5ms jitter, 2% loss (industrial WiFi)
#   yard:  100 Mbps, 10ms delay, 3ms jitter, 1% loss (cellular/WiFi)
#   gate:  1000 Mbps, 2ms delay, 1ms jitter, 0.1% loss (wired backbone)
#   toc:   1000 Mbps, 1ms delay, 0ms jitter, 0% loss (fiber)

set -euo pipefail

TOPO_NAME="${1:-port-terminal-200n}"

apply_netem() {
    local container="$1" rate_kbps="$2" delay="$3" jitter="$4" loss="$5"
    echo "  netem: $container — ${rate_kbps}kbps, ${delay} delay, ${jitter} jitter, ${loss}% loss"
    containerlab tools netem set -n "$container" -i eth0 \
        --rate "$rate_kbps" --delay "$delay" --jitter "$jitter" --loss "$loss" 2>/dev/null || \
        echo "    WARN: failed to set netem on $container"
}

echo "Applying network impairments for $TOPO_NAME..."
echo ""
echo "── TOC zone (Fiber backbone) ──"
apply_netem "clab-$TOPO_NAME-toc" 1000000 1ms 0ms 0
echo ""

echo "── BERTH zone (Industrial WiFi (metal interference)) ──"
apply_netem "clab-$TOPO_NAME-berth-1-qc-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-qc-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-qc-3" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-qc-4" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-reefer-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-reefer-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-reefer-3" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-reefer-4" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-1-worker-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-1-worker-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-1-worker-3" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-1-worker-4" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-1-worker-5" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-1-worker-6" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-2-worker-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-2-worker-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-2-worker-3" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-2-worker-4" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-2-worker-5" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-2-worker-6" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-3-worker-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-3-worker-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-3-worker-3" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-3-worker-4" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-3-worker-5" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-3-worker-6" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-4-worker-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-4-worker-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-4-worker-3" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-4-worker-4" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-4-worker-5" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-4-worker-6" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-5-worker-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-5-worker-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-5-worker-3" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-5-worker-4" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-5-worker-5" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-5-worker-6" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-6-worker-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-6-worker-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-6-worker-3" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-6-worker-4" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-6-worker-5" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-6-worker-6" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-lead-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-lead-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-lead-3" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-lead-4" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-lead-5" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-stevedore-lead-6" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-supervisor" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-tractor-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-tractor-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-tractor-3" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-tractor-4" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-vessel-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-1-vessel-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-qc-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-qc-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-qc-3" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-qc-4" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-reefer-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-reefer-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-reefer-3" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-reefer-4" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-1-worker-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-1-worker-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-1-worker-3" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-1-worker-4" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-1-worker-5" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-1-worker-6" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-2-worker-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-2-worker-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-2-worker-3" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-2-worker-4" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-2-worker-5" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-2-worker-6" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-3-worker-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-3-worker-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-3-worker-3" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-3-worker-4" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-3-worker-5" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-3-worker-6" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-4-worker-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-4-worker-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-4-worker-3" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-4-worker-4" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-4-worker-5" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-4-worker-6" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-5-worker-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-5-worker-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-5-worker-3" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-5-worker-4" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-5-worker-5" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-5-worker-6" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-6-worker-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-6-worker-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-6-worker-3" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-6-worker-4" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-6-worker-5" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-6-worker-6" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-lead-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-lead-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-lead-3" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-lead-4" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-lead-5" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-stevedore-lead-6" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-supervisor" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-tractor-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-tractor-2" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-tractor-3" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-tractor-4" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-vessel-1" 50000 15ms 5ms 2
apply_netem "clab-$TOPO_NAME-berth-2-vessel-2" 50000 15ms 5ms 2
echo ""

echo "── YARD zone (Cellular/WiFi mix) ──"
apply_netem "clab-$TOPO_NAME-yard-block-1" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-block-2" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-block-3" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-block-4" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-block-5" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-block-6" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-block-7" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-block-8" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-crane-1" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-crane-2" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-crane-3" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-crane-4" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-inspector-1" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-inspector-2" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-inspector-3" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-inspector-4" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-manager" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-reefer-1" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-reefer-2" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-reefer-3" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-reefer-4" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-reefer-5" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-reefer-6" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-reefer-7" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-reefer-8" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-stacker-1" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-stacker-2" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-stacker-3" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-stacker-4" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-stacker-5" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-stacker-6" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-stacker-7" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-stacker-8" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-tos-1" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-tos-2" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-tos-3" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-tos-4" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-tos-5" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-tos-6" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-tos-7" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-tos-8" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-tractor-1" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-tractor-10" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-tractor-11" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-tractor-12" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-tractor-2" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-tractor-3" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-tractor-4" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-tractor-5" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-tractor-6" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-tractor-7" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-tractor-8" 100000 10ms 3ms 1
apply_netem "clab-$TOPO_NAME-yard-tractor-9" 100000 10ms 3ms 1
echo ""

echo "── GATE zone (Wired backbone + WiFi mobile) ──"
apply_netem "clab-$TOPO_NAME-gate-customs-1" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-customs-2" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-manager" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-rail-loader-1" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-rail-loader-2" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-rail-scanner-1" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-rail-scanner-2" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-rail-supervisor" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-security-1" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-security-2" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-security-3" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-security-4" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-truck-1-controller" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-truck-1-lane-1-operator" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-truck-1-lane-1-rfid" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-truck-1-lane-1-scanner" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-truck-1-lane-2-operator" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-truck-1-lane-2-rfid" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-truck-1-lane-2-scanner" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-truck-1-lane-3-operator" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-truck-1-lane-3-rfid" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-truck-1-lane-3-scanner" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-truck-2-controller" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-truck-2-lane-1-operator" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-truck-2-lane-1-rfid" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-truck-2-lane-1-scanner" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-truck-2-lane-2-operator" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-truck-2-lane-2-rfid" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-truck-2-lane-2-scanner" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-truck-2-lane-3-operator" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-truck-2-lane-3-rfid" 1000000 2ms 1ms 0.1
apply_netem "clab-$TOPO_NAME-gate-truck-2-lane-3-scanner" 1000000 2ms 1ms 0.1
echo ""

echo "All impairments applied."