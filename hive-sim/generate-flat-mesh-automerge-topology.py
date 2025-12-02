#!/usr/bin/env python3
"""
Generate Flat P2P Mesh Topology with AutomergeIroh CRDT (Lab 3b-automerge)

All nodes are at the same tier (squad members) with full mesh connectivity
and AutomergeIroh CRDT synchronization.

This is the AutomergeIroh variant of generate-flat-mesh-hive-topology.py for
backend comparison testing.
"""

import sys
import yaml

def generate_flat_mesh_automerge(node_count, bandwidth, test_name):
    """
    Generate flat mesh P2P topology with AutomergeIroh CRDT.

    All nodes are equal peers (no hierarchy) using AutomergeIroh CRDT sync.
    No Ditto credentials required - uses local P2P discovery.
    """

    topology = {
        'name': test_name,
        'topology': {
            'nodes': {},
            'links': []
        }
    }

    # Build peer list for full mesh
    peer_ids = [f'peer-{i}' for i in range(1, node_count + 1)]

    # Create peer nodes (all at same tier)
    for i, peer_id in enumerate(peer_ids, 1):
        # Build TCP_CONNECT string (all other peers)
        other_peers = [f'{p}:{12345 + j}' for j, p in enumerate(peer_ids, 1) if p != peer_id]

        topology['topology']['nodes'][peer_id] = {
            'kind': 'linux',
            'image': 'hive-sim-node:latest',
            'env': {
                'NODE_ID': peer_id,
                'ROLE': 'squad_member',  # All same tier
                'PLATFORM_TYPE': 'soldier',
                'NODE_TYPE': 'soldier',
                'MODE': 'flat_mesh',  # Lab 3b: Flat mesh with CRDT
                'SQUAD_ID': 'flat-mesh',  # All in same logical group
                'TCP_LISTEN': str(12345 + i),
                'TCP_CONNECT': ','.join(other_peers),
                'UPDATE_RATE_MS': '5000',
                'BANDWIDTH': bandwidth,
                'BACKEND': 'automerge',  # Use AutomergeIroh CRDT backend
                # AutomergeIroh doesn't need Ditto credentials
                # Data persistence path for AutomergeIroh
                'CAP_DATA_PATH': f'/data/automerge/{peer_id}',
                'CAP_FILTER_ENABLED': 'false'  # No filtering - flat mesh
            }
        }

    # Create full mesh links
    link_id = 1
    for i in range(len(peer_ids)):
        for j in range(i + 1, len(peer_ids)):
            peer_a = peer_ids[i]
            peer_b = peer_ids[j]

            topology['topology']['links'].append({
                'endpoints': [
                    f'{peer_a}:eth{link_id}',
                    f'{peer_b}:eth{link_id}'
                ]
            })
            link_id += 1

    return topology

if __name__ == "__main__":
    if len(sys.argv) < 4:
        print("Usage: python3 generate-flat-mesh-automerge-topology.py <node_count> <bandwidth> <output_file>")
        sys.exit(1)

    node_count = int(sys.argv[1])
    bandwidth = sys.argv[2]
    output_file = sys.argv[3]

    test_name = f'automerge-flat-mesh-{node_count}n-{bandwidth}'

    topology = generate_flat_mesh_automerge(node_count, bandwidth, test_name)

    with open(output_file, 'w') as f:
        yaml.dump(topology, f, default_flow_style=False, sort_keys=False)

    connections = node_count * (node_count - 1) // 2
    print(f"Generated {output_file}: {node_count} peers (flat mesh with AutomergeIroh CRDT), {connections} links")
