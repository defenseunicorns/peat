#!/usr/bin/env python3
"""
Generate Lab 4 Hierarchical Topologies

Creates multi-tier HIVE hierarchical topologies for Lab 4 testing:
- Squad level (soldiers + squad leader)
- Platoon level (squads + platoon leader)
- Company level (platoons + company commander)
- Battalion level (companies + battalion HQ)

Usage:
    # 24-node platoon (3 squads × 7 soldiers)
    python3 generate-lab4-hierarchical-topology.py --nodes 24 --bandwidth 1gbps

    # 384-node multi-company (3 companies × 4 platoons × 4 squads × 8 soldiers)
    python3 generate-lab4-hierarchical-topology.py --nodes 384 --bandwidth 100mbps

    # 1000-node battalion
    python3 generate-lab4-hierarchical-topology.py --nodes 1000 --bandwidth 1mbps
"""

import argparse
import sys
import math


def get_credential_env_vars(backend):
    """Return the credential environment variables based on backend."""
    if backend == "automerge":
        return [
            "        DITTO_APP_ID: test-formation",
            "        HIVE_SECRET_KEY: aGl2ZS10ZXN0LWZvcm1hdGlvbi1zZWNyZXQta2V5LTA=",  # base64 of "hive-test-formation-secret-key-0" (32 bytes)
        ]
    else:
        return [
            "        DITTO_APP_ID: ${DITTO_APP_ID}",
            "        DITTO_OFFLINE_TOKEN: ${DITTO_OFFLINE_TOKEN}",
            "        DITTO_SHARED_KEY: ${DITTO_SHARED_KEY}",
        ]


def calculate_hierarchy(total_nodes):
    """
    Calculate optimal hierarchy structure for given node count.

    Returns: (soldiers_per_squad, squads_per_platoon, platoons_per_company, companies)
    """
    if total_nodes <= 24:
        # Small: 1 platoon, 3 squads, 7 soldiers each = 21 + 3 leaders + 1 platoon = 25 (close to 24)
        return (7, 3, 1, 1)
    elif total_nodes <= 48:
        # Medium: 2 platoons, 3 squads each, 7 soldiers = 42 + 6 + 2 = 50 (close to 48)
        return (7, 3, 2, 1)
    elif total_nodes <= 96:
        # Large: 4 platoons, 3 squads each, 7 soldiers = 84 + 12 + 4 = 100 (close to 96)
        return (7, 3, 4, 1)
    elif total_nodes <= 384:
        # Multi-company: 3 companies, 4 platoons, 4 squads, 8 soldiers
        # 3 × 4 × 4 × 8 = 384 soldiers + 48 squad leaders + 12 platoon leaders + 3 company commanders = 447
        # Adjust: 8 soldiers × 4 squads × 4 platoons × 3 companies = 384 (just soldiers)
        return (8, 4, 4, 3)
    else:
        # Battalion: Need to stay under ~1000 TOTAL nodes (including leaders)
        # 6 companies × 4 platoons × 4 squads × 8 soldiers = 768 soldiers
        # + 96 squad leaders + 24 platoon leaders + 6 company commanders = 894 total
        # This stays safely under the 1024 network interface limit
        return (8, 4, 4, 6)


def get_tcp_connect(node_name, parent_name, parent_port, name, backend):
    """Generate TCP_CONNECT env var for automerge backend.

    Docker's embedded DNS resolves container names on user-defined networks.
    Containerlab container names are: clab-{topology-name}-{node-name}
    Format for automerge: "peer_name|hostname:port"
    """
    if backend != "automerge":
        return []

    # For automerge, we need TCP_CONNECT to connect to parent
    if parent_name is None:
        return []  # Company commander has no parent

    # Use full container name as hostname (Docker DNS resolves this)
    container_name = f"clab-{name}-{parent_name}"
    return [f"        TCP_CONNECT: \"{parent_name}|{container_name}:{parent_port}\""]


def generate_lab4_topology(name, total_nodes, bandwidth, backend="ditto"):
    """Generate hierarchical topology for Lab 4."""

    soldiers_per_squad, squads_per_platoon, platoons_per_company, num_companies = calculate_hierarchy(total_nodes)

    # Determine MODE and BACKEND based on backend parameter
    # MODE controls the simulation behavior (writer/reader/hierarchical/flat_mesh)
    # BACKEND controls which CRDT backend to use (ditto/automerge)
    # For Lab 4, we always use "hierarchical" mode regardless of backend
    mode = "hierarchical"
    backend_env = backend  # "automerge" or "ditto"

    # Calculate actual node count
    soldiers = soldiers_per_squad * squads_per_platoon * platoons_per_company * num_companies
    squad_leaders = squads_per_platoon * platoons_per_company * num_companies
    platoon_leaders = platoons_per_company * num_companies
    company_commanders = num_companies

    actual_total = soldiers + squad_leaders + platoon_leaders + company_commanders

    # Determine subnet size based on node count
    # /24 = 254 hosts, /20 = 4094 hosts, /16 = 65534 hosts
    if actual_total > 4000:
        subnet = "172.30.0.0/16"
        ipv6_subnet = "3fff:172:30::/48"
    elif actual_total > 250:
        subnet = "172.30.0.0/20"
        ipv6_subnet = "3fff:172:30::/52"
    else:
        subnet = "172.20.20.0/24"
        ipv6_subnet = "3fff:172:20:20::/64"

    lines = [
        f"# Lab 4: Hierarchical HIVE CRDT - {name}",
        f"# Target nodes: {total_nodes}, Actual: {actual_total}",
        f"# Structure: {num_companies} companies × {platoons_per_company} platoons × {squads_per_platoon} squads × {soldiers_per_squad} soldiers",
        f"# Bandwidth: {bandwidth}",
        "",
        f"name: {name}",
        "",
        "mgmt:",
        f"  network: {name}",
        f"  ipv4-subnet: {subnet}",
        f"  ipv6-subnet: {ipv6_subnet}",
        "",
        "topology:",
        "  nodes:",
        ""
    ]

    # Track port assignments for TCP_CONNECT
    node_ports = {}  # node_id -> port
    node_counter = 0

    # Generate hierarchy - first pass: assign ports
    for company_idx in range(1, num_companies + 1):
        company_id = f"company-{company_idx}"
        commander_id = f"{company_id}-commander"
        node_ports[commander_id] = 12345  # Company commanders all on 12345

        for platoon_idx in range(1, platoons_per_company + 1):
            platoon_id = f"{company_id}-platoon-{platoon_idx}"
            leader_id = f"{platoon_id}-leader"
            node_ports[leader_id] = 12346 + node_counter
            node_counter += 1

            for squad_idx in range(1, squads_per_platoon + 1):
                squad_id = f"{platoon_id}-squad-{squad_idx}"
                squad_leader_id = f"{squad_id}-leader"
                node_ports[squad_leader_id] = 12346 + node_counter
                node_counter += 1

                for soldier_idx in range(1, soldiers_per_squad + 1):
                    soldier_id = f"{squad_id}-soldier-{soldier_idx}"
                    node_ports[soldier_id] = 12346 + node_counter
                    node_counter += 1

    # Reset counter for second pass
    node_counter = 0

    # Generate hierarchy - second pass: generate YAML with TCP_CONNECT
    for company_idx in range(1, num_companies + 1):
        company_id = f"company-{company_idx}"
        commander_id = f"{company_id}-commander"

        lines.append(f"    # ===== COMPANY {company_idx} =====")
        lines.append("")

        # Company commander (no parent to connect to)
        cred_vars = get_credential_env_vars(backend)
        tcp_connect = get_tcp_connect(commander_id, None, None, name, backend)
        lines.extend([
            f"    {commander_id}:",
            "      kind: linux",
            "      image: hive-sim-node:latest",
            "      env:",
            f"        NODE_ID: {commander_id}",
            "        ROLE: company_commander",
            "        PLATFORM_TYPE: soldier",
            "        NODE_TYPE: soldier",
            f"        MODE: {mode}",
            f"        BACKEND: {backend_env}",
            f"        COMPANY_ID: {company_id}",
            "        UPDATE_RATE_MS: \"5000\"",
            f"        TCP_LISTEN: \"{node_ports[commander_id]}\"",
        ] + tcp_connect + cred_vars + [""])

        for platoon_idx in range(1, platoons_per_company + 1):
            platoon_id = f"{company_id}-platoon-{platoon_idx}"
            leader_id = f"{platoon_id}-leader"

            lines.append(f"    # ----- Platoon {company_idx}-{platoon_idx} -----")
            lines.append("")

            # Platoon leader connects to company commander
            tcp_connect = get_tcp_connect(leader_id, commander_id, node_ports[commander_id], name, backend)
            lines.extend([
                f"    {leader_id}:",
                "      kind: linux",
                "      image: hive-sim-node:latest",
                "      env:",
                f"        NODE_ID: {leader_id}",
                "        ROLE: platoon_leader",
                "        PLATFORM_TYPE: soldier",
                "        NODE_TYPE: soldier",
                f"        MODE: {mode}",
                f"        BACKEND: {backend_env}",
                f"        PLATOON_ID: {platoon_id}",
                "        UPDATE_RATE_MS: \"5000\"",
                f"        TCP_LISTEN: \"{node_ports[leader_id]}\"",
            ] + tcp_connect + cred_vars + [""])
            node_counter += 1

            for squad_idx in range(1, squads_per_platoon + 1):
                squad_id = f"{platoon_id}-squad-{squad_idx}"
                squad_leader_id = f"{squad_id}-leader"

                # Squad leader
                squad_members = []
                for soldier_idx in range(1, soldiers_per_squad + 1):
                    soldier_id = f"{squad_id}-soldier-{soldier_idx}"
                    squad_members.append(soldier_id)

                # Squad leader connects to platoon leader
                tcp_connect = get_tcp_connect(squad_leader_id, leader_id, node_ports[leader_id], name, backend)
                lines.extend([
                    f"    {squad_leader_id}:",
                    "      kind: linux",
                    "      image: hive-sim-node:latest",
                    "      env:",
                    f"        NODE_ID: {squad_leader_id}",
                    "        ROLE: squad_leader",
                    "        PLATFORM_TYPE: soldier",
                    "        NODE_TYPE: soldier",
                    f"        MODE: {mode}",
                    f"        BACKEND: {backend_env}",
                    f"        SQUAD_ID: {squad_id}",
                    f"        SQUAD_MEMBERS: \"{','.join(squad_members)}\"",
                    "        UPDATE_RATE_MS: \"5000\"",
                    f"        TCP_LISTEN: \"{node_ports[squad_leader_id]}\"",
                ] + tcp_connect + cred_vars + [""])
                node_counter += 1

                # Squad soldiers
                for soldier_idx in range(1, soldiers_per_squad + 1):
                    soldier_id = f"{squad_id}-soldier-{soldier_idx}"

                    # Soldiers connect to squad leader
                    tcp_connect = get_tcp_connect(soldier_id, squad_leader_id, node_ports[squad_leader_id], name, backend)
                    lines.extend([
                        f"    {soldier_id}:",
                        "      kind: linux",
                        "      image: hive-sim-node:latest",
                        "      env:",
                        f"        NODE_ID: {soldier_id}",
                        "        ROLE: soldier",
                        "        PLATFORM_TYPE: soldier",
                        "        NODE_TYPE: soldier",
                        f"        MODE: {mode}",
                        f"        BACKEND: {backend_env}",
                        f"        SQUAD_ID: {squad_id}",
                        "        UPDATE_RATE_MS: \"5000\"",
                        f"        TCP_LISTEN: \"{node_ports[soldier_id]}\"",
                    ] + tcp_connect + cred_vars + [""])
                    node_counter += 1

        lines.append("")

    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(
        description="Generate Lab 4 hierarchical HIVE CRDT topologies"
    )
    parser.add_argument(
        "--nodes",
        type=int,
        required=True,
        help="Target number of nodes (24, 48, 96, 384, 1000)"
    )
    parser.add_argument(
        "--bandwidth",
        type=str,
        required=True,
        help="Bandwidth constraint (1gbps, 100mbps, 1mbps, 256kbps)"
    )
    parser.add_argument(
        "--output",
        type=str,
        help="Output file path (default: topologies/lab4-hierarchical-{nodes}n-{bandwidth}.yaml)"
    )
    parser.add_argument(
        "--backend",
        type=str,
        choices=["ditto", "automerge"],
        default="ditto",
        help="Backend type: ditto (default) or automerge"
    )
    parser.add_argument(
        "--name",
        type=str,
        help="Topology name (used for container DNS). Keep under 20 chars for DNS compatibility."
    )

    args = parser.parse_args()

    # Generate name based on backend - keep short for DNS label limit (63 chars)
    # Container names will be: clab-{name}-{node-id}
    # Longest node-id is ~40 chars: company-1-platoon-1-squad-1-soldier-1
    # So name should be ~18 chars max to stay under 63
    if args.name:
        name = args.name
    elif args.backend == "automerge":
        name = f"am{args.nodes}n"  # e.g. "am24n" instead of "lab4-automerge-24n-1gbps"
    else:
        name = f"d{args.nodes}n"  # e.g. "d24n" for ditto

    # Generate topology
    topology = generate_lab4_topology(name, args.nodes, args.bandwidth, args.backend)

    # Determine output path
    if args.output:
        output_path = args.output
    else:
        output_path = f"topologies/{name}.yaml"

    # Write topology
    with open(output_path, 'w') as f:
        f.write(topology)

    print(f"✅ Generated Lab 4 topology: {output_path}")
    print(f"   Target nodes: {args.nodes}")
    print(f"   Bandwidth: {args.bandwidth}")
    print(f"   Deployment: containerlab deploy -t {output_path}")


if __name__ == "__main__":
    main()
