/** HIVE hierarchy visualization — nodes as circles connected by edges.
 *  Shows the H1→H2→H3 hierarchy with status coloring. */

import { useViewerStore } from '../../protocol/state';
import type { NodeState } from '../../protocol/types';

const ROLE_COLORS: Record<string, { active: string; idle: string }> = {
  crane: { active: '#22d3ee', idle: '#155e75' },       // cyan
  operator: { active: '#22c55e', idle: '#14532d' },    // green
  aggregator: { active: '#a78bfa', idle: '#4c1d95' },  // violet
  unknown: { active: '#9ca3af', idle: '#374151' },      // gray
};

const HIVE_LEVELS: Record<string, { label: string; y: number }> = {
  aggregator: { label: 'H2 — Aggregator', y: 60 },
  crane: { label: 'H1 — Entity', y: 180 },
  operator: { label: 'H1 — Operator', y: 180 },
  unknown: { label: 'H0', y: 260 },
};

function nodeColor(node: NodeState): string {
  const colors = ROLE_COLORS[node.role];
  if (node.action === 'wait') return colors.idle;
  if (!node.success) return '#ef4444'; // red for failure
  if (node.contention_retry) return '#f59e0b'; // amber for contention
  return colors.active;
}

function actionLabel(action: string): string {
  const labels: Record<string, string> = {
    complete_container_move: 'MOVE',
    update_hold_summary: 'SUMMARY',
    emit_hold_event: 'EVENT',
    request_support: 'SUPPORT',
    report_equipment_status: 'STATUS',
    update_capability: 'CAPABILITY',
    wait: 'IDLE',
  };
  return labels[action] ?? action.toUpperCase();
}

export default function HierarchyTree() {
  const nodes = useViewerStore((s) => s.nodes);
  const nodeList = Object.values(nodes);

  if (nodeList.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-gray-500 text-sm">
        Waiting for agent data...
      </div>
    );
  }

  // Group nodes by role for layout
  const aggregators = nodeList.filter((n) => n.role === 'aggregator');
  const cranes = nodeList.filter((n) => n.role === 'crane');
  const others = nodeList.filter((n) => n.role === 'unknown');

  const svgWidth = 400;
  const svgHeight = 300;

  // Position nodes horizontally
  function xPositions(items: NodeState[], y: number): { node: NodeState; x: number; y: number }[] {
    const spacing = svgWidth / (items.length + 1);
    return items.map((node, i) => ({ node, x: spacing * (i + 1), y }));
  }

  const aggPositions = xPositions(aggregators, HIVE_LEVELS.aggregator.y);
  const cranePositions = xPositions(cranes, HIVE_LEVELS.crane.y);
  const otherPositions = xPositions(others, HIVE_LEVELS.unknown.y);

  // Draw edges from aggregators to cranes
  const edges: { x1: number; y1: number; x2: number; y2: number }[] = [];
  for (const agg of aggPositions) {
    for (const crane of cranePositions) {
      edges.push({ x1: agg.x, y1: agg.y + 20, x2: crane.x, y2: crane.y - 20 });
    }
  }

  const allPositions = [...aggPositions, ...cranePositions, ...otherPositions];

  return (
    <div className="flex flex-col h-full">
      <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-wider px-3 py-2 border-b border-gray-800">
        Hierarchy
      </h3>
      <div className="flex-1 flex items-center justify-center p-2">
        <svg viewBox={`0 0 ${svgWidth} ${svgHeight}`} className="w-full h-full max-h-72">
          {/* Level labels */}
          <text x="8" y={HIVE_LEVELS.aggregator.y - 20} className="fill-gray-600" fontSize="10">
            {HIVE_LEVELS.aggregator.label}
          </text>
          <text x="8" y={HIVE_LEVELS.crane.y - 20} className="fill-gray-600" fontSize="10">
            {HIVE_LEVELS.crane.label}
          </text>

          {/* Edges */}
          {edges.map((e, i) => (
            <line
              key={i}
              x1={e.x1} y1={e.y1} x2={e.x2} y2={e.y2}
              stroke="#374151"
              strokeWidth="1.5"
              strokeDasharray="4 3"
            />
          ))}

          {/* Nodes */}
          {allPositions.map(({ node, x, y }) => {
            const color = nodeColor(node);
            const radius = node.role === 'aggregator' ? 22 : 18;
            return (
              <g key={node.node_id}>
                {/* Glow for active nodes */}
                {node.action !== 'wait' && (
                  <circle cx={x} cy={y} r={radius + 4} fill={color} opacity={0.15} />
                )}
                {/* Contention pulse */}
                {node.contention_retry && (
                  <circle cx={x} cy={y} r={radius + 8} fill="none" stroke="#f59e0b" strokeWidth="1" opacity={0.5}>
                    <animate attributeName="r" from={String(radius + 4)} to={String(radius + 16)} dur="1s" repeatCount="indefinite" />
                    <animate attributeName="opacity" from="0.5" to="0" dur="1s" repeatCount="indefinite" />
                  </circle>
                )}
                <circle cx={x} cy={y} r={radius} fill="#111827" stroke={color} strokeWidth="2.5" />
                {/* Action label inside */}
                <text x={x} y={y - 3} textAnchor="middle" fontSize="8" fontWeight="bold" fill={color}>
                  {actionLabel(node.action)}
                </text>
                {/* Cycle number */}
                <text x={x} y={y + 8} textAnchor="middle" fontSize="7" fill="#9ca3af">
                  C{node.cycle}
                </text>
                {/* Node ID below */}
                <text x={x} y={y + radius + 14} textAnchor="middle" fontSize="9" fill="#d1d5db">
                  {node.node_id}
                </text>
              </g>
            );
          })}
        </svg>
      </div>
    </div>
  );
}
