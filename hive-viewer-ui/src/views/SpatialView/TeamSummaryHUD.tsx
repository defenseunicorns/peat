import { useSpatialState } from '../../spatial/useSpatialState';
import { CONTAINER_GRID } from '../../spatial/constants';

export default function TeamSummaryHUD() {
  const { holdSummary, cranes, operators, tractors, sensors, aggregatorActive, schedulerActive } = useSpatialState();
  const totalMoves = Object.values(cranes).reduce((sum, c) => sum + c.moveCount, 0);
  const anyContention = Object.values(cranes).some((c) => c.isContending);
  const opEntries = Object.values(operators);
  const opsAvailable = opEntries.filter((o) => o.isAvailable || o.assignedTo).length;
  const opsOnBreak = opEntries.some((o) => o.isOnBreak);
  const tractorEntries = Object.values(tractors);
  const tractorsActive = tractorEntries.filter((t) => t.isMoving).length;
  const sensorEntries = Object.values(sensors);

  return (
    <div className="absolute top-2 right-2 bg-gray-900/85 border border-gray-700 rounded px-3 py-2 text-[11px] font-mono text-gray-300 space-y-1 min-w-[160px] pointer-events-none">
      <div className="text-[10px] text-gray-500 uppercase tracking-wider mb-1">
        Hold-3 Summary
      </div>

      <div className="flex justify-between">
        <span>Moves/hr</span>
        <span className="text-cyan-400">
          {holdSummary.movesPerHour > 0 ? holdSummary.movesPerHour.toFixed(0) : '--'}
        </span>
      </div>

      <div className="flex justify-between">
        <span>Completed</span>
        <span className="text-green-400">
          {totalMoves}/{CONTAINER_GRID.total}
        </span>
      </div>

      <div className="flex justify-between">
        <span>Remaining</span>
        <span>{CONTAINER_GRID.total - totalMoves}</span>
      </div>

      {holdSummary.gapCount > 0 && (
        <div className="flex justify-between">
          <span>Gaps</span>
          <span className="text-amber-400">{holdSummary.gapCount}</span>
        </div>
      )}

      {opEntries.length > 0 && (
        <div className="flex justify-between">
          <span>Operators</span>
          <span className={opsOnBreak ? 'text-gray-500' : 'text-green-400'}>
            {opsOnBreak ? 'ON BREAK' : `${opsAvailable}/${opEntries.length}`}
          </span>
        </div>
      )}

      {tractorEntries.length > 0 && (
        <div className="flex justify-between">
          <span>Tractors</span>
          <span className="text-amber-400">
            {tractorsActive}/{tractorEntries.length}
          </span>
        </div>
      )}

      {sensorEntries.length > 0 && (
        <div className="flex justify-between">
          <span>Sensors</span>
          <span className="text-blue-400">
            {sensorEntries.filter((s) => s.isEmitting).length}/{sensorEntries.length}
          </span>
        </div>
      )}

      <div className="border-t border-gray-700 pt-1 mt-1 flex justify-between">
        <span>Aggregator</span>
        <span className={aggregatorActive ? 'text-violet-400' : 'text-gray-600'}>
          {aggregatorActive ? 'ACTIVE' : 'IDLE'}
        </span>
      </div>

      <div className="flex justify-between">
        <span>Scheduler</span>
        <span className={schedulerActive ? 'text-purple-400' : 'text-gray-600'}>
          {schedulerActive ? 'ACTIVE' : 'IDLE'}
        </span>
      </div>

      {anyContention && (
        <div className="text-amber-400 text-center animate-pulse">
          CONTENTION
        </div>
      )}
    </div>
  );
}
