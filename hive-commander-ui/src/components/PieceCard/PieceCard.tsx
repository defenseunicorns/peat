import { ComposedCapability } from '../../types';

interface CapabilityCardProps {
  capability: ComposedCapability;
  isSelected?: boolean;
  onClick?: () => void;
}

export function CapabilityCard({ capability, isSelected, onClick }: CapabilityCardProps) {
  const fuelPercent = capability.maxFuel > 0
    ? (capability.totalFuel / capability.maxFuel) * 100
    : 100;

  const fuelColor = fuelPercent < 30 ? '#ff4444' : fuelPercent < 60 ? '#ffaa00' : '#44ff44';

  // Determine primary capability type
  const primaryType = (() => {
    if (capability.strikeBonus >= 3 && capability.authorizeBonus >= 2) return 'STRIKE_READY';
    if (capability.fuseBonus >= 2 && capability.detectBonus >= 3) return 'AI_ISR_PKG';
    if (capability.detectBonus >= 3 && capability.trackBonus >= 2) return 'ISR_PACKAGE';
    if (capability.classifyBonus >= 3) return 'ANALYSIS_CELL';
    if (capability.predictBonus >= 2) return 'PREDICT_CELL';
    if (capability.reconBonus >= 3) return 'RECON_TEAM';
    if (capability.relayBonus >= 2) return 'SUPPORT_NET';
    if (capability.authorizeBonus >= 3) return 'COMMAND_ELM';
    return 'TASK_FORCE';
  })();

  return (
    <div
      onClick={onClick}
      style={{
        background: isSelected ? '#1a3a4a' : '#1a1a2a',
        border: isSelected ? '2px solid #00ffff' : '1px solid #333',
        borderRadius: '8px',
        padding: '12px',
        marginBottom: '8px',
        cursor: 'pointer',
        transition: 'all 0.2s',
      }}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '8px' }}>
        <span style={{ color: '#00ffff', fontWeight: 'bold', fontSize: '14px' }}>
          {capability.name}
        </span>
        <span style={{ color: '#888', fontSize: '12px' }}>
          ({capability.centerX}, {capability.centerY})
        </span>
      </div>

      <div style={{ color: '#aaa', fontSize: '11px', marginBottom: '8px' }}>
        {primaryType}
      </div>

      {/* Capability bonuses */}
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px', marginBottom: '8px' }}>
        {capability.detectBonus > 0 && (
          <StatBadge label="DET" value={capability.detectBonus} color="#44ff44" />
        )}
        {capability.trackBonus > 0 && (
          <StatBadge label="TRK" value={capability.trackBonus} color="#ffff44" />
        )}
        {capability.strikeBonus > 0 && (
          <StatBadge label="STR" value={capability.strikeBonus} color="#ff4444" />
        )}
        {capability.authorizeBonus > 0 && (
          <StatBadge label="AUTH" value={capability.authorizeBonus} color="#ff44ff" />
        )}
        {capability.classifyBonus > 0 && (
          <StatBadge label="CLS" value={capability.classifyBonus} color="#44aaff" />
        )}
        {capability.predictBonus > 0 && (
          <StatBadge label="PRD" value={capability.predictBonus} color="#aa44ff" />
        )}
        {capability.fuseBonus > 0 && (
          <StatBadge label="FUSE" value={capability.fuseBonus} color="#44ffaa" />
        )}
      </div>

      {/* Fuel bar */}
      <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
        <span style={{ color: '#888', fontSize: '11px' }}>Fuel:</span>
        <div style={{ flex: 1, height: '6px', background: '#333', borderRadius: '3px', overflow: 'hidden' }}>
          <div
            style={{
              width: `${fuelPercent}%`,
              height: '100%',
              background: fuelColor,
              transition: 'width 0.3s',
            }}
          />
        </div>
        <span style={{ color: '#888', fontSize: '11px' }}>
          {capability.pieceIds.length} units
        </span>
      </div>
    </div>
  );
}

interface StatBadgeProps {
  label: string;
  value: number;
  color: string;
}

function StatBadge({ label, value, color }: StatBadgeProps) {
  return (
    <div style={{
      display: 'flex',
      alignItems: 'center',
      gap: '4px',
      padding: '2px 6px',
      background: '#222',
      borderRadius: '4px',
      fontSize: '11px',
    }}>
      <span style={{ color }}>{label}</span>
      <span style={{ color: '#fff' }}>+{value}</span>
    </div>
  );
}

export default CapabilityCard;
