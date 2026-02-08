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
  wait: '\u23F8',                     // pause
};

const PRIORITY_COLORS: Record<string, string> = {
  HIGH: 'text-red-400',
  ROUTINE: 'text-gray-400',
  LOW: 'text-gray-600',
};

const NODE_COLORS: Record<string, string> = {
  'crane-1': 'text-cyan-400',
  'crane-2': 'text-teal-400',
  'hold-agg-3': 'text-violet-400',
};

function nodeColor(nodeId: string): string {
  return NODE_COLORS[nodeId] ?? 'text-gray-300';
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
            const prioClass = PRIORITY_COLORS[entry.priority] ?? 'text-gray-400';
            return (
              <div key={i} className={`flex gap-1.5 items-baseline ${prioClass}`}>
                <span className="w-4 text-center shrink-0">{'\u26A1'}</span>
                <span className={`w-20 shrink-0 ${nodeColor(entry.source)}`}>
                  {entry.source}
                </span>
                <span className="flex-1 truncate font-semibold">
                  {entry.eventType}
                </span>
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
