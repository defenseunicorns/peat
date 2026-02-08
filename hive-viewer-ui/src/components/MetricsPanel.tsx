/** Aggregate metrics panel — summary stats across all agents. */

import { useViewerStore } from '../protocol/state';

export default function MetricsPanel() {
  const nodes = useViewerStore((s) => s.nodes);
  const totalCycles = useViewerStore((s) => s.totalCycles);
  const events = useViewerStore((s) => s.events);

  const nodeList = Object.values(nodes);
  if (nodeList.length === 0) return null;

  const totalActions = nodeList.reduce(
    (sum, n) => sum + n.history.filter((c) => c.action !== 'wait').length,
    0
  );
  const totalContentions = nodeList.reduce(
    (sum, n) => sum + n.history.filter((c) => c.contention_retry).length,
    0
  );
  const maxCycle = Math.max(...nodeList.map((n) => n.cycle));
  const latestTime = nodeList.find((n) => n.cycle === maxCycle)?.sim_time ?? '';

  return (
    <div className="flex items-center gap-4 text-xs font-mono">
      <Stat label="Agents" value={nodeList.length} />
      <Stat label="Cycles" value={totalCycles} />
      <Stat label="Actions" value={totalActions} />
      <Stat label="Events" value={events.length} />
      {totalContentions > 0 && (
        <Stat label="Contentions" value={totalContentions} className="text-amber-400" />
      )}
      {latestTime && (
        <span className="text-gray-500">
          Sim: <span className="text-gray-300">{latestTime}</span>
        </span>
      )}
    </div>
  );
}

function Stat({
  label,
  value,
  className = '',
}: {
  label: string;
  value: number | string;
  className?: string;
}) {
  return (
    <span className={`text-gray-500 ${className}`}>
      {label}: <span className="text-gray-200 font-semibold">{value}</span>
    </span>
  );
}
