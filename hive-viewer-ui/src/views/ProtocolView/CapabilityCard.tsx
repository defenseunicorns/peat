/** Capability card — one per HIVE entity, showing latest state + cycle history. */

import { useViewerStore } from '../../protocol/state';
import type { NodeState } from '../../protocol/types';

const ROLE_BADGES: Record<string, { label: string; color: string }> = {
  crane: { label: 'CRANE', color: 'bg-cyan-900 text-cyan-300' },
  aggregator: { label: 'AGG', color: 'bg-violet-900 text-violet-300' },
  unknown: { label: 'NODE', color: 'bg-gray-800 text-gray-400' },
};

function ActionBar({ node }: { node: NodeState }) {
  // Mini bar chart of recent actions
  const recent = node.history.slice(-10);
  return (
    <div className="flex gap-0.5 mt-1.5">
      {recent.map((c, i) => {
        let color = 'bg-cyan-600';
        if (c.action === 'wait') color = 'bg-gray-700';
        else if (!c.success) color = 'bg-red-500';
        else if (c.contention_retry) color = 'bg-amber-500';
        else if (c.action === 'request_support') color = 'bg-orange-500';
        else if (c.action.includes('summary') || c.action.includes('event'))
          color = 'bg-violet-500';
        return (
          <div
            key={i}
            className={`w-2 h-4 rounded-sm ${color}`}
            title={`C${c.cycle}: ${c.action}${c.contention_retry ? ' (retry)' : ''}`}
          />
        );
      })}
    </div>
  );
}

function TimingRow({ label, ms }: { label: string; ms: number }) {
  const barWidth = Math.min(ms * 20, 100);
  return (
    <div className="flex items-center gap-2 text-[10px]">
      <span className="w-10 text-gray-500 shrink-0">{label}</span>
      <div className="flex-1 h-1.5 bg-gray-800 rounded-full overflow-hidden">
        <div
          className="h-full bg-cyan-700 rounded-full"
          style={{ width: `${barWidth}%` }}
        />
      </div>
      <span className="w-12 text-right text-gray-500 font-mono">{ms.toFixed(1)}ms</span>
    </div>
  );
}

function Card({ node }: { node: NodeState }) {
  const badge = ROLE_BADGES[node.role];
  const totalActions = node.history.filter((c) => c.action !== 'wait').length;
  const contentions = node.history.filter((c) => c.contention_retry).length;
  const failures = node.history.filter((c) => !c.success).length;

  return (
    <div className="bg-gray-900 border border-gray-800 rounded-lg p-3 min-w-[200px]">
      {/* Header */}
      <div className="flex items-center justify-between mb-2">
        <span className="font-semibold text-sm text-gray-200">{node.node_id}</span>
        <span className={`text-[10px] font-bold px-1.5 py-0.5 rounded ${badge.color}`}>
          {badge.label}
        </span>
      </div>

      {/* Current state */}
      <div className="text-xs space-y-1 mb-2">
        <div className="flex justify-between">
          <span className="text-gray-500">Cycle</span>
          <span className="font-mono">{node.cycle}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-gray-500">Action</span>
          <span className="font-mono truncate ml-2">{node.action.replace(/_/g, ' ')}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-gray-500">Time</span>
          <span className="font-mono">{node.sim_time}</span>
        </div>
      </div>

      {/* Stats row */}
      <div className="flex gap-3 text-[10px] text-gray-500 border-t border-gray-800 pt-1.5 mb-1.5">
        <span>{totalActions} actions</span>
        {contentions > 0 && (
          <span className="text-amber-500">{contentions} retries</span>
        )}
        {failures > 0 && (
          <span className="text-red-400">{failures} fails</span>
        )}
      </div>

      {/* OODA timing */}
      <div className="space-y-0.5">
        <TimingRow label="OBS" ms={node.observe_ms} />
        <TimingRow label="DEC" ms={node.decide_ms} />
        <TimingRow label="ACT" ms={node.act_ms} />
      </div>

      {/* Action history bar */}
      <ActionBar node={node} />
    </div>
  );
}

export default function CapabilityCards() {
  const nodes = useViewerStore((s) => s.nodes);
  const nodeList = Object.values(nodes);

  if (nodeList.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-gray-500 text-sm">
        No agents connected
      </div>
    );
  }

  // Sort: aggregators first, then cranes
  const sorted = [...nodeList].sort((a, b) => {
    const order: Record<string, number> = { aggregator: 0, operator: 1, crane: 2, unknown: 3 };
    return (order[a.role] ?? 2) - (order[b.role] ?? 2);
  });

  return (
    <div className="flex flex-col h-full">
      <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-wider px-3 py-2 border-b border-gray-800">
        Capabilities
        <span className="ml-2 text-gray-600">({nodeList.length} agents)</span>
      </h3>
      <div className="flex-1 overflow-y-auto p-2 space-y-2">
        {sorted.map((node) => (
          <Card key={node.node_id} node={node} />
        ))}
      </div>
    </div>
  );
}
