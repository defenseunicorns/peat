import { ComposedCapability, Piece, getHealthColor, getHealthStatus, getWorstConfidence } from '../../types';

interface HierarchyTreeProps {
  capabilities: ComposedCapability[];
  pieces: Piece[];
  selectedCapability?: number;
  onSelectCapability?: (id: number | undefined) => void;
}

export function HierarchyTree({ capabilities, pieces, selectedCapability, onSelectCapability }: HierarchyTreeProps) {
  // Group capabilities by team
  const blueCaps = capabilities.filter(c => c.team === 'blue');

  // Overall force health = worst confidence across all capabilities
  const forceConfidence = blueCaps.length > 0
    ? Math.min(...blueCaps.map(c => getWorstConfidence(c)))
    : 1.0;
  const forceColor = getHealthColor(forceConfidence);
  const forceStatus = getHealthStatus(forceConfidence);

  return (
    <div style={{ fontSize: '12px' }}>
      {/* Force root node */}
      <div style={{
        display: 'flex',
        alignItems: 'center',
        gap: '6px',
        padding: '4px 0',
        marginBottom: '4px',
      }}>
        <span style={{
          width: '8px',
          height: '8px',
          borderRadius: '50%',
          background: forceColor,
          display: 'inline-block',
          flexShrink: 0,
        }} />
        <span style={{ color: '#ddd', fontWeight: 'bold' }}>BLUE FORCE</span>
        <span style={{ color: forceColor, fontSize: '10px', marginLeft: 'auto' }}>
          {forceStatus.toUpperCase()}
        </span>
      </div>

      {/* Capability nodes */}
      {blueCaps.map((cap) => {
        const worstConf = getWorstConfidence(cap);
        const nodeColor = getHealthColor(worstConf);
        const nodeStatus = getHealthStatus(worstConf);
        const isSelected = selectedCapability === cap.id;
        const capPieces = pieces.filter(p => cap.pieceIds.includes(p.id));

        return (
          <div key={cap.id} style={{ marginLeft: '12px' }}>
            {/* Capability node */}
            <div
              onClick={() => onSelectCapability?.(isSelected ? undefined : cap.id)}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '6px',
                padding: '3px 6px',
                marginBottom: '2px',
                borderRadius: '4px',
                cursor: 'pointer',
                background: isSelected ? '#1a3a4a' : 'transparent',
              }}
            >
              <span style={{
                width: '6px',
                height: '6px',
                borderRadius: '50%',
                background: nodeColor,
                display: 'inline-block',
                flexShrink: 0,
              }} />
              <span style={{ color: isSelected ? '#00ffff' : '#ccc' }}>{cap.name}</span>
              <span style={{ color: nodeColor, fontSize: '10px', marginLeft: 'auto' }}>
                {Math.round(worstConf * 100)}%
              </span>
              <span style={{ color: nodeColor, fontSize: '9px' }}>
                {nodeStatus === 'nominal' ? '' : nodeStatus.toUpperCase()}
              </span>
            </div>

            {/* Piece leaves (shown when capability selected) */}
            {isSelected && capPieces.map((piece) => {
              const fuelPct = piece.maxFuel > 0 ? piece.fuel / piece.maxFuel : 1;
              const pieceColor = getHealthColor(fuelPct);
              const typeLabel = piece.pieceType.type.toUpperCase();

              return (
                <div key={piece.id} style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '6px',
                  marginLeft: '16px',
                  padding: '2px 4px',
                  fontSize: '10px',
                }}>
                  <span style={{
                    width: '4px',
                    height: '4px',
                    borderRadius: '50%',
                    background: pieceColor,
                    display: 'inline-block',
                    flexShrink: 0,
                  }} />
                  <span style={{ color: '#999' }}>{typeLabel}</span>
                  <span style={{ color: '#666', marginLeft: 'auto' }}>
                    fuel {Math.round(fuelPct * 100)}%
                  </span>
                </div>
              );
            })}
          </div>
        );
      })}
    </div>
  );
}

export default HierarchyTree;
