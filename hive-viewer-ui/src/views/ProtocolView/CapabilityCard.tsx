/** Capability card — per HIVE entity: OODA state + lifecycle health. */

import { useViewerStore } from '../../protocol/state';
import type { NodeState, LifecycleState } from '../../protocol/types';

const ROLE_BADGES: Record<string, { label: string; color: string }> = {
  crane: { label: 'CRANE', color: 'bg-cyan-900 text-cyan-300' },
  aggregator: { label: 'AGG', color: 'bg-violet-900 text-violet-300' },
  operator: { label: 'OP', color: 'bg-amber-900 text-amber-300' },
  tractor: { label: 'TRACTOR', color: 'bg-amber-900 text-amber-300' },
  scheduler: { label: 'SCHED', color: 'bg-purple-900 text-purple-300' },
  sensor: { label: 'SENSOR', color: 'bg-blue-900 text-blue-300' },
  unknown: { label: 'NODE', color: 'bg-gray-800 text-gray-400' },
};

function confidenceColor(v: number): string {
  if (v >= 0.7) return 'bg-green-500';
  if (v >= 0.4) return 'bg-amber-500';
  if (v > 0) return 'bg-red-500';
  return 'bg-gray-600';
}

function resourceColor(v: number): string {
  if (v >= 50) return 'bg-cyan-600';
  if (v >= 25) return 'bg-amber-500';
  return 'bg-red-500';
}

function ConfidenceBar({ label, value, status }: { label: string; value: number; status: string }) {
  const pct = Math.max(0, Math.min(100, value * 100));
  return (
    <div className="flex items-center gap-1.5 text-[10px]">
      <span className="w-16 text-gray-500 truncate shrink-0">{label}</span>
      <div className="flex-1 h-1.5 bg-gray-800 rounded-full overflow-hidden">
        <div className={`h-full rounded-full ${confidenceColor(value)}`} style={{ width: `${pct}%` }} />
      </div>
      <span className={`w-10 text-right font-mono ${status === 'NOMINAL' ? 'text-green-400' : status === 'DEGRADED' ? 'text-amber-400' : 'text-red-400'}`}>
        {(value * 100).toFixed(0)}%
      </span>
    </div>
  );
}

function ResourceBar({ label, value }: { label: string; value: number }) {
  const shortLabel = label.replace('_pct', '').replace('hydraulic_fluid', 'hydro').replace('battery', 'batt');
  return (
    <div className="flex items-center gap-1.5 text-[10px]">
      <span className="w-16 text-gray-500 truncate shrink-0">{shortLabel}</span>
      <div className="flex-1 h-1.5 bg-gray-800 rounded-full overflow-hidden">
        <div className={`h-full rounded-full ${resourceColor(value)}`} style={{ width: `${value}%` }} />
      </div>
      <span className="w-10 text-right font-mono text-gray-400">{value.toFixed(0)}%</span>
    </div>
  );
}

function GapBadge({ lifecycle }: { lifecycle: LifecycleState }) {
  if (!lifecycle.gapReport || lifecycle.gapReport.gaps.length === 0) return null;
  const score = lifecycle.gapReport.readinessScore;
  const color = score >= 0.7 ? 'text-green-400 border-green-800' : score >= 0.4 ? 'text-amber-400 border-amber-800' : 'text-red-400 border-red-800';
  return (
    <div className={`text-[9px] font-mono px-1.5 py-0.5 border rounded ${color}`}>
      RDY {(score * 100).toFixed(0)}% / {lifecycle.gapReport.gaps.length} gap{lifecycle.gapReport.gaps.length !== 1 ? 's' : ''}
    </div>
  );
}

function ActionBar({ node }: { node: NodeState }) {
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

function Card({ node }: { node: NodeState }) {
  const badge = ROLE_BADGES[node.role] ?? ROLE_BADGES.unknown;
  const lc = node.lifecycle;
  const hasSubs = Object.keys(lc.subsystems).length > 0;
  const hasResources = Object.keys(lc.resources).length > 0;
  const inMaint = lc.maintenanceJobs.length > 0;

  return (
    <div className="bg-gray-900 border border-gray-800 rounded-lg p-3 min-w-[200px]">
      {/* Header */}
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2">
          <span className="font-semibold text-sm text-gray-200">{node.node_id}</span>
          {inMaint && (
            <span className="text-[9px] font-bold px-1 py-0.5 rounded bg-orange-900 text-orange-300">MAINT</span>
          )}
          {lc.equipmentState === 'RESUPPLYING' && (
            <span className="text-[9px] font-bold px-1 py-0.5 rounded bg-yellow-900 text-yellow-300">RESUPPLY</span>
          )}
        </div>
        <span className={`text-[10px] font-bold px-1.5 py-0.5 rounded ${badge.color}`}>
          {badge.label}
        </span>
      </div>

      {/* OODA state */}
      <div className="text-xs space-y-0.5 mb-2">
        <div className="flex justify-between">
          <span className="text-gray-500">Cycle {node.cycle}</span>
          <span className="font-mono text-gray-400">{node.sim_time}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-gray-500">Action</span>
          <span className="font-mono truncate ml-2">{node.action.replace(/_/g, ' ')}</span>
        </div>
      </div>

      {/* Subsystem health bars */}
      {hasSubs && (
        <div className="space-y-0.5 mb-2 border-t border-gray-800 pt-1.5">
          <div className="text-[9px] text-gray-600 uppercase tracking-wider mb-0.5">Equipment Health</div>
          {Object.entries(lc.subsystems).map(([name, sub]) => (
            <ConfidenceBar key={name} label={name} value={sub.confidence} status={sub.status} />
          ))}
        </div>
      )}

      {/* Resource bars */}
      {hasResources && (
        <div className="space-y-0.5 mb-2 border-t border-gray-800 pt-1.5">
          <div className="text-[9px] text-gray-600 uppercase tracking-wider mb-0.5">Resources</div>
          {Object.entries(lc.resources).map(([name, res]) => (
            <ResourceBar key={name} label={name} value={res.value} />
          ))}
        </div>
      )}

      {/* Gap analysis badge */}
      {lc.gapReport && (
        <div className="border-t border-gray-800 pt-1.5 mb-1.5">
          <GapBadge lifecycle={lc} />
        </div>
      )}

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

  const sorted = [...nodeList].sort((a, b) => {
    const order: Record<string, number> = { scheduler: 0, aggregator: 1, operator: 2, crane: 3, tractor: 4, sensor: 5, unknown: 6 };
    return (order[a.role] ?? 6) - (order[b.role] ?? 6);
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
