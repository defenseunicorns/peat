/** Scrolling event stream — shows OODA cycles and HIVE events as a log. */

import { useEffect, useRef } from 'react';
import { useViewerStore } from '../../protocol/state';
import type { NodeState, HiveEvent } from '../../protocol/types';

const ACTION_ICONS: Record<string, string> = {
  complete_container_move: '\u25B6',  // play
  update_hold_summary: '\u2211',      // sigma
  emit_hold_event: '\u26A0',          // warning
  request_support: '\u2691',          // flag
  report_equipment_status: '\u2699',  // gear
  update_capability: '\u2B06',        // up arrow
  transport_container: '\u{1F69A}',   // truck
  report_position: '\u{1F4CD}',       // pin
  request_charge: '\u{1F50B}',        // battery
  rebalance_assignments: '\u21C4',    // arrows
  update_priority_queue: '\u2195',    // up-down
  dispatch_resource: '\u27A1',        // right arrow
  emit_schedule_event: '\u{1F4CB}',   // clipboard
  emit_reading: '\u{1F4CA}',          // chart
  report_calibration: '\u{1F527}',    // wrench
  wait: '\u23F8',                     // pause
};

/** Lifecycle event icons and colors by event_type. */
const LIFECYCLE_STYLE: Record<string, { icon: string; color: string }> = {
  CAPABILITY_DEGRADED:      { icon: '\u2193', color: 'text-amber-400' },    // down arrow
  RESOURCE_CONSUMED:        { icon: '\u2B07', color: 'text-cyan-400' },     // down arrow
  RESUPPLY_REQUESTED:       { icon: '\u26FD', color: 'text-yellow-400' },   // fuel pump
  RESUPPLY_COMPLETED:       { icon: '\u2705', color: 'text-green-400' },    // check
  MAINTENANCE_SCHEDULED:    { icon: '\uD83D\uDD27', color: 'text-orange-400' }, // wrench
  MAINTENANCE_STARTED:      { icon: '\u2692', color: 'text-orange-300' },   // hammers
  MAINTENANCE_COMPLETE:     { icon: '\u2714', color: 'text-green-400' },    // check
  CERTIFICATION_EXPIRING:   { icon: '\u23F0', color: 'text-amber-400' },    // alarm
  CERTIFICATION_EXPIRED:    { icon: '\u274C', color: 'text-red-400' },      // cross
  RECERTIFICATION_COMPLETED:{ icon: '\u2714', color: 'text-green-400' },    // check
  GAP_ANALYSIS_REPORT:      { icon: '\u2637', color: 'text-violet-400' },   // trigram
  SHIFT_RELIEF_REQUESTED:   { icon: '\u21C4', color: 'text-blue-400' },     // arrows
  SHIFT_RELIEF_ARRIVED:     { icon: '\u2714', color: 'text-green-400' },    // check
  CALIBRATION_DRIFT:        { icon: '\u{1F527}', color: 'text-amber-400' }, // wrench
  container_transported:    { icon: '\u{1F69A}', color: 'text-amber-300' }, // truck
  sensor_reading:           { icon: '\u{1F4CA}', color: 'text-blue-300' },  // chart
  anomaly_detected:         { icon: '\u26A0', color: 'text-red-400' },      // warning
  operator_reassigned:      { icon: '\u21C4', color: 'text-purple-400' },   // arrows
  queue_reordered:          { icon: '\u2195', color: 'text-purple-300' },   // up-down
  resource_dispatched:      { icon: '\u27A1', color: 'text-purple-400' },   // right arrow
};

const NODE_COLORS: Record<string, string> = {
  'crane-1': 'text-cyan-400',
  'crane-2': 'text-teal-400',
  'hold-agg-3': 'text-violet-400',
};

function nodeColor(nodeId: string): string {
  if (nodeId.startsWith('crane')) return 'text-cyan-400';
  if (nodeId.includes('agg')) return 'text-violet-400';
  if (nodeId.startsWith('op-')) return 'text-green-400';
  if (nodeId.startsWith('tractor-')) return 'text-amber-400';
  if (nodeId.startsWith('scheduler')) return 'text-purple-400';
  if (nodeId.startsWith('load-cell-') || nodeId.startsWith('rfid-')) return 'text-blue-400';
  return NODE_COLORS[nodeId] ?? 'text-gray-300';
}

/** Extract a short detail summary from lifecycle event details. */
function lifecycleDetail(eventType: string, details: unknown): string {
  const d = (details ?? {}) as Record<string, unknown>;
  switch (eventType) {
    case 'CAPABILITY_DEGRADED': {
      const sub = d.subsystem as string ?? '';
      const after = d.after as number;
      const status = d.status as string ?? '';
      return `${sub} ${(after * 100).toFixed(0)}% [${status}]`;
    }
    case 'RESOURCE_CONSUMED': {
      const res = (d.resource as string ?? '').replace('_pct', '');
      const after = d.after as number;
      return `${res} ${after.toFixed(0)}%`;
    }
    case 'RESUPPLY_REQUESTED':
    case 'RESUPPLY_COMPLETED':
      return (d.equipment_id as string) ?? '';
    case 'MAINTENANCE_SCHEDULED':
    case 'MAINTENANCE_STARTED':
    case 'MAINTENANCE_COMPLETE': {
      const sub = d.subsystem as string ?? d.equipment_id as string ?? '';
      const restored = d.restored_confidence as number;
      return restored !== undefined ? `${sub} \u2192 ${(restored * 100).toFixed(0)}%` : sub;
    }
    case 'GAP_ANALYSIS_REPORT': {
      const score = d.readiness_score as number;
      const gaps = d.gaps as unknown[];
      return `RDY ${(score * 100).toFixed(0)}% / ${gaps?.length ?? 0} gaps`;
    }
    case 'CERTIFICATION_EXPIRING':
    case 'CERTIFICATION_EXPIRED':
    case 'RECERTIFICATION_COMPLETED':
      return (d.worker_id as string) ?? (d.cert_type as string) ?? '';
    case 'CALIBRATION_DRIFT': {
      const acc = d.accuracy_pct as number;
      const st = d.status as string ?? '';
      return `${acc?.toFixed(0)}% [${st}]`;
    }
    case 'container_transported':
      return `${d.container_id ?? ''} → ${d.destination ?? ''}`;
    case 'sensor_reading':
      return `${d.reading_type ?? ''} ${d.value ?? ''}${d.unit ?? ''}`;
    case 'anomaly_detected':
      return `${d.reading_type ?? ''} ${d.value ?? ''} (expected ${d.expected ?? ''})`;
    case 'resource_dispatched':
      return `${d.resource_type ?? ''}: ${d.from_entity ?? ''} → ${d.to_entity ?? ''}`;
    default:
      return '';
  }
}

/** Merge OODA cycle histories and HIVE events into a unified timeline. */
function buildTimeline(
  nodes: Record<string, NodeState>,
  events: HiveEvent[]
): TimelineEntry[] {
  const entries: TimelineEntry[] = [];

  // Add all OODA cycles
  for (const node of Object.values(nodes)) {
    for (const cycle of node.history) {
      entries.push({
        kind: 'cycle',
        sortKey: node.node_id + '-' + String(cycle.cycle).padStart(4, '0'),
        cycle: cycle.cycle,
        nodeId: node.node_id,
        action: cycle.action,
        success: cycle.success,
        contention: cycle.contention_retry,
        simTime: cycle.sim_time,
        totalMs: cycle.total_ms,
      });
    }
  }

  // Add HIVE events
  for (let i = 0; i < events.length; i++) {
    const e = events[i];
    entries.push({
      kind: 'event',
      sortKey: `event-${String(i).padStart(6, '0')}`,
      eventType: e.event_type,
      source: e.source,
      priority: e.priority,
      details: e.details,
    });
  }

  // Sort by cycle-based order (cycles first, events at end)
  entries.sort((a, b) => a.sortKey.localeCompare(b.sortKey));
  return entries;
}

type TimelineEntry =
  | {
      kind: 'cycle';
      sortKey: string;
      cycle: number;
      nodeId: string;
      action: string;
      success: boolean;
      contention: boolean;
      simTime: string;
      totalMs?: number;
    }
  | {
      kind: 'event';
      sortKey: string;
      eventType: string;
      source: string;
      priority: string;
      details: unknown;
    };

export default function EventStream() {
  const nodes = useViewerStore((s) => s.nodes);
  const events = useViewerStore((s) => s.events);
  const bottomRef = useRef<HTMLDivElement>(null);

  const timeline = buildTimeline(nodes, events);

  // Auto-scroll to bottom on new entries
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [timeline.length]);

  return (
    <div className="flex flex-col h-full">
      <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-wider px-3 py-2 border-b border-gray-800">
        Event Stream
        <span className="ml-2 text-gray-600">({timeline.length})</span>
      </h3>
      <div className="flex-1 overflow-y-auto font-mono text-xs leading-5 px-2 py-1">
        {timeline.length === 0 && (
          <div className="text-gray-600 py-4 text-center">No events yet</div>
        )}
        {timeline.map((entry, i) => {
          if (entry.kind === 'cycle') {
            const icon = ACTION_ICONS[entry.action] ?? '\u25CB';
            const statusClass = !entry.success
              ? 'text-red-400'
              : entry.contention
              ? 'text-amber-400'
              : 'text-gray-300';
            return (
              <div key={i} className={`flex gap-1.5 items-baseline ${statusClass}`}>
                <span className="w-4 text-center shrink-0">{icon}</span>
                <span className={`w-20 shrink-0 ${nodeColor(entry.nodeId)}`}>
                  {entry.nodeId}
                </span>
                <span className="w-8 text-gray-500 shrink-0">C{entry.cycle}</span>
                <span className="flex-1 truncate">{formatAction(entry.action)}</span>
                {entry.contention && (
                  <span className="text-amber-500 shrink-0" title="Contention retry">
                    RETRY
                  </span>
                )}
                {entry.totalMs !== undefined && (
                  <span className="text-gray-600 shrink-0 w-14 text-right">
                    {entry.totalMs.toFixed(1)}ms
                  </span>
                )}
              </div>
            );
          } else {
            const style = LIFECYCLE_STYLE[entry.eventType];
            const icon = style?.icon ?? '\u26A1';
            const colorClass = style?.color ?? 'text-gray-400';
            const detail = lifecycleDetail(entry.eventType, entry.details);
            return (
              <div key={i} className={`flex gap-1.5 items-baseline ${colorClass}`}>
                <span className="w-4 text-center shrink-0">{icon}</span>
                <span className={`w-20 shrink-0 ${nodeColor(entry.source)}`}>
                  {entry.source}
                </span>
                <span className="shrink-0 font-semibold">
                  {formatEventType(entry.eventType)}
                </span>
                {detail && (
                  <span className="flex-1 truncate text-gray-400">{detail}</span>
                )}
                {!detail && <span className="flex-1" />}
                <span className="text-gray-600 shrink-0 text-[10px]">{entry.priority}</span>
              </div>
            );
          }
        })}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}

function formatAction(action: string): string {
  return action.replace(/_/g, ' ');
}

function formatEventType(eventType: string): string {
  return eventType.replace(/_/g, ' ').replace(/\b\w/g, (c) => c);
}
