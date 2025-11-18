#!/usr/bin/env python3
"""
Generate a 384-node Traditional Baseline topology for Containerlab.

Pattern: 1 battalion-hq + 383 soldiers in client-server topology
"""

def generate_topology():
    """Generate 384-node traditional battalion topology."""

    lines = [
        "name: traditional-battalion-384node",
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

    # Generate 383 soldier nodes (p1-soldier-1 through p1-soldier-383)
    for i in range(1, 384):
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

    output_file = "topologies/traditional-battalion-384node.yaml"
    with open(output_file, "w") as f:
        f.write(topology)

    print(f"✅ Generated {output_file}")
    print(f"   Total nodes: 384 (1 commander + 383 soldiers)")
    print(f"   File size: {len(topology)} bytes")
    print(f"   Lines: {topology.count(chr(10))}")
