#!/usr/bin/env python3
"""
Generate Flat P2P Mesh Topologies for Lab 3b Testing

Creates full-mesh topologies where all nodes are equal peers with no hierarchy.
Used for Lab 3b experiments comparing flat mesh vs hierarchical architectures.

This generator is backend-agnostic - use the --backend flag or set BACKEND env var.

Usage:
    # Generate 24-node flat mesh for Ditto backend
    python3 generate-flat-mesh-topology.py --nodes 24 --bandwidth 1gbps --backend ditto

    # Generate 50-node flat mesh for Automerge backend
    python3 generate-flat-mesh-topology.py --nodes 50 --bandwidth 100mbps --backend automerge

    # Backend-agnostic (uses ${BACKEND} substitution at deploy time)
    python3 generate-flat-mesh-topology.py --nodes 24 --bandwidth 1gbps

Note: Flat mesh has O(n^2) connection overhead. Lab 3b demonstrated this becomes
      impractical beyond ~50 nodes. For larger deployments, use the hierarchical
      generator (generate-lab4-hierarchical-topology.py) instead.
"""

import argparse
import sys
import yaml


def get_credential_env_vars(backend):
    """Return credential environment variables based on backend.

    Args:
        backend: 'ditto', 'automerge', or None for backend-agnostic

    Returns:
        Dict of credential env vars
    """
    if backend == 'ditto':
        return {
            'HIVE_APP_ID': '${HIVE_APP_ID}',
            'HIVE_OFFLINE_TOKEN': '${HIVE_OFFLINE_TOKEN}',
            'HIVE_SHARED_KEY': '${HIVE_SHARED_KEY}',
        }
    elif backend == 'automerge':
        return {
            'HIVE_SECRET_KEY': '${HIVE_SECRET_KEY}',
        }
    else:
        # Backend-agnostic: include all credentials, backend ignores irrelevant ones
        return {
            'HIVE_APP_ID': '${HIVE_APP_ID}',
            'HIVE_SECRET_KEY': '${HIVE_SECRET_KEY}',
            'HIVE_OFFLINE_TOKEN': '${HIVE_OFFLINE_TOKEN}',
            'HIVE_SHARED_KEY': '${HIVE_SHARED_KEY}',
        }


def generate_flat_mesh_topology(node_count, bandwidth, topology_name, backend=None):
    """Generate a flat P2P mesh topology for containerlab.

    All nodes are equal peers with full mesh connectivity. Each node connects
    to every other node, creating O(n^2) connections.

    Args:
        node_count: Number of peer nodes
        bandwidth: Bandwidth constraint string (e.g., '1gbps', '100mbps')
        topology_name: Name for the containerlab topology
        backend: 'ditto', 'automerge', or None for backend-agnostic

    Returns:
        Dict representing the containerlab topology YAML structure
    """
    topology = {
        'name': topology_name,
        'topology': {
            'nodes': {},
            'links': []
        }
    }

    # Determine backend value for env var
    if backend:
        backend_env = backend
    else:
        backend_env = '${BACKEND}'  # Shell variable substitution at deploy time

    # Build peer list
    peer_ids = [f'peer-{i}' for i in range(1, node_count + 1)]

    # Create peer nodes (all at same tier - no hierarchy)
    for i, peer_id in enumerate(peer_ids, 1):
        port = 12345 + i

        # Build TCP_CONNECT string to all other peers
        # Uses containerlab DNS format: clab-{topology}-{node}:{port}
        other_peers = []
        for j, other_peer in enumerate(peer_ids, 1):
            if other_peer != peer_id:
                container_name = f'clab-{topology_name}-{other_peer}'
                other_peers.append(f'{container_name}:{12345 + j}')

        # Base environment variables
        env = {
            'NODE_ID': peer_id,
            'ROLE': 'squad_member',
            'PLATFORM_TYPE': 'soldier',
            'NODE_TYPE': 'soldier',
            'MODE': 'flat_mesh',
            'BACKEND': backend_env,
            'SQUAD_ID': 'flat-mesh',
            'TCP_LISTEN': str(port),
            'TCP_CONNECT': ','.join(other_peers),
            'UPDATE_RATE_MS': '5000',
            'BANDWIDTH': bandwidth,
        }

        # Add credential env vars
        env.update(get_credential_env_vars(backend))

        topology['topology']['nodes'][peer_id] = {
            'kind': 'linux',
            'image': 'hive-sim-node:latest',
            'env': env
        }

    # Create full mesh links (for bandwidth constraints)
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


def main():
    parser = argparse.ArgumentParser(
        description='Generate flat P2P mesh topologies for Lab 3b testing',
        epilog='''
Examples:
  %(prog)s --nodes 24 --bandwidth 1gbps --backend ditto
  %(prog)s --nodes 50 --bandwidth 100mbps --backend automerge
  %(prog)s --nodes 24 --bandwidth 1gbps  # backend-agnostic
        '''
    )
    parser.add_argument(
        '--nodes', '-n',
        type=int,
        required=True,
        help='Number of peer nodes (recommended max: 50 due to O(n^2) scaling)'
    )
    parser.add_argument(
        '--bandwidth', '-b',
        type=str,
        required=True,
        help='Bandwidth constraint (e.g., 1gbps, 100mbps, 1mbps, 256kbps)'
    )
    parser.add_argument(
        '--backend',
        type=str,
        choices=['ditto', 'automerge'],
        default=None,
        help='CRDT backend (omit for backend-agnostic topology using ${BACKEND})'
    )
    parser.add_argument(
        '--output', '-o',
        type=str,
        help='Output file path (default: topologies/flat-mesh-{nodes}n-{bandwidth}.yaml)'
    )
    parser.add_argument(
        '--name',
        type=str,
        help='Topology name for containerlab (default: flat-mesh-{nodes}n)'
    )

    args = parser.parse_args()

    # Warn about O(n^2) scaling for large node counts
    if args.nodes > 50:
        print(f'Warning: {args.nodes} nodes will create {args.nodes * (args.nodes - 1) // 2} '
              f'connections (O(n^2) scaling).', file=sys.stderr)
        print('Consider using generate-lab4-hierarchical-topology.py for large deployments.',
              file=sys.stderr)

    # Generate topology name
    if args.name:
        topology_name = args.name
    else:
        if args.backend:
            topology_name = f'{args.backend}-flat-mesh-{args.nodes}n'
        else:
            topology_name = f'flat-mesh-{args.nodes}n'

    # Generate topology
    topology = generate_flat_mesh_topology(
        args.nodes,
        args.bandwidth,
        topology_name,
        args.backend
    )

    # Determine output path
    if args.output:
        output_path = args.output
    else:
        output_path = f'topologies/{topology_name}-{args.bandwidth}.yaml'

    # Write topology
    with open(output_path, 'w') as f:
        yaml.dump(topology, f, default_flow_style=False, sort_keys=False)

    # Summary
    connections = args.nodes * (args.nodes - 1) // 2
    backend_str = args.backend if args.backend else 'backend-agnostic'
    print(f'Generated {output_path}')
    print(f'  Nodes: {args.nodes} peers (flat mesh)')
    print(f'  Connections: {connections}')
    print(f'  Bandwidth: {args.bandwidth}')
    print(f'  Backend: {backend_str}')
    print(f'  Deploy: containerlab deploy -t {output_path}')


if __name__ == '__main__':
    main()
