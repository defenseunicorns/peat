import { useMemo } from 'react';
import { BerthTopology, HoldId, HoldTeam, roleColors } from '../../wire-types';

interface TeamSummaryHUDProps {
  topology: BerthTopology;
  selectedHold?: HoldId;
  onHoldSelect?: (holdId: HoldId | undefined) => void;
}

interface HoldStats {
  holdId: HoldId;
  total: number;
  active: number;
  busy: number;
  idle: number;
  offline: number;
  cranes: number;
  stevedores: number;
  lashers: number;
}

function computeHoldStats(hold: HoldTeam): HoldStats {
  const nodes = [
    hold.supervisor,
    hold.cranes.lead,
    ...hold.cranes.operators,
    hold.stevedores.lead,
    ...hold.stevedores.workers,
    hold.lashing.lead,
    ...hold.lashing.lashers,
    hold.signaler,
  ];

  return {
    holdId: hold.holdId,
    total: nodes.length,
    active: nodes.filter(n => n.status === 'active').length,
    busy: nodes.filter(n => n.status === 'busy').length,
    idle: nodes.filter(n => n.status === 'idle').length,
    offline: nodes.filter(n => n.status === 'offline').length,
    cranes: hold.cranes.operators.length,
    stevedores: hold.stevedores.workers.length,
    lashers: hold.lashing.lashers.length,
  };
}

function HoldCard({ stats, isSelected, onClick }: { stats: HoldStats; isSelected: boolean; onClick: () => void }) {
  const readiness = stats.total > 0 ? Math.round(((stats.active + stats.busy) / stats.total) * 100) : 0;
  const readinessColor = readiness >= 80 ? '#44ff44' : readiness >= 50 ? '#ffaa00' : '#ff4444';

  return (
    <div
      onClick={onClick}
      style={{
        background: isSelected ? '#1a2a3a' : '#111122',
        border: isSelected ? '2px solid #cc44ff' : '1px solid #333',
        borderRadius: '8px',
        padding: '10px',
        cursor: 'pointer',
        transition: 'all 0.2s',
      }}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '6px' }}>
        <span style={{ color: '#cc44ff', fontWeight: 'bold', fontSize: '13px' }}>
          Hold {stats.holdId}
        </span>
        <span style={{ color: readinessColor, fontSize: '12px', fontWeight: 'bold' }}>
          {readiness}%
        </span>
      </div>

      {/* Status bar */}
      <div style={{ display: 'flex', height: '4px', borderRadius: '2px', overflow: 'hidden', marginBottom: '8px' }}>
        <div style={{ flex: stats.active, background: '#44ff44' }} />
        <div style={{ flex: stats.busy, background: '#ffaa00' }} />
        <div style={{ flex: stats.idle, background: '#555' }} />
        <div style={{ flex: stats.offline, background: '#ff4444' }} />
      </div>

      {/* Team breakdown */}
      <div style={{ display: 'flex', gap: '8px', fontSize: '10px' }}>
        <RoleBadge label="CRN" count={stats.cranes} color={roleColors.crane_operator} />
        <RoleBadge label="STV" count={stats.stevedores} color={roleColors.stevedore} />
        <RoleBadge label="LSH" count={stats.lashers} color={roleColors.lasher} />
      </div>

      <div style={{ color: '#666', fontSize: '10px', marginTop: '4px' }}>
        {stats.total} nodes | {stats.active} active, {stats.busy} busy
      </div>
    </div>
  );
}

function RoleBadge({ label, count, color }: { label: string; count: number; color: string }) {
  return (
    <span style={{
      padding: '1px 5px',
      background: '#1a1a2a',
      borderRadius: '3px',
      border: `1px solid ${color}33`,
    }}>
      <span style={{ color }}>{label}</span>
      <span style={{ color: '#aaa', marginLeft: '3px' }}>{count}</span>
    </span>
  );
}

export default function TeamSummaryHUD({ topology, selectedHold, onHoldSelect }: TeamSummaryHUDProps) {
  const holdStats = useMemo(() => topology.holds.map(computeHoldStats), [topology]);

  const berthTotal = useMemo(() => ({
    total: topology.nodes.length,
    active: topology.nodes.filter(n => n.status === 'active').length,
    busy: topology.nodes.filter(n => n.status === 'busy').length,
    idle: topology.nodes.filter(n => n.status === 'idle').length,
    offline: topology.nodes.filter(n => n.status === 'offline').length,
  }), [topology]);

  const sharedNodes = useMemo(() => {
    const tractors = [topology.shared.tractors.lead, ...topology.shared.tractors.drivers];
    const yardA = [topology.shared.yardBlockA.lead, ...topology.shared.yardBlockA.workers];
    const yardB = [topology.shared.yardBlockB.lead, ...topology.shared.yardBlockB.workers];
    return { tractors: tractors.length, yardA: yardA.length, yardB: yardB.length };
  }, [topology]);

  return (
    <div style={{ padding: '12px', overflow: 'auto' }}>
      {/* Berth total */}
      <div style={{
        background: '#111122',
        border: '1px solid #333',
        borderRadius: '8px',
        padding: '10px',
        marginBottom: '12px',
      }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '4px' }}>
          <span style={{ color: '#ff6600', fontWeight: 'bold', fontSize: '13px' }}>BERTH TOTAL</span>
          <span style={{ color: '#00ffff', fontSize: '12px' }}>{berthTotal.total} nodes</span>
        </div>
        <div style={{ display: 'flex', height: '6px', borderRadius: '3px', overflow: 'hidden', marginBottom: '4px' }}>
          <div style={{ flex: berthTotal.active, background: '#44ff44' }} />
          <div style={{ flex: berthTotal.busy, background: '#ffaa00' }} />
          <div style={{ flex: berthTotal.idle, background: '#555' }} />
          <div style={{ flex: berthTotal.offline, background: '#ff4444' }} />
        </div>
        <div style={{ display: 'flex', gap: '12px', fontSize: '10px', color: '#888' }}>
          <span><span style={{ color: '#44ff44' }}>&#9679;</span> {berthTotal.active} active</span>
          <span><span style={{ color: '#ffaa00' }}>&#9679;</span> {berthTotal.busy} busy</span>
          <span><span style={{ color: '#555' }}>&#9679;</span> {berthTotal.idle} idle</span>
          {berthTotal.offline > 0 && <span><span style={{ color: '#ff4444' }}>&#9679;</span> {berthTotal.offline} off</span>}
        </div>
      </div>

      {/* Per-hold stats */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: '8px', marginBottom: '12px' }}>
        {holdStats.map(stats => (
          <HoldCard
            key={stats.holdId}
            stats={stats}
            isSelected={selectedHold === stats.holdId}
            onClick={() => onHoldSelect?.(selectedHold === stats.holdId ? undefined : stats.holdId)}
          />
        ))}
      </div>

      {/* Shared resources */}
      <div style={{
        background: '#111122',
        border: '1px solid #333',
        borderRadius: '8px',
        padding: '10px',
      }}>
        <div style={{ color: '#66aa88', fontWeight: 'bold', fontSize: '12px', marginBottom: '6px' }}>
          SHARED RESOURCES
        </div>
        <div style={{ display: 'flex', gap: '8px', fontSize: '10px' }}>
          <RoleBadge label="TRC" count={sharedNodes.tractors} color={roleColors.tractor_driver} />
          <RoleBadge label="YD-A" count={sharedNodes.yardA} color={roleColors.yard_worker} />
          <RoleBadge label="YD-B" count={sharedNodes.yardB} color={roleColors.yard_worker} />
        </div>

        {/* Yard fill levels */}
        <div style={{ marginTop: '8px' }}>
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
    </div>
  );
}
