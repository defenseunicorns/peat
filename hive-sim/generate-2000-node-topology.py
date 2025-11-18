#!/usr/bin/env python3
"""Generate a 2000-node Traditional Baseline topology for Containerlab."""

def generate_topology(node_count=2000):
    lines = [
        f"name: traditional-battalion-{node_count}node",
        "",
        "# 2000-node topology - extreme upper boundary test",
        "mgmt:",
        "  network: clab",
        "  ipv4-subnet: 172.20.0.0/16",
        "  ipv6-subnet: 3fff:172:20::/48",
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
    for i in range(1, node_count):
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
    topology = generate_topology(2000)
    output_file = "topologies/traditional-battalion-2000node.yaml"
    with open(output_file, "w") as f:
        f.write(topology)
    print(f"✅ Generated {output_file} ({len(topology)/1024:.1f} KB, {topology.count(chr(10))} lines)")
