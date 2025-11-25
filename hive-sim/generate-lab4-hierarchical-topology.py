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
        # Battalion: 8 companies, 4 platoons, 4 squads, 8 soldiers
        # 8 × 4 × 4 × 8 = 1024 soldiers
        return (8, 4, 4, 8)


def generate_lab4_topology(name, total_nodes, bandwidth):
    """Generate hierarchical topology for Lab 4."""

    soldiers_per_squad, squads_per_platoon, platoons_per_company, num_companies = calculate_hierarchy(total_nodes)

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

    node_counter = 0

    # Generate hierarchy bottom-up
    for company_idx in range(1, num_companies + 1):
        company_id = f"company-{company_idx}"

        lines.append(f"    # ===== COMPANY {company_idx} =====")
        lines.append("")

        # Company commander
        lines.extend([
            f"    {company_id}-commander:",
            "      kind: linux",
            "      image: hive-sim-node:latest",
            "      env:",
            f"        NODE_ID: {company_id}-commander",
            "        ROLE: company_commander",
            "        PLATFORM_TYPE: soldier",
            "        NODE_TYPE: soldier",
            "        MODE: hierarchical",
            f"        COMPANY_ID: {company_id}",
            "        UPDATE_RATE_MS: \"5000\"",
            "        TCP_LISTEN: \"12345\"",
            "        DITTO_APP_ID: ${DITTO_APP_ID}",
            "        DITTO_OFFLINE_TOKEN: ${DITTO_OFFLINE_TOKEN}",
            "        DITTO_SHARED_KEY: ${DITTO_SHARED_KEY}",
            ""
        ])

        for platoon_idx in range(1, platoons_per_company + 1):
            platoon_id = f"{company_id}-platoon-{platoon_idx}"

            lines.append(f"    # ----- Platoon {company_idx}-{platoon_idx} -----")
            lines.append("")

            # Platoon leader
            lines.extend([
                f"    {platoon_id}-leader:",
                "      kind: linux",
                "      image: hive-sim-node:latest",
                "      env:",
                f"        NODE_ID: {platoon_id}-leader",
                "        ROLE: platoon_leader",
                "        PLATFORM_TYPE: soldier",
                "        NODE_TYPE: soldier",
                "        MODE: hierarchical",
                f"        PLATOON_ID: {platoon_id}",
                "        UPDATE_RATE_MS: \"5000\"",
                f"        TCP_LISTEN: \"{12346 + node_counter}\"",
                "        DITTO_APP_ID: ${DITTO_APP_ID}",
                "        DITTO_OFFLINE_TOKEN: ${DITTO_OFFLINE_TOKEN}",
                "        DITTO_SHARED_KEY: ${DITTO_SHARED_KEY}",
                ""
            ])
            node_counter += 1

            for squad_idx in range(1, squads_per_platoon + 1):
                squad_id = f"{platoon_id}-squad-{squad_idx}"

                # Squad leader
                squad_members = []
                for soldier_idx in range(1, soldiers_per_squad + 1):
                    soldier_id = f"{squad_id}-soldier-{soldier_idx}"
                    squad_members.append(soldier_id)

                lines.extend([
                    f"    {squad_id}-leader:",
                    "      kind: linux",
                    "      image: hive-sim-node:latest",
                    "      env:",
                    f"        NODE_ID: {squad_id}-leader",
                    "        ROLE: squad_leader",
                    "        PLATFORM_TYPE: soldier",
                    "        NODE_TYPE: soldier",
                    "        MODE: hierarchical",
                    f"        SQUAD_ID: {squad_id}",
                    f"        SQUAD_MEMBERS: \"{','.join(squad_members)}\"",
                    "        UPDATE_RATE_MS: \"5000\"",
                    f"        TCP_LISTEN: \"{12346 + node_counter}\"",
                    "        DITTO_APP_ID: ${DITTO_APP_ID}",
                    "        DITTO_OFFLINE_TOKEN: ${DITTO_OFFLINE_TOKEN}",
                    "        DITTO_SHARED_KEY: ${DITTO_SHARED_KEY}",
                    ""
                ])
                node_counter += 1

                # Squad soldiers
                for soldier_idx in range(1, soldiers_per_squad + 1):
                    soldier_id = f"{squad_id}-soldier-{soldier_idx}"

                    lines.extend([
                        f"    {soldier_id}:",
                        "      kind: linux",
                        "      image: hive-sim-node:latest",
                        "      env:",
                        f"        NODE_ID: {soldier_id}",
                        "        ROLE: soldier",
                        "        PLATFORM_TYPE: soldier",
                        "        NODE_TYPE: soldier",
                        "        MODE: hierarchical",
                        f"        SQUAD_ID: {squad_id}",
                        "        UPDATE_RATE_MS: \"5000\"",
                        f"        TCP_LISTEN: \"{12346 + node_counter}\"",
                        "        DITTO_APP_ID: ${DITTO_APP_ID}",
                        "        DITTO_OFFLINE_TOKEN: ${DITTO_OFFLINE_TOKEN}",
                        "        DITTO_SHARED_KEY: ${DITTO_SHARED_KEY}",
                        ""
                    ])
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

    args = parser.parse_args()

    # Generate name
    name = f"lab4-hierarchical-{args.nodes}n-{args.bandwidth}"

    # Generate topology
    topology = generate_lab4_topology(name, args.nodes, args.bandwidth)

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
