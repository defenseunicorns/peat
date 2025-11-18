#!/usr/bin/env python3
"""
Generate a 500-node Traditional Baseline topology for Containerlab.

Pattern: 1 battalion-hq + 499 soldiers in client-server topology
Tests approaching the practical single-machine limit for Containerlab.
"""

def generate_topology():
    """Generate 500-node traditional battalion topology."""

    lines = [
        "name: traditional-battalion-500node",
        "",
        "# Management network configuration - /16 subnet for 500+ node capacity",
        "# IPv6 ENABLED (dual-stack performs 2.7x better than IPv4-only)",
        "# Requires kernel tuning: net.ipv6.neigh.default.gc_thresh3 = 32768",
        "mgmt:",
        "  network: clab",
        "  ipv4-subnet: 172.20.0.0/16  # 65,534 usable IPs (vs 253 for /24)",
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

    # Generate 499 soldier nodes (p1-soldier-1 through p1-soldier-499)
    for i in range(1, 500):
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

    output_file = "topologies/traditional-battalion-500node.yaml"
    with open(output_file, "w") as f:
        f.write(topology)

    print(f"✅ Generated {output_file}")
    print(f"   Total nodes: 500 (1 commander + 499 soldiers)")
    print(f"   File size: {len(topology)} bytes ({len(topology)/1024:.1f} KB)")
    print(f"   Lines: {topology.count(chr(10))}")
    print(f"")
    print(f"⚠️  Note: 500 nodes approaches practical single-machine limits")
    print(f"   Estimated RAM: ~25-30GB")
    print(f"   Recommended: Monitor resources during deployment")
