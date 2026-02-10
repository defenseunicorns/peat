import { useMemo } from 'react';
import type { HierarchyNode, ViewerHierarchyLevel, YardSummary, CranePosition } from '../../wire-types';

// -- Colours per level ------------------------------------------------------

const LEVEL_COLORS: Record<ViewerHierarchyLevel, string> = {
  H4: '#ff4444', // TOC — red
  H3: '#ffaa00', // Yard Manager — amber
  H2: '#44bbff', // Yard Block — blue
  H1: '#888888', // Equipment — grey
};

const STATUS_BADGE: Record<string, string> = {
  active: '#44ff44',
  degraded: '#ffaa00',
  offline: '#ff4444',
};

// -- Props ------------------------------------------------------------------

interface HierarchyTreeProps {
  nodes: HierarchyNode[];
  yardSummaries?: Record<string, YardSummary>;
  cranePositions?: Record<string, CranePosition>;
  selectedId?: string;
  onSelect?: (id: string) => void;
}

// -- Component --------------------------------------------------------------

export default function HierarchyTree({
  nodes,
  yardSummaries = {},
  cranePositions = {},
  selectedId,
  onSelect,
}: HierarchyTreeProps) {
  // Build a lookup and find root nodes
  const { lookup, roots } = useMemo(() => {
    const lk = new Map<string, HierarchyNode>();
    for (const n of nodes) lk.set(n.id, n);

    const rts = nodes.filter((n) => !n.parentId);
    // Sort roots by hierarchy level descending (H4 first)
    rts.sort((a, b) => levelOrd(b.level) - levelOrd(a.level));
    return { lookup: lk, roots: rts };
  }, [nodes]);

  return (
    <div style={{ fontFamily: 'monospace', fontSize: 13, color: '#ccc' }}>
      {roots.map((r) => (
        <TreeNode
          key={r.id}
          node={r}
          lookup={lookup}
          depth={0}
          yardSummaries={yardSummaries}
          cranePositions={cranePositions}
          selectedId={selectedId}
          onSelect={onSelect}
        />
      ))}
    </div>
  );
}

// -- Recursive tree node ----------------------------------------------------

function TreeNode({
  node,
  lookup,
  depth,
  yardSummaries,
  cranePositions,
  selectedId,
  onSelect,
}: {
  node: HierarchyNode;
  lookup: Map<string, HierarchyNode>;
  depth: number;
  yardSummaries: Record<string, YardSummary>;
  cranePositions: Record<string, CranePosition>;
  selectedId?: string;
  onSelect?: (id: string) => void;
}) {
  const isSelected = node.id === selectedId;
  const color = LEVEL_COLORS[node.level];
  const badge = STATUS_BADGE[node.status] ?? '#555';

  const children = node.children
    .map((cid) => lookup.get(cid))
    .filter(Boolean) as HierarchyNode[];

  const summary = node.level === 'H3' ? yardSummaries[node.zone] : undefined;
  const crane =
    node.level === 'H1' && node.role === 'stacking_crane'
      ? cranePositions[node.id]
      : undefined;

  return (
    <div style={{ marginLeft: depth * 20 }}>
      {/* Node row */}
      <div
        onClick={() => onSelect?.(node.id)}
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '4px 8px',
          borderRadius: 4,
          cursor: 'pointer',
          background: isSelected ? 'rgba(255,255,255,0.08)' : 'transparent',
        }}
      >
        {/* Level badge */}
        <span
          style={{
            background: color,
            color: '#000',
            fontWeight: 'bold',
            fontSize: 10,
            padding: '1px 5px',
            borderRadius: 3,
          }}
        >
          {node.level}
        </span>

        {/* Role / name */}
        <span style={{ color }}>{node.role}</span>
        <span style={{ color: '#666', fontSize: 11 }}>{node.zone}</span>

        {/* Status dot */}
        <span
          style={{
            width: 8,
            height: 8,
            borderRadius: '50%',
            background: badge,
            display: 'inline-block',
            marginLeft: 'auto',
          }}
        />
      </div>

      {/* Yard Manager (H3) inline summary */}
      {summary && (
        <div
          style={{
            marginLeft: depth * 20 + 28,
            fontSize: 11,
            color: '#999',
            padding: '2px 0 4px',
          }}
        >
          {summary.totalUsedTeu}/{summary.totalCapacityTeu} TEU
          {' | '}
          {(summary.utilization * 100).toFixed(0)}% util
          {' | '}
          {summary.blockCount} blocks
          {summary.hazmatZonesActive > 0 && (
            <span style={{ color: '#ff6600', marginLeft: 6 }}>
              HAZMAT x{summary.hazmatZonesActive}
            </span>
          )}
        </div>
      )}

      {/* Stacking Crane (H1) inline status */}
      {crane && (
        <div
          style={{
            marginLeft: depth * 20 + 28,
            fontSize: 11,
            color: '#999',
            padding: '2px 0 4px',
          }}
        >
          R{crane.position.row}/B{crane.position.bay}
          {' | '}
          {crane.status}
          {crane.containerId && (
            <span style={{ color: '#44bbff', marginLeft: 6 }}>
              {crane.containerId}
            </span>
          )}
          {crane.fault && (
            <span style={{ color: '#ff4444', marginLeft: 6 }}>
              FAULT: {crane.fault}
            </span>
          )}
        </div>
      )}

      {/* Children */}
      {children.map((c) => (
        <TreeNode
          key={c.id}
          node={c}
          lookup={lookup}
          depth={depth + 1}
          yardSummaries={yardSummaries}
          cranePositions={cranePositions}
          selectedId={selectedId}
          onSelect={onSelect}
        />
      ))}
    </div>
  );
}

// -- Helpers ----------------------------------------------------------------

function levelOrd(level: ViewerHierarchyLevel): number {
  return { H1: 1, H2: 2, H3: 3, H4: 4 }[level];
}
