#!/usr/bin/env python3
"""
Generate a 750-node Traditional Baseline topology for Containerlab.

Pattern: 1 battalion-hq + 749 soldiers in client-server topology
Tests upper boundary with kernel tuning applied.
"""

def generate_topology():
    """Generate 750-node traditional battalion topology."""

    lines = [
        "name: traditional-battalion-750node",
        "",
        "# Management network configuration - /16 subnet for 750+ node capacity",
        "# IPv6 ENABLED (dual-stack performs 2.7x better than IPv4-only)",
        "# Requires kernel tuning: net.ipv6.neigh.default.gc_thresh3 = 32768",
        "mgmt:",
        "  network: clab",
        "  ipv4-subnet: 172.20.0.0/16  # 65,534 usable IPs",
        "  ipv6-subnet: 3fff:172:20::/48  # Dual-stack for optimal performance",
        "",
        "topology:",
        "  nodes:",
        "    battalion-hq:",
        "      kind: linux",
        "      image: hive-sim-node:latest",
        "      env:",
        "        NODE_ID: battalion-hq",
        "        ROLE: battalion_commander",
        "        PLATFORM_TYPE: soldier",
        "        MODE: writer",
        "        TCP_LISTEN: '12345'",
        "        USE_TRADITIONAL: 'true'",
        "        UPDATE_FREQUENCY_SECS: '0.5'",
    ]

    # Generate 749 soldier nodes (p1-soldier-1 through p1-soldier-749)
    for i in range(1, 750):
        lines.extend([
            f"    p1-soldier-{i}:",
            "      kind: linux",
            "      image: hive-sim-node:latest",
            "      env:",
            f"        NODE_ID: p1-soldier-{i}",
            "        ROLE: soldier",
            "        PLATFORM_TYPE: soldier",
            "        MODE: reader",
            "        TCP_CONNECT: battalion-hq:12345",
            "        USE_TRADITIONAL: 'true'",
            "        UPDATE_FREQUENCY_SECS: '0.5'",
        ])

    return "\n".join(lines) + "\n"

if __name__ == "__main__":
    topology = generate_topology()

    output_file = "topologies/traditional-battalion-750node.yaml"
    with open(output_file, "w") as f:
        f.write(topology)

    print(f"✅ Generated {output_file}")
    print(f"   Total nodes: 750 (1 commander + 749 soldiers)")
    print(f"   File size: {len(topology)} bytes ({len(topology)/1024:.1f} KB)")
    print(f"   Lines: {topology.count(chr(10))}")
    print(f"")
    print(f"🔬 Testing upper boundary with kernel tuning")
    print(f"   Expected RAM: ~11-12GB")
    print(f"   IPv6 neighbors: ~750 / 32,768")
