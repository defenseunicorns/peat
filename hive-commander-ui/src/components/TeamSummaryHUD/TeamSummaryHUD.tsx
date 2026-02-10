import { useMemo } from 'react';
import {
  TerminalTopology, TerminalNode, BerthOperation, ZoneId,
  zoneColors,
} from '../../wire-types';

interface TeamSummaryHUDProps {
  topology: TerminalTopology;
  selectedZone?: ZoneId;
  onZoneSelect?: (zoneId: ZoneId | undefined) => void;
}

interface ZoneStats {
  zoneId: ZoneId;
  label: string;
  total: number;
  active: number;
  busy: number;
  idle: number;
  offline: number;
}

function collectNodes(nodes: TerminalNode[]): Omit<ZoneStats, 'zoneId' | 'label'> {
  return {
    total: nodes.length,
    active: nodes.filter(n => n.status === 'active').length,
    busy: nodes.filter(n => n.status === 'busy').length,
    idle: nodes.filter(n => n.status === 'idle').length,
    offline: nodes.filter(n => n.status === 'offline').length,
  };
}

function berthNodes(berth: BerthOperation): TerminalNode[] {
  const nodes: TerminalNode[] = [berth.manager];
  berth.holds.forEach(h => {
    nodes.push(h.supervisor, h.cranes.lead, ...h.cranes.operators,
      h.stevedores.lead, ...h.stevedores.workers,
      h.lashing.lead, ...h.lashing.lashers, h.signaler);
  });
  return nodes;
}

function StatusBar({ active, busy, idle, offline }: { active: number; busy: number; idle: number; offline: number }) {
  return (
    <div style={{ display: 'flex', height: '4px', borderRadius: '2px', overflow: 'hidden', marginBottom: '4px' }}>
      <div style={{ flex: active, background: '#44ff44' }} />
      <div style={{ flex: busy, background: '#ffaa00' }} />
      <div style={{ flex: idle, background: '#555' }} />
      <div style={{ flex: offline, background: '#ff4444' }} />
    </div>
  );
}

function ZoneCard({ stats, isSelected, onClick }: { stats: ZoneStats; isSelected: boolean; onClick: () => void }) {
  const readiness = stats.total > 0 ? Math.round(((stats.active + stats.busy) / stats.total) * 100) : 0;
  const readinessColor = readiness >= 80 ? '#44ff44' : readiness >= 50 ? '#ffaa00' : '#ff4444';
  const color = zoneColors[stats.zoneId];

  return (
    <div
      onClick={onClick}
      style={{
        background: isSelected ? '#1a2a3a' : '#111122',
        border: isSelected ? `2px solid ${color}` : '1px solid #333',
        borderRadius: '8px',
        padding: '8px',
        cursor: 'pointer',
        transition: 'all 0.2s',
      }}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '4px' }}>
        <span style={{ color, fontWeight: 'bold', fontSize: '12px' }}>{stats.label}</span>
        <span style={{ color: readinessColor, fontSize: '11px', fontWeight: 'bold' }}>{readiness}%</span>
      </div>
      <StatusBar active={stats.active} busy={stats.busy} idle={stats.idle} offline={stats.offline} />
      <div style={{ color: '#666', fontSize: '10px' }}>
        {stats.total} nodes | {stats.active} active, {stats.busy} busy
      </div>
    </div>
  );
}

export default function TeamSummaryHUD({ topology, selectedZone, onZoneSelect }: TeamSummaryHUDProps) {
  // Zone-level stats
  const zoneStats = useMemo<ZoneStats[]>(() => {
    const berth1 = { ...collectNodes(berthNodes(topology.berths[0])), zoneId: 'berth1' as ZoneId, label: 'Berth 1' };
    const berth2 = { ...collectNodes(berthNodes(topology.berths[1])), zoneId: 'berth2' as ZoneId, label: 'Berth 2' };

    const yardNodes: TerminalNode[] = [
      topology.yard.manager, topology.yard.scSupervisor,
      ...topology.yard.stackingCranes.flatMap(sc => [sc.lead, ...sc.operators]),
      ...topology.yard.blocks.flatMap(b => [b.supervisor, b.lead, ...b.workers]),
    ];
    const yardStats = { ...collectNodes(yardNodes), zoneId: 'yard' as ZoneId, label: 'Yard' };

    const gateNodes: TerminalNode[] = [
      topology.gate.manager,
      ...topology.gate.gates.flatMap(g => [g.supervisor, g.lead, ...g.scanners, ...g.rfidReaders, ...g.workers]),
      topology.gate.rail.supervisor, topology.gate.rail.lead, ...topology.gate.rail.operators,
    ];
    const gateStats = { ...collectNodes(gateNodes), zoneId: 'gate' as ZoneId, label: 'Gate' };

    const tractorNodes: TerminalNode[] = [topology.tractorPool.lead, ...topology.tractorPool.drivers];
    const tractorStats = { ...collectNodes(tractorNodes), zoneId: 'tractor' as ZoneId, label: 'Tractor Pool' };

    return [berth1, berth2, yardStats, gateStats, tractorStats];
  }, [topology]);

  // Terminal-wide totals
  const terminalTotal = useMemo(() => ({
    total: topology.nodes.length,
    active: topology.nodes.filter(n => n.status === 'active').length,
    busy: topology.nodes.filter(n => n.status === 'busy').length,
    idle: topology.nodes.filter(n => n.status === 'idle').length,
    offline: topology.nodes.filter(n => n.status === 'offline').length,
  }), [topology]);

  // Simulated operational metrics
  const opMetrics = useMemo(() => {
    const activeCranes = topology.nodes.filter(n => n.role === 'crane_operator' && n.status === 'active').length;
    const totalCranes = topology.nodes.filter(n => n.role === 'crane_operator').length;
    const movesPerHour = Math.round(activeCranes * 28); // ~28 moves/hour per active crane

    const totalYardCapacity = topology.yardBlocks.reduce((s, b) => s + b.capacity, 0);
    const totalYardFilled = topology.yardBlocks.reduce((s, b) => s + b.filled, 0);
    const yardUtilization = Math.round((totalYardFilled / totalYardCapacity) * 100);

    const gateWorkers = topology.nodes.filter(n => (n.role === 'gate_worker' || n.role === 'gate_scanner') && n.status === 'active').length;
    const gateThroughput = Math.round(gateWorkers * 12); // ~12 trucks/hour per active worker

    const scActive = topology.nodes.filter(n => n.role === 'stacking_crane_op' && n.status === 'active').length;
    const scCyclesPerHour = Math.round(scActive * 18); // ~18 cycles/hour per SC

    return { movesPerHour, totalCranes, activeCranes, yardUtilization, totalYardCapacity, totalYardFilled, gateThroughput, scCyclesPerHour };
  }, [topology]);

  return (
    <div style={{ padding: '10px', overflow: 'auto' }}>
      {/* Terminal total */}
      <div style={{
        background: '#111122',
        border: '1px solid #333',
        borderRadius: '8px',
        padding: '10px',
        marginBottom: '10px',
      }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '4px' }}>
          <span style={{ color: '#ff9900', fontWeight: 'bold', fontSize: '13px' }}>TERMINAL</span>
          <span style={{ color: '#00ffff', fontSize: '12px' }}>{terminalTotal.total} nodes</span>
        </div>
        <div style={{ display: 'flex', height: '6px', borderRadius: '3px', overflow: 'hidden', marginBottom: '4px' }}>
          <div style={{ flex: terminalTotal.active, background: '#44ff44' }} />
          <div style={{ flex: terminalTotal.busy, background: '#ffaa00' }} />
          <div style={{ flex: terminalTotal.idle, background: '#555' }} />
          <div style={{ flex: terminalTotal.offline, background: '#ff4444' }} />
        </div>
        <div style={{ display: 'flex', gap: '10px', fontSize: '10px', color: '#888' }}>
          <span><span style={{ color: '#44ff44' }}>&#9679;</span> {terminalTotal.active} active</span>
          <span><span style={{ color: '#ffaa00' }}>&#9679;</span> {terminalTotal.busy} busy</span>
          <span><span style={{ color: '#555' }}>&#9679;</span> {terminalTotal.idle} idle</span>
          {terminalTotal.offline > 0 && <span><span style={{ color: '#ff4444' }}>&#9679;</span> {terminalTotal.offline} off</span>}
        </div>
      </div>

      {/* Operational metrics */}
      <div style={{
        background: '#111122',
        border: '1px solid #333',
        borderRadius: '8px',
        padding: '10px',
        marginBottom: '10px',
      }}>
        <div style={{ color: '#00ffff', fontWeight: 'bold', fontSize: '11px', marginBottom: '6px' }}>
          OPERATIONS
        </div>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '6px', fontSize: '10px' }}>
          <MetricBox label="Moves/hr" value={`${opMetrics.movesPerHour}`} sub={`${opMetrics.activeCranes}/${opMetrics.totalCranes} cranes`} color="#00ccff" />
          <MetricBox label="Yard" value={`${opMetrics.yardUtilization}%`} sub={`${opMetrics.totalYardFilled}/${opMetrics.totalYardCapacity}`} color={opMetrics.yardUtilization > 80 ? '#ff4444' : opMetrics.yardUtilization > 60 ? '#ffaa00' : '#44ff44'} />
          <MetricBox label="Gate/hr" value={`${opMetrics.gateThroughput}`} sub="trucks processed" color="#dd8844" />
          <MetricBox label="SC cycles/hr" value={`${opMetrics.scCyclesPerHour}`} sub="stack/retrieve" color="#44ddbb" />
        </div>
      </div>

      {/* Zone cards */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: '6px', marginBottom: '10px' }}>
        {zoneStats.map(stats => (
          <ZoneCard
            key={stats.zoneId}
            stats={stats}
            isSelected={selectedZone === stats.zoneId}
            onClick={() => onZoneSelect?.(selectedZone === stats.zoneId ? undefined : stats.zoneId)}
          />
        ))}
      </div>

      {/* Yard block fill levels */}
      <div style={{
        background: '#111122',
        border: '1px solid #333',
        borderRadius: '8px',
        padding: '10px',
      }}>
        <div style={{ color: '#66aa88', fontWeight: 'bold', fontSize: '11px', marginBottom: '6px' }}>
          YARD BLOCKS
        </div>
        {topology.yardBlocks.map(block => {
          const pct = Math.round((block.filled / block.capacity) * 100);
          const color = pct > 80 ? '#ff4444' : pct > 60 ? '#ffaa00' : '#44ff44';
          return (
            <div key={block.id} style={{ marginBottom: '4px' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '10px', color: '#888' }}>
                <span>{block.name}</span>
                <span style={{ color }}>{block.filled}/{block.capacity} ({pct}%)</span>
              </div>
              <div style={{ height: '3px', background: '#222', borderRadius: '2px', overflow: 'hidden' }}>
                <div style={{ width: `${pct}%`, height: '100%', background: color }} />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function MetricBox({ label, value, sub, color }: { label: string; value: string; sub: string; color: string }) {
  return (
    <div style={{ background: '#0a0a14', borderRadius: '4px', padding: '6px', border: '1px solid #222' }}>
      <div style={{ color: '#888', fontSize: '9px', marginBottom: '2px' }}>{label}</div>
      <div style={{ color, fontSize: '16px', fontWeight: 'bold', fontFamily: 'monospace' }}>{value}</div>
      <div style={{ color: '#555', fontSize: '8px' }}>{sub}</div>
    </div>
  );
}
