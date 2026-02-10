#!/usr/bin/env python3
"""
Generate Port Terminal 200-Node ContainerLab Topology

Creates a full container port terminal hierarchy per ADR-051 Phase 3:
  - TOC (Terminal Operations Center) — top-level aggregation
  - 2 Berth operations (~58 nodes each): quay cranes, vessel agents, stevedore teams, tractors, reefer monitors
  - Yard zone (~53 nodes): block supervisors, stacking cranes, reach stackers, tractors, TOS terminals, reefer, inspectors
  - Gate zone (~32 nodes): truck gates (3 lanes each), rail gate, security, customs

Network impairment profiles per zone:
  - Berth:  Industrial WiFi — 50 Mbps, 15ms delay, 5ms jitter, 2% loss
  - Yard:   Cellular/WiFi   — 100 Mbps, 10ms delay, 3ms jitter, 1% loss
  - Gate:   Wired backbone   — 1 Gbps, 2ms delay, 1ms jitter, 0.1% loss
  - TOC:    Fiber backbone   — 1 Gbps, 1ms delay, 0ms jitter, 0% loss

Usage:
    python3 generate-port-terminal-topology.py
    python3 generate-port-terminal-topology.py --output topologies/port-terminal-200node.clab.yaml
"""

import argparse


# ── Zone impairment profiles (applied post-deploy via containerlab netem) ──

ZONE_IMPAIRMENTS = {
    "berth": {"rate_mbps": 50, "delay_ms": 15, "jitter_ms": 5, "loss_pct": 2.0, "desc": "Industrial WiFi (metal interference)"},
    "yard":  {"rate_mbps": 100, "delay_ms": 10, "jitter_ms": 3, "loss_pct": 1.0, "desc": "Cellular/WiFi mix"},
    "gate":  {"rate_mbps": 1000, "delay_ms": 2, "jitter_ms": 1, "loss_pct": 0.1, "desc": "Wired backbone + WiFi mobile"},
    "toc":   {"rate_mbps": 1000, "delay_ms": 1, "jitter_ms": 0, "loss_pct": 0.0, "desc": "Fiber backbone"},
}


def get_credential_env_vars():
    return [
        "        HIVE_APP_ID: ${HIVE_APP_ID}",
        "        HIVE_SECRET_KEY: ${HIVE_SECRET_KEY}",
        "        HIVE_OFFLINE_TOKEN: ${HIVE_OFFLINE_TOKEN}",
        "        HIVE_SHARED_KEY: ${HIVE_SHARED_KEY}",
    ]


def get_circuit_breaker_env_vars():
    return [
        '        CIRCUIT_FAILURE_THRESHOLD: "3"',
        '        CIRCUIT_FAILURE_WINDOW_SECS: "2"',
        '        CIRCUIT_OPEN_TIMEOUT_SECS: "2"',
        '        CIRCUIT_SUCCESS_THRESHOLD: "2"',
    ]


class PortAllocator:
    """Sequential TCP port allocator."""
    def __init__(self, start=12345):
        self._next = start

    def alloc(self):
        p = self._next
        self._next += 1
        return p


def tcp_connect_str(topo_name, parent_name, parent_port):
    container = f"clab-{topo_name}-{parent_name}"
    return f'        TCP_CONNECT: "{container}:{parent_port}"'


def node_block(node_id, *, role, platform_type, node_type, zone, mode, parent_name,
               parent_port, port, topo_name, extra_env=None, event_gen=False,
               detection_rate="1", telemetry_rate="1"):
    """Emit a single ContainerLab node definition."""
    cred = get_credential_env_vars()
    cb = get_circuit_breaker_env_vars()
    lines = [
        f"    {node_id}:",
        "      kind: linux",
        "      image: hive-sim-node:latest",
        "      env:",
        f"        NODE_ID: {node_id}",
        f"        ROLE: {role}",
        f"        PLATFORM_TYPE: {platform_type}",
        f"        NODE_TYPE: {node_type}",
        f"        ZONE: {zone}",
        f"        MODE: {mode}",
        "        BACKEND: ${BACKEND:-automerge}",
        '        CAP_IN_MEMORY: "${CAP_IN_MEMORY:-false}"',
        '        UPDATE_RATE_MS: "5000"',
        f'        TCP_LISTEN: "{port}"',
    ]
    if parent_name is not None:
        lines.append(tcp_connect_str(topo_name, parent_name, parent_port))
    if extra_env:
        lines.extend(extra_env)
    lines.extend(cred)
    if event_gen:
        lines.extend([
            '        EVENT_ROUTING_ENABLED: "true"',
            '        EVENT_GENERATION_ENABLED: "true"',
            f'        DETECTION_RATE_PER_SEC: "{detection_rate}"',
            f'        TELEMETRY_RATE_PER_SEC: "{telemetry_rate}"',
            '        ANOMALY_RATE_PER_SEC: "0.01"',
            '        CRITICAL_RATE_PER_SEC: "0.001"',
        ])
    else:
        lines.extend([
            '        EVENT_ROUTING_ENABLED: "true"',
            '        AGGREGATION_WINDOW_MS: "1000"',
        ])
    lines.extend(cb)
    lines.append("")
    return lines


def generate_topology():
    topo_name = "port-terminal-200n"
    pa = PortAllocator(12345)
    nodes = []          # list of YAML line groups
    node_zones = {}     # node_id -> zone (for impairment script)
    node_count = 0

    def add_node(node_id, zone, **kwargs):
        nonlocal node_count
        port = pa.alloc()
        blk = node_block(node_id, port=port, topo_name=topo_name, zone=zone, **kwargs)
        nodes.append(blk)
        node_zones[node_id] = zone
        node_count += 1
        return port

    # ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    # TOC — Terminal Operations Center (1 node)
    # ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    nodes.append(["    # ══════════════════════════════════════════════════"])
    nodes.append(["    # TOC — Terminal Operations Center (top-level aggregation)"])
    nodes.append(["    # ══════════════════════════════════════════════════"])
    nodes.append([""])

    toc_port = add_node("toc",
        zone="toc", role="toc", platform_type="controller", node_type="toc",
        mode="hierarchical", parent_name=None, parent_port=None)

    # ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    # Berth Operations (2 berths × ~57 nodes = ~114 nodes)
    # ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    for berth_idx in range(1, 3):
        berth_prefix = f"berth-{berth_idx}"
        nodes.append([f"    # ══════════════════════════════════════════════════"])
        nodes.append([f"    # BERTH {berth_idx} — Quayside operations (~57 nodes)"])
        nodes.append([f"    # Impairment: Industrial WiFi — 50 Mbps, 15ms, 2% loss"])
        nodes.append([f"    # ══════════════════════════════════════════════════"])
        nodes.append([""])

        # Berth supervisor → connects to TOC
        sup_port = add_node(f"{berth_prefix}-supervisor",
            zone="berth", role="berth_supervisor", platform_type="controller",
            node_type="zone_supervisor", mode="hierarchical",
            parent_name="toc", parent_port=toc_port)

        # Quay cranes (4)
        nodes.append([f"    # ----- {berth_prefix}: Quay Cranes (STS gantry) -----"])
        nodes.append([""])
        for i in range(1, 5):
            add_node(f"{berth_prefix}-qc-{i}",
                zone="berth", role="quay_crane", platform_type="crane",
                node_type="equipment", mode="hierarchical",
                parent_name=f"{berth_prefix}-supervisor", parent_port=sup_port,
                event_gen=True, detection_rate="5", telemetry_rate="2")

        # Vessel agents (2)
        nodes.append([f"    # ----- {berth_prefix}: Vessel Agents -----"])
        nodes.append([""])
        for i in range(1, 3):
            add_node(f"{berth_prefix}-vessel-{i}",
                zone="berth", role="vessel_agent", platform_type="operator",
                node_type="agent", mode="hierarchical",
                parent_name=f"{berth_prefix}-supervisor", parent_port=sup_port,
                event_gen=True, detection_rate="2", telemetry_rate="1")

        # Stevedore teams: 6 leads, each with 6 workers (6 + 36 = 42)
        nodes.append([f"    # ----- {berth_prefix}: Stevedore Teams (6 leads × 6 workers) -----"])
        nodes.append([""])
        for team_idx in range(1, 7):
            lead_id = f"{berth_prefix}-stevedore-lead-{team_idx}"
            lead_port = add_node(lead_id,
                zone="berth", role="stevedore_lead", platform_type="operator",
                node_type="team_lead", mode="hierarchical",
                parent_name=f"{berth_prefix}-supervisor", parent_port=sup_port)

            for w in range(1, 7):
                add_node(f"{berth_prefix}-stevedore-{team_idx}-worker-{w}",
                    zone="berth", role="stevedore", platform_type="worker",
                    node_type="worker", mode="hierarchical",
                    parent_name=lead_id, parent_port=lead_port,
                    event_gen=True, detection_rate="1", telemetry_rate="0.5")

        # Yard tractors assigned to berth (4)
        nodes.append([f"    # ----- {berth_prefix}: Yard Tractors -----"])
        nodes.append([""])
        for i in range(1, 5):
            add_node(f"{berth_prefix}-tractor-{i}",
                zone="berth", role="yard_tractor", platform_type="vehicle",
                node_type="equipment", mode="hierarchical",
                parent_name=f"{berth_prefix}-supervisor", parent_port=sup_port,
                event_gen=True, detection_rate="3", telemetry_rate="2")

        # Reefer monitors (4)
        nodes.append([f"    # ----- {berth_prefix}: Reefer Monitors -----"])
        nodes.append([""])
        for i in range(1, 5):
            add_node(f"{berth_prefix}-reefer-{i}",
                zone="berth", role="reefer_monitor", platform_type="sensor",
                node_type="sensor", mode="hierarchical",
                parent_name=f"{berth_prefix}-supervisor", parent_port=sup_port,
                event_gen=True, detection_rate="0.5", telemetry_rate="2")

    # ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    # Yard Zone (~53 nodes)
    # ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    nodes.append(["    # ══════════════════════════════════════════════════"])
    nodes.append(["    # YARD ZONE — Container storage & handling (~53 nodes)"])
    nodes.append(["    # Impairment: Cellular/WiFi — 100 Mbps, 10ms, 1% loss"])
    nodes.append(["    # ══════════════════════════════════════════════════"])
    nodes.append([""])

    yard_port = add_node("yard-manager",
        zone="yard", role="yard_manager", platform_type="controller",
        node_type="zone_supervisor", mode="hierarchical",
        parent_name="toc", parent_port=toc_port)

    # Yard block supervisors (8)
    nodes.append(["    # ----- Yard Block Supervisors (8 blocks) -----"])
    nodes.append([""])
    for i in range(1, 9):
        add_node(f"yard-block-{i}",
            zone="yard", role="block_supervisor", platform_type="operator",
            node_type="block_supervisor", mode="hierarchical",
            parent_name="yard-manager", parent_port=yard_port,
            event_gen=True, detection_rate="2", telemetry_rate="1")

    # Stacking cranes — RTG/RMG (4)
    nodes.append(["    # ----- Stacking Cranes (RTG/RMG) -----"])
    nodes.append([""])
    for i in range(1, 5):
        add_node(f"yard-crane-{i}",
            zone="yard", role="stacking_crane", platform_type="crane",
            node_type="equipment", mode="hierarchical",
            parent_name="yard-manager", parent_port=yard_port,
            event_gen=True, detection_rate="5", telemetry_rate="2")

    # Reach stackers (8)
    nodes.append(["    # ----- Reach Stackers -----"])
    nodes.append([""])
    for i in range(1, 9):
        add_node(f"yard-stacker-{i}",
            zone="yard", role="reach_stacker", platform_type="vehicle",
            node_type="equipment", mode="hierarchical",
            parent_name="yard-manager", parent_port=yard_port,
            event_gen=True, detection_rate="3", telemetry_rate="2")

    # Yard tractors — shared pool (12)
    nodes.append(["    # ----- Yard Tractors (shared pool) -----"])
    nodes.append([""])
    for i in range(1, 13):
        add_node(f"yard-tractor-{i}",
            zone="yard", role="yard_tractor", platform_type="vehicle",
            node_type="equipment", mode="hierarchical",
            parent_name="yard-manager", parent_port=yard_port,
            event_gen=True, detection_rate="3", telemetry_rate="2")

    # TOS terminals (8)
    nodes.append(["    # ----- TOS Terminals (Terminal Operating System) -----"])
    nodes.append([""])
    for i in range(1, 9):
        add_node(f"yard-tos-{i}",
            zone="yard", role="tos_terminal", platform_type="terminal",
            node_type="terminal", mode="hierarchical",
            parent_name="yard-manager", parent_port=yard_port,
            event_gen=True, detection_rate="8", telemetry_rate="1")

    # Reefer monitoring (8)
    nodes.append(["    # ----- Yard Reefer Monitors -----"])
    nodes.append([""])
    for i in range(1, 9):
        add_node(f"yard-reefer-{i}",
            zone="yard", role="reefer_monitor", platform_type="sensor",
            node_type="sensor", mode="hierarchical",
            parent_name="yard-manager", parent_port=yard_port,
            event_gen=True, detection_rate="0.5", telemetry_rate="2")

    # Yard inspectors (4)
    nodes.append(["    # ----- Yard Inspectors -----"])
    nodes.append([""])
    for i in range(1, 5):
        add_node(f"yard-inspector-{i}",
            zone="yard", role="yard_inspector", platform_type="worker",
            node_type="worker", mode="hierarchical",
            parent_name="yard-manager", parent_port=yard_port,
            event_gen=True, detection_rate="1", telemetry_rate="0.5")

    # ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    # Gate Zone (~32 nodes)
    # ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    nodes.append(["    # ══════════════════════════════════════════════════"])
    nodes.append(["    # GATE ZONE — Ingress/egress control (~32 nodes)"])
    nodes.append(["    # Impairment: Wired backbone — 1 Gbps, 2ms, 0.1% loss"])
    nodes.append(["    # ══════════════════════════════════════════════════"])
    nodes.append([""])

    gate_port = add_node("gate-manager",
        zone="gate", role="gate_manager", platform_type="controller",
        node_type="zone_supervisor", mode="hierarchical",
        parent_name="toc", parent_port=toc_port)

    # Truck Gate 1: controller + 3 lanes (scanner + RFID + operator each = 9)
    for gate_idx in range(1, 3):
        gprefix = f"gate-truck-{gate_idx}"
        nodes.append([f"    # ----- Truck Gate {gate_idx} (3 lanes × 3 devices) -----"])
        nodes.append([""])

        ctrl_port = add_node(f"{gprefix}-controller",
            zone="gate", role="gate_controller", platform_type="controller",
            node_type="gate_controller", mode="hierarchical",
            parent_name="gate-manager", parent_port=gate_port)

        for lane in range(1, 4):
            lprefix = f"{gprefix}-lane-{lane}"
            add_node(f"{lprefix}-scanner",
                zone="gate", role="scanner", platform_type="scanner",
                node_type="sensor", mode="hierarchical",
                parent_name=f"{gprefix}-controller", parent_port=ctrl_port,
                event_gen=True, detection_rate="10", telemetry_rate="1")
            add_node(f"{lprefix}-rfid",
                zone="gate", role="rfid_reader", platform_type="reader",
                node_type="sensor", mode="hierarchical",
                parent_name=f"{gprefix}-controller", parent_port=ctrl_port,
                event_gen=True, detection_rate="15", telemetry_rate="1")
            add_node(f"{lprefix}-operator",
                zone="gate", role="gate_operator", platform_type="operator",
                node_type="worker", mode="hierarchical",
                parent_name=f"{gprefix}-controller", parent_port=ctrl_port,
                event_gen=True, detection_rate="2", telemetry_rate="0.5")

    # Rail Gate: supervisor + 2 loaders + 2 scanners (5)
    nodes.append(["    # ----- Rail Gate -----"])
    nodes.append([""])
    rail_port = add_node("gate-rail-supervisor",
        zone="gate", role="rail_supervisor", platform_type="controller",
        node_type="gate_controller", mode="hierarchical",
        parent_name="gate-manager", parent_port=gate_port)

    for i in range(1, 3):
        add_node(f"gate-rail-loader-{i}",
            zone="gate", role="rail_loader", platform_type="vehicle",
            node_type="equipment", mode="hierarchical",
            parent_name="gate-rail-supervisor", parent_port=rail_port,
            event_gen=True, detection_rate="3", telemetry_rate="2")
    for i in range(1, 3):
        add_node(f"gate-rail-scanner-{i}",
            zone="gate", role="scanner", platform_type="scanner",
            node_type="sensor", mode="hierarchical",
            parent_name="gate-rail-supervisor", parent_port=rail_port,
            event_gen=True, detection_rate="10", telemetry_rate="1")

    # Security workers (4)
    nodes.append(["    # ----- Security -----"])
    nodes.append([""])
    for i in range(1, 5):
        add_node(f"gate-security-{i}",
            zone="gate", role="security", platform_type="worker",
            node_type="worker", mode="hierarchical",
            parent_name="gate-manager", parent_port=gate_port,
            event_gen=True, detection_rate="1", telemetry_rate="0.5")

    # Customs inspectors (2)
    nodes.append(["    # ----- Customs -----"])
    nodes.append([""])
    for i in range(1, 3):
        add_node(f"gate-customs-{i}",
            zone="gate", role="customs_inspector", platform_type="worker",
            node_type="worker", mode="hierarchical",
            parent_name="gate-manager", parent_port=gate_port,
            event_gen=True, detection_rate="2", telemetry_rate="0.5")

    # ── Assemble the full YAML ──
    header = [
        f"# Port Terminal 200-Node Topology — Phase 3 (ADR-051)",
        f"# Full terminal operation: TOC + 2 berths + yard + gate = {node_count} nodes",
        f"#",
        f"# Zone breakdown:",
        f"#   TOC:   1 node  (fiber backbone)",
        f"#   Berth: 2 × 57 = 114 nodes (industrial WiFi — 50 Mbps, 15ms, 2% loss)",
        f"#   Yard:  53 nodes (cellular/WiFi — 100 Mbps, 10ms, 1% loss)",
        f"#   Gate:  32 nodes (wired backbone — 1 Gbps, 2ms, 0.1% loss)",
        f"#",
        f"# Deploy:  containerlab deploy -t port-terminal-200node.clab.yaml",
        f"# Destroy: containerlab destroy -t port-terminal-200node.clab.yaml",
        f"# Impairments: ./apply-port-terminal-impairments.sh port-terminal-200n",
        "",
        f"name: {topo_name}",
        "",
        "mgmt:",
        f"  network: {topo_name}",
        "  ipv4-subnet: 172.20.200.0/24",
        "",
        "topology:",
        "  nodes:",
        "",
    ]

    output_lines = header[:]
    for block in nodes:
        output_lines.extend(block)

    return "\n".join(output_lines), node_count, node_zones, topo_name


def generate_impairment_script(node_zones, topo_name):
    """Generate a shell script that applies per-zone network impairments."""
    lines = [
        "#!/usr/bin/env bash",
        "# Apply network impairments per zone for port terminal topology",
        "# Usage: ./apply-port-terminal-impairments.sh [topology-name]",
        "#",
        "# Zone profiles:",
        "#   berth: 50 Mbps, 15ms delay, 5ms jitter, 2% loss (industrial WiFi)",
        "#   yard:  100 Mbps, 10ms delay, 3ms jitter, 1% loss (cellular/WiFi)",
        "#   gate:  1000 Mbps, 2ms delay, 1ms jitter, 0.1% loss (wired backbone)",
        "#   toc:   1000 Mbps, 1ms delay, 0ms jitter, 0% loss (fiber)",
        "",
        'set -euo pipefail',
        '',
        f'TOPO_NAME="${{1:-{topo_name}}}"',
        '',
        'apply_netem() {',
        '    local container="$1" rate_kbps="$2" delay="$3" jitter="$4" loss="$5"',
        '    echo "  netem: $container — ${rate_kbps}kbps, ${delay} delay, ${jitter} jitter, ${loss}% loss"',
        '    containerlab tools netem set -n "$container" -i eth0 \\',
        '        --rate "$rate_kbps" --delay "$delay" --jitter "$jitter" --loss "$loss" 2>/dev/null || \\',
        '        echo "    WARN: failed to set netem on $container"',
        '}',
        '',
        'echo "Applying network impairments for $TOPO_NAME..."',
        'echo ""',
    ]

    # Group nodes by zone
    zones = {}
    for node_id, zone in sorted(node_zones.items()):
        zones.setdefault(zone, []).append(node_id)

    zone_params = {
        "toc":   ("1000000", "1ms",  "0ms",  "0"),
        "berth": ("50000",   "15ms", "5ms",  "2"),
        "yard":  ("100000",  "10ms", "3ms",  "1"),
        "gate":  ("1000000", "2ms",  "1ms",  "0.1"),
    }

    for zone in ["toc", "berth", "yard", "gate"]:
        if zone not in zones:
            continue
        rate, delay, jitter, loss = zone_params[zone]
        imp = ZONE_IMPAIRMENTS[zone]
        lines.append(f'echo "── {zone.upper()} zone ({imp["desc"]}) ──"')
        for node_id in zones[zone]:
            container = f"clab-$TOPO_NAME-{node_id}"
            lines.append(f'apply_netem "{container}" {rate} {delay} {jitter} {loss}')
        lines.append('echo ""')
        lines.append('')

    lines.append('echo "All impairments applied."')
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description="Generate port terminal 200-node topology")
    parser.add_argument("--output", type=str,
                        default="topologies/port-terminal-200node.clab.yaml",
                        help="Output topology file path")
    parser.add_argument("--impairment-script", type=str,
                        default="apply-port-terminal-impairments.sh",
                        help="Output impairment script path")
    args = parser.parse_args()

    topology_yaml, node_count, node_zones, topo_name = generate_topology()

    with open(args.output, "w") as f:
        f.write(topology_yaml)

    imp_script = generate_impairment_script(node_zones, topo_name)
    with open(args.impairment_script, "w") as f:
        f.write(imp_script)

    # Count by zone
    zone_counts = {}
    for z in node_zones.values():
        zone_counts[z] = zone_counts.get(z, 0) + 1

    print(f"Generated port terminal topology: {args.output}")
    print(f"  Total nodes: {node_count}")
    for z in ["toc", "berth", "yard", "gate"]:
        print(f"    {z:6s}: {zone_counts.get(z, 0)} nodes")
    print(f"  Impairment script: {args.impairment_script}")
    print(f"  Deploy: containerlab deploy -t {args.output}")


if __name__ == "__main__":
    main()
