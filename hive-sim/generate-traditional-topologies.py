#!/usr/bin/env python3
"""
Generate ContainerLab topology YAML files for Traditional IoT Baseline Testing
Creates topologies for client-server and hub-spoke modes (NO mesh - n-squared problem)
"""

def generate_traditional_client_server():
    """Traditional IoT: Client-Server (all nodes → soldier-1)"""

    nodes = []

    # Soldier 1 - Server (creates documents, broadcasts to all clients)
    nodes.append("""    soldier-1:
      kind: linux
      image: cap-sim-node:latest
      env:
        NODE_ID: soldier-1
        ROLE: squad_leader
        PLATFORM_TYPE: soldier
        MODE: writer
        TCP_LISTEN: "12345"
        USE_TRADITIONAL: "true"
        UPDATE_FREQUENCY_SECS: "5"
        NODE_TYPE: server
        DITTO_APP_ID: ${DITTO_APP_ID}
        DITTO_OFFLINE_TOKEN: ${DITTO_OFFLINE_TOKEN}
        DITTO_SHARED_KEY: ${DITTO_SHARED_KEY}
      ports:
        - "12345:12345"
""")

    # All other nodes - Clients (connect to soldier-1)
    clients = [
        ("soldier-2", "rifleman", "soldier"),
        ("soldier-3", "grenadier", "soldier"),
        ("soldier-4", "automatic_rifleman", "soldier"),
        ("soldier-5", "rifleman", "soldier"),
        ("soldier-6", "team_leader", "soldier"),
        ("soldier-7", "grenadier", "soldier"),
        ("soldier-8", "automatic_rifleman", "soldier"),
        ("soldier-9", "rifleman", "soldier"),
        ("ugv-1", "resupply_isr", "ugv"),
        ("uav-1", "reconnaissance", "uav"),
        ("uav-2", "reconnaissance", "uav"),
    ]

    for node_id, role, platform in clients:
        nodes.append(f"""    {node_id}:
      kind: linux
      image: cap-sim-node:latest
      env:
        NODE_ID: {node_id}
        ROLE: {role}
        PLATFORM_TYPE: {platform}
        MODE: reader
        TCP_CONNECT: "soldier-1:12345"
        USE_TRADITIONAL: "true"
        UPDATE_FREQUENCY_SECS: "5"
        NODE_TYPE: client
        DITTO_APP_ID: ${{DITTO_APP_ID}}
        DITTO_OFFLINE_TOKEN: ${{DITTO_OFFLINE_TOKEN}}
        DITTO_SHARED_KEY: ${{DITTO_SHARED_KEY}}
""")

    header = """# ContainerLab Topology: 12-Node Squad - Traditional IoT Baseline (Client-Server)
#
# Purpose: Traditional IoT architecture comparison (NO CRDT, periodic full messages)
# Architecture: All nodes connect to central server (star topology)
# Transmission: Full state messages every 5 seconds
# Comparison: Baseline for measuring CRDT overhead and CAP filtering benefits
#
# Network: All nodes → soldier-1 (centralized)
# Use: containerlab deploy -t topologies/traditional-squad-client-server.yaml

name: traditional-squad-client-server

topology:
  nodes:
"""

    return header + "\n".join(nodes)


def generate_traditional_hub_spoke():
    """Traditional IoT: Hub-Spoke (hierarchical with team leaders)"""

    nodes = []

    # Soldier 1 - Squad Leader Server (top of hierarchy)
    nodes.append("""    soldier-1:
      kind: linux
      image: cap-sim-node:latest
      env:
        NODE_ID: soldier-1
        ROLE: squad_leader
        PLATFORM_TYPE: soldier
        MODE: writer
        TCP_LISTEN: "12345"
        USE_TRADITIONAL: "true"
        UPDATE_FREQUENCY_SECS: "5"
        NODE_TYPE: server
        DITTO_APP_ID: ${DITTO_APP_ID}
        DITTO_OFFLINE_TOKEN: ${DITTO_OFFLINE_TOKEN}
        DITTO_SHARED_KEY: ${DITTO_SHARED_KEY}
      ports:
        - "12345:12345"
""")

    # Team Leaders - Connect to squad leader, relay to team members
    # In traditional baseline, these are still clients (no relay capability yet)
    # This demonstrates hierarchical network topology, but data flow is still star

    team_leaders = [
        ("soldier-6", "team_leader", "soldier"),
    ]

    for node_id, role, platform in team_leaders:
        nodes.append(f"""    {node_id}:
      kind: linux
      image: cap-sim-node:latest
      env:
        NODE_ID: {node_id}
        ROLE: {role}
        PLATFORM_TYPE: {platform}
        MODE: reader
        TCP_CONNECT: "soldier-1:12345"
        USE_TRADITIONAL: "true"
        UPDATE_FREQUENCY_SECS: "5"
        NODE_TYPE: client
        DITTO_APP_ID: ${{DITTO_APP_ID}}
        DITTO_OFFLINE_TOKEN: ${{DITTO_OFFLINE_TOKEN}}
        DITTO_SHARED_KEY: ${{DITTO_SHARED_KEY}}
""")

    # Team members - All connect to squad leader (simplified hub-spoke)
    team_members = [
        ("soldier-2", "rifleman", "soldier"),
        ("soldier-3", "grenadier", "soldier"),
        ("soldier-4", "automatic_rifleman", "soldier"),
        ("soldier-5", "rifleman", "soldier"),
        ("soldier-7", "grenadier", "soldier"),
        ("soldier-8", "automatic_rifleman", "soldier"),
        ("soldier-9", "rifleman", "soldier"),
        ("ugv-1", "resupply_isr", "ugv"),
        ("uav-1", "reconnaissance", "uav"),
        ("uav-2", "reconnaissance", "uav"),
    ]

    for node_id, role, platform in team_members:
        nodes.append(f"""    {node_id}:
      kind: linux
      image: cap-sim-node:latest
      env:
        NODE_ID: {node_id}
        ROLE: {role}
        PLATFORM_TYPE: {platform}
        MODE: reader
        TCP_CONNECT: "soldier-1:12345"
        USE_TRADITIONAL: "true"
        UPDATE_FREQUENCY_SECS: "5"
        NODE_TYPE: client
        DITTO_APP_ID: ${{DITTO_APP_ID}}
        DITTO_OFFLINE_TOKEN: ${{DITTO_OFFLINE_TOKEN}}
        DITTO_SHARED_KEY: ${{DITTO_SHARED_KEY}}
""")

    header = """# ContainerLab Topology: 12-Node Squad - Traditional IoT Baseline (Hub-Spoke)
#
# Purpose: Traditional IoT architecture with hierarchical topology
# Architecture: Squad leader → Team leaders → Team members
# Note: In this implementation, all nodes still connect to squad leader (star)
#       True multi-hop relay would require additional implementation
# Transmission: Full state messages every 5 seconds
#
# Network: Hierarchical star (potential for future relay implementation)
# Use: containerlab deploy -t topologies/traditional-squad-hub-spoke.yaml

name: traditional-squad-hub-spoke

topology:
  nodes:
"""

    return header + "\n".join(nodes)


def main():
    """Generate all traditional baseline topology files"""

    topologies = [
        ("topologies/traditional-squad-client-server.yaml", generate_traditional_client_server()),
        ("topologies/traditional-squad-hub-spoke.yaml", generate_traditional_hub_spoke()),
    ]

    for filename, content in topologies:
        with open(filename, 'w') as f:
            f.write(content)
        print(f"✓ Generated {filename}")

    print("\nTraditional IoT Baseline topologies generated!")
    print("\nNote: Mesh topology NOT generated (n-squared problem)")
    print("      Traditional architectures use client-server or hub-spoke")


if __name__ == "__main__":
    main()
