#!/usr/bin/env python3
"""
Generate battalion-scale CAP topologies (48 and 96 nodes)
Creates both CAP Full (flat) and CAP Hierarchical (Mode 4) variants
"""

def generate_cap_full_battalion(num_nodes, output_file):
    """Generate CAP Full topology - flat client-server, no hierarchy"""

    with open(output_file, 'w') as f:
        f.write(f"""# ContainerLab Topology: {num_nodes}-Node Battalion - CAP Full (No Hierarchy)
# Architecture: All nodes connect to central server (star topology)
# CRDT: Yes (Ditto) | Hierarchy: No | Aggregation: No

name: cap-battalion-{num_nodes}node

topology:
  nodes:
    battalion-hq:
      kind: linux
      image: cap-sim-node:latest
      env:
        NODE_ID: battalion-hq
        ROLE: battalion_commander
        PLATFORM_TYPE: soldier
        NODE_TYPE: soldier
        MODE: writer
        UPDATE_RATE_MS: "5000"
        TCP_LISTEN: "12345"
        DITTO_APP_ID: ${{DITTO_APP_ID}}
        DITTO_OFFLINE_TOKEN: ${{DITTO_OFFLINE_TOKEN}}
        DITTO_SHARED_KEY: ${{DITTO_SHARED_KEY}}
      ports:
        - "12345:12345"

""")

        # Generate flat list of soldiers
        for i in range(1, num_nodes + 1):
            f.write(f"""    soldier-{i}:
      kind: linux
      image: cap-sim-node:latest
      env:
        NODE_ID: soldier-{i}
        ROLE: rifleman
        PLATFORM_TYPE: soldier
        NODE_TYPE: soldier
        MODE: reader
        TCP_CONNECT: "battalion-hq:12345"
        DITTO_APP_ID: ${{DITTO_APP_ID}}
        DITTO_OFFLINE_TOKEN: ${{DITTO_OFFLINE_TOKEN}}
        DITTO_SHARED_KEY: ${{DITTO_SHARED_KEY}}

""")

        # Generate links
        f.write("  links:\n")
        for i in range(1, num_nodes + 1):
            f.write(f"    - endpoints: [battalion-hq:eth{i}, soldier-{i}:eth1]\n")

def generate_cap_hierarchical_battalion(num_nodes, output_file):
    """Generate CAP Hierarchical topology - Mode 4 with aggregation"""

    # Calculate hierarchy: 4 platoons of 12 nodes each for 48
    # or 8 platoons of 12 nodes each for 96
    nodes_per_platoon = 12
    num_platoons = num_nodes // nodes_per_platoon

    with open(output_file, 'w') as f:
        f.write(f"""# ContainerLab Topology: {num_nodes}-Node Battalion - CAP Hierarchical (Mode 4)
# Architecture: Hierarchical with aggregation
# CRDT: Yes (Ditto) | Hierarchy: Yes | Aggregation: Yes (Mode 4)
#
# Structure:
#   Battalion HQ (1) - Aggregates {num_platoons} PlatoonSummaries → 1 BattalionSummary
#   ├── Platoon Leaders ({num_platoons}) - Each aggregates {nodes_per_platoon} NodeStates → 1 PlatoonSummary
#   └── Soldiers ({num_nodes}) - Each reports NodeState
#
# Expected ops: {num_nodes} NodeStates + {num_platoons} PlatoonSummaries + 1 BattalionSummary = {num_nodes + num_platoons + 1} total

name: cap-battalion-{num_nodes}node-mode4

topology:
  nodes:
    battalion-hq:
      kind: linux
      image: cap-sim-node:latest
      env:
        NODE_ID: battalion-hq
        ROLE: battalion_commander
        PLATFORM_TYPE: soldier
        NODE_TYPE: soldier
        MODE: hierarchical
        BATTALION_ID: "battalion-1"
        UPDATE_RATE_MS: "5000"
        TCP_LISTEN: "12345"
        DITTO_APP_ID: ${{DITTO_APP_ID}}
        DITTO_OFFLINE_TOKEN: ${{DITTO_OFFLINE_TOKEN}}
        DITTO_SHARED_KEY: ${{DITTO_SHARED_KEY}}
        CAP_FILTER_ENABLED: "true"
      ports:
        - "12345:12345"

""")

        # Generate platoon leaders
        for p in range(1, num_platoons + 1):
            f.write(f"""    platoon-{p}-leader:
      kind: linux
      image: cap-sim-node:latest
      env:
        NODE_ID: platoon-{p}-leader
        ROLE: platoon_leader
        PLATFORM_TYPE: soldier
        NODE_TYPE: soldier
        MODE: hierarchical
        PLATOON_ID: "platoon-{p}"
        TCP_LISTEN: "{12345 + p}"
        TCP_CONNECT: "battalion-hq:12345"
        DITTO_APP_ID: ${{DITTO_APP_ID}}
        DITTO_OFFLINE_TOKEN: ${{DITTO_OFFLINE_TOKEN}}
        DITTO_SHARED_KEY: ${{DITTO_SHARED_KEY}}
        CAP_FILTER_ENABLED: "true"

""")

        # Generate soldiers for each platoon
        soldier_num = 1
        for p in range(1, num_platoons + 1):
            for s in range(1, nodes_per_platoon + 1):
                f.write(f"""    platoon-{p}-soldier-{s}:
      kind: linux
      image: cap-sim-node:latest
      env:
        NODE_ID: platoon-{p}-soldier-{s}
        ROLE: rifleman
        PLATFORM_TYPE: soldier
        NODE_TYPE: soldier
        MODE: hierarchical
        PLATOON_ID: "platoon-{p}"
        TCP_CONNECT: "platoon-{p}-leader:{12345 + p}"
        DITTO_APP_ID: ${{DITTO_APP_ID}}
        DITTO_OFFLINE_TOKEN: ${{DITTO_OFFLINE_TOKEN}}
        DITTO_SHARED_KEY: ${{DITTO_SHARED_KEY}}
        CAP_FILTER_ENABLED: "true"

""")
                soldier_num += 1

        # Generate links
        f.write("  links:\n")
        # Battalion HQ to platoon leaders
        for p in range(1, num_platoons + 1):
            f.write(f"    - endpoints: [battalion-hq:eth{p}, platoon-{p}-leader:eth1]\n")

        # Platoon leaders to soldiers
        link_num = 1
        for p in range(1, num_platoons + 1):
            for s in range(1, nodes_per_platoon + 1):
                f.write(f"    - endpoints: [platoon-{p}-leader:eth{s+1}, platoon-{p}-soldier-{s}:eth1]\n")
                link_num += 1

if __name__ == "__main__":
    print("Generating battalion-scale CAP topologies...")

    # 48 nodes
    print("  - CAP Full 48 nodes...")
    generate_cap_full_battalion(48, "battalion-48node-client-server.yaml")
    print("  - CAP Hierarchical 48 nodes...")
    generate_cap_hierarchical_battalion(48, "battalion-48node-client-server-mode4.yaml")

    # 96 nodes
    print("  - CAP Full 96 nodes...")
    generate_cap_full_battalion(96, "battalion-96node-client-server.yaml")
    print("  - CAP Hierarchical 96 nodes...")
    generate_cap_hierarchical_battalion(96, "battalion-96node-client-server-mode4.yaml")

    print("Done!")
