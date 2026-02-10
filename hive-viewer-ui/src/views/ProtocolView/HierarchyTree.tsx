/** HIVE hierarchy visualization — nodes as circles connected by edges.
 *  Shows the H1→H2→H3 hierarchy with status coloring. */

import { useViewerStore } from '../../protocol/state';
import type { NodeState } from 'hive-ui-shared/protocol/wire-types';

const ROLE_COLORS: Record<string, { active: string; idle: string }> = {
  crane: { active: '#22d3ee', idle: '#155e75' },       // cyan
  operator: { active: '#22c55e', idle: '#14532d' },    // green
  aggregator: { active: '#a78bfa', idle: '#4c1d95' },  // violet
  berth_manager: { active: '#f472b6', idle: '#831843' }, // pink
  tractor: { active: '#f59e0b', idle: '#78350f' },     // amber
  scheduler: { active: '#a78bfa', idle: '#581c87' },   // purple
  sensor: { active: '#3b82f6', idle: '#1e3a5f' },      // blue
  yard_block: { active: '#fb923c', idle: '#7c2d12' },  // orange
  lashing_crew: { active: '#ef4444', idle: '#7f1d1d' }, // red
  unknown: { active: '#9ca3af', idle: '#374151' },      // gray
};

const HIVE_LEVELS: Record<string, { label: string; y: number }> = {
  scheduler: { label: 'H4 — Scheduler', y: 30 },
  berth_manager: { label: 'H3 — Berth Mgr', y: 60 },
  aggregator: { label: 'H2 — Aggregator', y: 100 },
  yard_block: { label: 'H2 — Yard Block', y: 100 },
  crane: { label: 'H1 — Entity', y: 170 },
  operator: { label: 'H1 — Operator', y: 170 },
  tractor: { label: 'H1 — Tractor', y: 170 },
  lashing_crew: { label: 'H1 — Lashing', y: 170 },
  sensor: { label: 'H0 — Sensor', y: 250 },
  unknown: { label: '?', y: 290 },
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
    transport_container: 'TRANSPORT',
    report_position: 'POSITION',
    request_charge: 'CHARGE',
    rebalance_assignments: 'REBALANCE',
    update_priority_queue: 'PRIORITY',
    dispatch_resource: 'DISPATCH',
    emit_schedule_event: 'SCHEDULE',
    emit_reading: 'READING',
    report_calibration: 'CALIBRATE',
    update_berth_summary: 'BERTH SUM',
    emit_berth_event: 'BERTH EVT',
    request_tractor_rebalance: 'REBALANCE',
    accept_container: 'ACCEPT',
    assign_slot: 'ASSIGN',
    report_capacity: 'CAPACITY',
    secure_container: 'SECURE',
    report_lashing_complete: 'COMPLETE',
    inspect_lashing: 'INSPECT',
    request_lashing_tools: 'TOOLS',
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
  const schedulers = nodeList.filter((n) => n.role === 'scheduler');
  const berthManagers = nodeList.filter((n) => n.role === 'berth_manager');
  const aggregators = nodeList.filter((n) => n.role === 'aggregator');
  const yardBlocks = nodeList.filter((n) => n.role === 'yard_block');
  const cranes = nodeList.filter((n) => n.role === 'crane');
  const operators = nodeList.filter((n) => n.role === 'operator');
  const tractors = nodeList.filter((n) => n.role === 'tractor');
  const lashingCrew = nodeList.filter((n) => n.role === 'lashing_crew');
  const sensorNodes = nodeList.filter((n) => n.role === 'sensor');
  const others = nodeList.filter((n) => n.role === 'unknown');

  const svgWidth = 460;
  const svgHeight = 340;

  // Position nodes horizontally
  function xPositions(items: NodeState[], y: number): { node: NodeState; x: number; y: number }[] {
    const spacing = svgWidth / (items.length + 1);
    return items.map((node, i) => ({ node, x: spacing * (i + 1), y }));
  }

  const schedPositions = xPositions(schedulers, HIVE_LEVELS.scheduler.y);
  const berthMgrPositions = xPositions(berthManagers, HIVE_LEVELS.berth_manager.y);
  const h2Nodes = [...aggregators, ...yardBlocks];
  const aggPositions = xPositions(h2Nodes, HIVE_LEVELS.aggregator.y);
  const h1Nodes = [...cranes, ...operators, ...tractors, ...lashingCrew];
  const h1Positions = xPositions(h1Nodes, HIVE_LEVELS.crane.y);
  const sensorPositions = xPositions(sensorNodes, HIVE_LEVELS.sensor.y);
  const otherPositions = xPositions(others, HIVE_LEVELS.unknown?.y ?? 290);

  // Draw edges: H4→H3→H2→H1→H0 (fallback H4→H2 when no berth managers)
  const edges: { x1: number; y1: number; x2: number; y2: number }[] = [];
  if (berthMgrPositions.length > 0) {
    // H4→H3
    for (const sched of schedPositions) {
      for (const bm of berthMgrPositions) {
        edges.push({ x1: sched.x, y1: sched.y + 14, x2: bm.x, y2: bm.y - 14 });
      }
    }
    // H3→H2
    for (const bm of berthMgrPositions) {
      for (const agg of aggPositions) {
        edges.push({ x1: bm.x, y1: bm.y + 14, x2: agg.x, y2: agg.y - 14 });
      }
    }
  } else {
    // Fallback: H4→H2 when no berth managers
    for (const sched of schedPositions) {
      for (const agg of aggPositions) {
        edges.push({ x1: sched.x, y1: sched.y + 14, x2: agg.x, y2: agg.y - 14 });
      }
    }
  }
  for (const agg of aggPositions) {
    for (const h1 of h1Positions) {
      edges.push({ x1: agg.x, y1: agg.y + 14, x2: h1.x, y2: h1.y - 14 });
    }
  }
  for (const h1 of h1Positions) {
    for (const s of sensorPositions) {
      // Only connect sensors to nearby cranes (by proximity in layout)
      if (Math.abs(h1.x - s.x) < svgWidth / 4) {
        edges.push({ x1: h1.x, y1: h1.y + 14, x2: s.x, y2: s.y - 14 });
      }
    }
  }

  // Edges: H2 yard blocks ← H1 tractors (tractors deliver to yard blocks)
  for (const yb of aggPositions.filter(p => yardBlocks.includes(p.node))) {
    for (const h1 of h1Positions.filter(p => tractors.includes(p.node))) {
      edges.push({ x1: yb.x, y1: yb.y + 14, x2: h1.x, y2: h1.y - 14 });
    }
  }

  const allPositions = [...schedPositions, ...berthMgrPositions, ...aggPositions, ...h1Positions, ...sensorPositions, ...otherPositions];

  return (
    <div className="flex flex-col h-full">
      <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-wider px-3 py-2 border-b border-gray-800">
        Hierarchy
      </h3>
      <div className="flex-1 flex items-center justify-center p-2">
        <svg viewBox={`0 0 ${svgWidth} ${svgHeight}`} className="w-full h-full max-h-72">
          {/* Level labels */}
          {schedulers.length > 0 && (
            <text x="8" y={HIVE_LEVELS.scheduler.y - 10} className="fill-gray-600" fontSize="9">
              {HIVE_LEVELS.scheduler.label}
            </text>
          )}
          {berthManagers.length > 0 && (
            <text x="8" y={HIVE_LEVELS.berth_manager.y - 10} className="fill-gray-600" fontSize="9">
              {HIVE_LEVELS.berth_manager.label}
            </text>
          )}
          <text x="8" y={HIVE_LEVELS.aggregator.y - 10} className="fill-gray-600" fontSize="9">
            {HIVE_LEVELS.aggregator.label}
          </text>
          <text x="8" y={HIVE_LEVELS.crane.y - 10} className="fill-gray-600" fontSize="9">
            H1 — Entities
          </text>
          {sensorNodes.length > 0 && (
            <text x="8" y={HIVE_LEVELS.sensor.y - 10} className="fill-gray-600" fontSize="9">
              {HIVE_LEVELS.sensor.label}
            </text>
          )}

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
            const radius = (node.role === 'aggregator' || node.role === 'scheduler' || node.role === 'berth_manager' || node.role === 'yard_block') ? 18 : node.role === 'sensor' ? 12 : 15;
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
