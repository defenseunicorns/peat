#!/usr/bin/env python3
"""
Generate ContainerLab topology YAML files for E8 Phase 1
Creates three modes: client-server, hub-spoke, dynamic-mesh
"""

def generate_mode1_client_server():
    """Mode 1: All nodes connect to soldier-1 (central server)"""

    nodes = []

    # Soldier 1 - Server (writer)
    nodes.append("""    soldier-1:
      kind: linux
      image: cap-sim-node:latest
      env:
        NODE_ID: soldier-1
        ROLE: squad_leader
        PLATFORM_TYPE: soldier
        MODE: writer
        TCP_LISTEN: "12345"
        DITTO_APP_ID: ${DITTO_APP_ID}
        DITTO_OFFLINE_TOKEN: ${DITTO_OFFLINE_TOKEN}
        DITTO_SHARED_KEY: ${DITTO_SHARED_KEY}
      ports:
        - "12345:12345"
""")

    # Soldiers 2-9, UGV, UAVs - All connect to soldier-1
    roles = [
        ("soldier-2", "rifleman", "12346"),
        ("soldier-3", "grenadier", "12347"),
        ("soldier-4", "automatic_rifleman", "12348"),
        ("soldier-5", "rifleman", "12349"),
        ("soldier-6", "team_leader", "12350"),
        ("soldier-7", "grenadier", "12351"),
        ("soldier-8", "automatic_rifleman", "12352"),
        ("soldier-9", "rifleman", "12353"),
        ("ugv-1", "resupply_isr", "12354"),
        ("uav-1", "reconnaissance", "12355"),
        ("uav-2", "reconnaissance", "12356"),
    ]

    for node_id, role, port in roles:
        platform = "ugv" if "ugv" in node_id else ("uav" if "uav" in node_id else "soldier")
        nodes.append(f"""    {node_id}:
      kind: linux
      image: cap-sim-node:latest
      env:
        NODE_ID: {node_id}
        ROLE: {role}
        PLATFORM_TYPE: {platform}
        MODE: reader
        TCP_LISTEN: "{port}"
        TCP_CONNECT: "soldier-1:12345"
        DITTO_APP_ID: ${{DITTO_APP_ID}}
        DITTO_OFFLINE_TOKEN: ${{DITTO_OFFLINE_TOKEN}}
        DITTO_SHARED_KEY: ${{DITTO_SHARED_KEY}}
""")

    header = """# ContainerLab Topology: 12-Node Squad - Mode 1 (Client-Server)
#
# Purpose: Simple validation - all nodes connect to central server
# Tests: Basic sync, infrastructure, baseline metrics
#
# Network: All nodes → soldier-1 (star topology)
# Use: make sim-deploy-squad-simple

name: cap-squad-client-server

topology:
  nodes:
"""

    return header + "\n".join(nodes)


def generate_mode2_hub_spoke():
    """Mode 2: Hierarchical hub-spoke with squad/team leaders"""

    nodes = []

    # Soldier 1 - Squad Leader (hub)
    nodes.append("""    soldier-1:
      kind: linux
      image: cap-sim-node:latest
      env:
        NODE_ID: soldier-1
        ROLE: squad_leader
        PLATFORM_TYPE: soldier
        MODE: writer
        TCP_LISTEN: "12345"
        DITTO_APP_ID: ${DITTO_APP_ID}
        DITTO_OFFLINE_TOKEN: ${DITTO_OFFLINE_TOKEN}
        DITTO_SHARED_KEY: ${DITTO_SHARED_KEY}
      ports:
        - "12345:12345"
""")

    # Fire Team 1 (soldiers 2-5) - Connect to squad leader
    team1 = [
        ("soldier-2", "rifleman", "12346"),
        ("soldier-3", "grenadier", "12347"),
        ("soldier-4", "automatic_rifleman", "12348"),
        ("soldier-5", "rifleman", "12349"),
    ]

    for node_id, role, port in team1:
        nodes.append(f"""    {node_id}:
      kind: linux
      image: cap-sim-node:latest
      env:
        NODE_ID: {node_id}
        ROLE: {role}
        PLATFORM_TYPE: soldier
        MODE: reader
        TCP_LISTEN: "{port}"
        TCP_CONNECT: "soldier-1:12345"
        DITTO_APP_ID: ${{DITTO_APP_ID}}
        DITTO_OFFLINE_TOKEN: ${{DITTO_OFFLINE_TOKEN}}
        DITTO_SHARED_KEY: ${{DITTO_SHARED_KEY}}
""")

    # Soldier 6 - Team Leader 2 (sub-hub)
    nodes.append("""    soldier-6:
      kind: linux
      image: cap-sim-node:latest
      env:
        NODE_ID: soldier-6
        ROLE: team_leader
        PLATFORM_TYPE: soldier
        MODE: reader
        TCP_LISTEN: "12350"
        TCP_CONNECT: "soldier-1:12345"
        DITTO_APP_ID: ${DITTO_APP_ID}
        DITTO_OFFLINE_TOKEN: ${DITTO_OFFLINE_TOKEN}
        DITTO_SHARED_KEY: ${DITTO_SHARED_KEY}
      ports:
        - "12350:12350"
""")

    # Fire Team 2 (soldiers 7-9) - Connect to team leader
    team2 = [
        ("soldier-7", "grenadier", "12351"),
        ("soldier-8", "automatic_rifleman", "12352"),
        ("soldier-9", "rifleman", "12353"),
    ]

    for node_id, role, port in team2:
        nodes.append(f"""    {node_id}:
      kind: linux
      image: cap-sim-node:latest
      env:
        NODE_ID: {node_id}
        ROLE: {role}
        PLATFORM_TYPE: soldier
        MODE: reader
        TCP_LISTEN: "{port}"
        TCP_CONNECT: "soldier-6:12350"
        DITTO_APP_ID: ${{DITTO_APP_ID}}
        DITTO_OFFLINE_TOKEN: ${{DITTO_OFFLINE_TOKEN}}
        DITTO_SHARED_KEY: ${{DITTO_SHARED_KEY}}
""")

    # UGV - Connects to both leaders (relay)
    nodes.append("""    ugv-1:
      kind: linux
      image: cap-sim-node:latest
      env:
        NODE_ID: ugv-1
        ROLE: resupply_isr
        PLATFORM_TYPE: ugv
        MODE: reader
        TCP_LISTEN: "12354"
        TCP_CONNECT: "soldier-1:12345,soldier-6:12350"
        DITTO_APP_ID: ${DITTO_APP_ID}
        DITTO_OFFLINE_TOKEN: ${DITTO_OFFLINE_TOKEN}
        DITTO_SHARED_KEY: ${DITTO_SHARED_KEY}
""")

    # UAVs - Connect to leaders
    nodes.append("""    uav-1:
      kind: linux
      image: cap-sim-node:latest
      env:
        NODE_ID: uav-1
        ROLE: reconnaissance
        PLATFORM_TYPE: uav
        MODE: reader
        TCP_LISTEN: "12355"
        TCP_CONNECT: "soldier-1:12345"
        DITTO_APP_ID: ${DITTO_APP_ID}
        DITTO_OFFLINE_TOKEN: ${DITTO_OFFLINE_TOKEN}
        DITTO_SHARED_KEY: ${DITTO_SHARED_KEY}
""")

    nodes.append("""    uav-2:
      kind: linux
      image: cap-sim-node:latest
      env:
        NODE_ID: uav-2
        ROLE: reconnaissance
        PLATFORM_TYPE: uav
        MODE: reader
        TCP_LISTEN: "12356"
        TCP_CONNECT: "soldier-6:12350"
        DITTO_APP_ID: ${DITTO_APP_ID}
        DITTO_OFFLINE_TOKEN: ${DITTO_OFFLINE_TOKEN}
        DITTO_SHARED_KEY: ${DITTO_SHARED_KEY}
""")

    header = """# ContainerLab Topology: 12-Node Squad - Mode 2 (Hub-Spoke)
#
# Purpose: Realistic hierarchical structure with squad/team leaders
# Tests: Hierarchical sync, O(n log n) messaging, relay behavior
#
# Network Topology:
#   Squad Leader (soldier-1)
#     ├─ Fire Team 1 (soldiers 2-5)
#     ├─ Team Leader 2 (soldier-6)
#     │   └─ Fire Team 2 (soldiers 7-9)
#     ├─ UGV (connects to both leaders - relay)
#     └─ UAV-1
#   Team Leader 2
#     └─ UAV-2
#
# Use: make sim-deploy-squad-hierarchical

name: cap-squad-hub-spoke

topology:
  nodes:
"""

    return header + "\n".join(nodes)


def generate_mode3_dynamic_mesh():
    """Mode 3: All nodes know all peers, Ditto forms mesh dynamically"""

    # Build peer list (all nodes)
    all_peers = [f"soldier-{i}:1234{4+i}" for i in range(1, 10)]
    all_peers.extend(["ugv-1:12354", "uav-1:12355", "uav-2:12356"])
    peer_string = ",".join(all_peers)

    nodes = []

    # All nodes get the same configuration
    all_nodes = [
        ("soldier-1", "squad_leader", "soldier", "12345", "writer"),
        ("soldier-2", "rifleman", "soldier", "12346", "reader"),
        ("soldier-3", "grenadier", "soldier", "12347", "reader"),
        ("soldier-4", "automatic_rifleman", "soldier", "12348", "reader"),
        ("soldier-5", "rifleman", "soldier", "12349", "reader"),
        ("soldier-6", "team_leader", "soldier", "12350", "reader"),
        ("soldier-7", "grenadier", "soldier", "12351", "reader"),
        ("soldier-8", "automatic_rifleman", "soldier", "12352", "reader"),
        ("soldier-9", "rifleman", "soldier", "12353", "reader"),
        ("ugv-1", "resupply_isr", "ugv", "12354", "reader"),
        ("uav-1", "reconnaissance", "uav", "12355", "reader"),
        ("uav-2", "reconnaissance", "uav", "12356", "reader"),
    ]

    for node_id, role, platform, port, mode in all_nodes:
        # Each node connects to ALL other nodes
        # Ditto will manage the mesh automatically
        nodes.append(f"""    {node_id}:
      kind: linux
      image: cap-sim-node:latest
      env:
        NODE_ID: {node_id}
        ROLE: {role}
        PLATFORM_TYPE: {platform}
        MODE: {mode}
        TCP_LISTEN: "{port}"
        TCP_CONNECT: "{peer_string}"
        DITTO_APP_ID: ${{DITTO_APP_ID}}
        DITTO_OFFLINE_TOKEN: ${{DITTO_OFFLINE_TOKEN}}
        DITTO_SHARED_KEY: ${{DITTO_SHARED_KEY}}
""")

    header = """# ContainerLab Topology: 12-Node Squad - Mode 3 (Dynamic Mesh)
#
# Purpose: Autonomous peer discovery - all nodes know all peers
# Tests: Dynamic mesh formation, partition/heal, full connectivity
#
# Network: Each node configured with all potential peers
# Ditto: Automatically forms mesh based on reachability
#
# How it works:
#   - All nodes get same peer list (all 12 endpoints)
#   - Each node tries to connect to all listed peers
#   - Ditto manages actual mesh topology dynamically
#   - Network constraints limit effective connectivity
#
# Use: make sim-deploy-squad-dynamic

name: cap-squad-dynamic-mesh

topology:
  nodes:
"""

    return header + "\n".join(nodes)


if __name__ == "__main__":
    import sys

    topologies = {
        "client-server": generate_mode1_client_server(),
        "hub-spoke": generate_mode2_hub_spoke(),
        "dynamic-mesh": generate_mode3_dynamic_mesh(),
    }

    if len(sys.argv) > 1:
        mode = sys.argv[1]
        if mode in topologies:
            print(topologies[mode])
        else:
            print(f"Unknown mode: {mode}")
            print(f"Available: {', '.join(topologies.keys())}")
            sys.exit(1)
    else:
        # Generate all three
        for mode, content in topologies.items():
            filename = f"topologies/squad-12node-{mode}.yaml"
            with open(filename, "w") as f:
                f.write(content)
            print(f"✓ Generated {filename}")
