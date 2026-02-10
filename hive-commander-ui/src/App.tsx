import { useState, useMemo } from 'react';
import Map3D from './components/Map3D/Map3D';
import { CapabilityCard } from './components/PieceCard/PieceCard';
import HierarchyTree from './components/HierarchyTree/HierarchyTree';
import BerthScene from './components/BerthScene/BerthScene';
import TeamSummaryHUD from './components/TeamSummaryHUD/TeamSummaryHUD';
import EventStream from './components/EventStream/EventStream';
import { TerrainType, Piece, ComposedCapability, Objective, GamePhase } from './types';
import { ZoneId, createPhase3Topology } from './wire-types';

type ViewMode = 'tactical' | 'terminal_hierarchy' | 'terminal_spatial';

// Generate simple terrain for demo
function generateTerrain(width: number, height: number): TerrainType[][] {
  const terrain: TerrainType[][] = [];
  for (let y = 0; y < height; y++) {
    const row: TerrainType[] = [];
    for (let x = 0; x < width; x++) {
      const noise = Math.sin(x * 0.5) * Math.cos(y * 0.5) + Math.random() * 0.3;
      if (noise < -0.4) row.push('deep_water');
      else if (noise < -0.1) row.push('shallow_water');
      else if (noise < 0.3) row.push('plains');
      else if (noise < 0.5) row.push(Math.random() > 0.5 ? 'forest' : 'plains');
      else if (noise < 0.7) row.push('hills');
      else row.push('mountain');
    }
    terrain.push(row);
  }
  terrain[height / 2][2] = 'base';
  terrain[height / 2][width - 3] = 'base';
  terrain[4][8] = 'urban';
  terrain[4][9] = 'urban';
  terrain[5][8] = 'urban';
  return terrain;
}

// Demo game data
function createDemoState() {
  const terrain = generateTerrain(20, 12);

  const pieces: Piece[] = [
    { id: 0, pieceType: { type: 'sensor', mode: 'eo' }, team: 'blue', x: 3, y: 4, fuel: 8, maxFuel: 10 },
    { id: 1, pieceType: { type: 'sensor', mode: 'ir' }, team: 'blue', x: 3, y: 5, fuel: 10, maxFuel: 10 },
    { id: 2, pieceType: { type: 'sensor', mode: 'radar' }, team: 'blue', x: 4, y: 4, fuel: 7, maxFuel: 10 },
    { id: 3, pieceType: { type: 'scout' }, team: 'blue', x: 5, y: 6, fuel: 6, maxFuel: 10 },
    { id: 4, pieceType: { type: 'striker' }, team: 'blue', x: 6, y: 5, fuel: 10, maxFuel: 10 },
    { id: 5, pieceType: { type: 'striker' }, team: 'blue', x: 6, y: 6, fuel: 9, maxFuel: 10 },
    { id: 6, pieceType: { type: 'support' }, team: 'blue', x: 4, y: 6, fuel: 10, maxFuel: 10 },
    { id: 7, pieceType: { type: 'authority' }, team: 'blue', x: 2, y: 5, fuel: 10, maxFuel: 10 },
    { id: 8, pieceType: { type: 'analyst' }, team: 'blue', x: 4, y: 5, fuel: 10, maxFuel: 10 },
    { id: 10, pieceType: { type: 'sensor', mode: 'ir' }, team: 'red', x: 15, y: 4, fuel: 10, maxFuel: 10 },
    { id: 11, pieceType: { type: 'striker' }, team: 'red', x: 16, y: 5, fuel: 10, maxFuel: 10 },
  ];

  const capabilities: ComposedCapability[] = [
    { id: 0, name: 'AI_ISR-1', pieceIds: [0, 1, 2, 8], centerX: 4, centerY: 5, detectBonus: 10, trackBonus: 8, strikeBonus: 0, reconBonus: 1, authorizeBonus: 0, relayBonus: 0, classifyBonus: 3, predictBonus: 2, fuseBonus: 3, totalFuel: 35, maxFuel: 40, team: 'blue' },
    { id: 1, name: 'STRIKE-2', pieceIds: [4, 5, 7], centerX: 6, centerY: 5, detectBonus: 0, trackBonus: 0, strikeBonus: 8, reconBonus: 0, authorizeBonus: 5, relayBonus: 0, classifyBonus: 0, predictBonus: 0, fuseBonus: 0, totalFuel: 29, maxFuel: 30, team: 'blue' },
    { id: 2, name: 'RECON-3', pieceIds: [3, 6], centerX: 5, centerY: 6, detectBonus: 3, trackBonus: 2, strikeBonus: 0, reconBonus: 3, authorizeBonus: 0, relayBonus: 3, classifyBonus: 0, predictBonus: 0, fuseBonus: 0, totalFuel: 16, maxFuel: 20, team: 'blue' },
  ];

  const objectives: Objective[] = [
    { id: 0, name: 'TRACK HVT', description: 'Locate and track high-value target', x: 10, y: 4, detectRequired: 2, trackRequired: 2, strikeRequired: 0, authorizeRequired: 0, completed: false, turnsRemaining: 0, points: 100 },
    { id: 1, name: 'SECURE AREA', description: 'Establish presence and secure zone', x: 12, y: 7, detectRequired: 1, trackRequired: 0, strikeRequired: 2, authorizeRequired: 2, completed: false, turnsRemaining: 0, points: 150 },
    { id: 2, name: 'ANALYZE TARGET', description: 'AI-assisted target analysis', x: 8, y: 5, detectRequired: 2, trackRequired: 1, strikeRequired: 0, authorizeRequired: 0, completed: false, turnsRemaining: 0, points: 75 },
  ];

  return { terrain, pieces, capabilities, objectives };
}

export default function App() {
  const [viewMode, setViewMode] = useState<ViewMode>('terminal_hierarchy');
  const [showPieces, setShowPieces] = useState(false);
  const [selectedCapability, setSelectedCapability] = useState<number | undefined>(undefined);
  const [selectedObjective, setSelectedObjective] = useState<number | undefined>(undefined);
  const [selectedZone, setSelectedZone] = useState<ZoneId | undefined>(undefined);
  const [selectedNodeId, setSelectedNodeId] = useState<string | undefined>(undefined);
  const [phase] = useState<GamePhase>('select_objective');
  const [turn] = useState(1);
  const [score] = useState(0);

  const demoState = useMemo(() => createDemoState(), []);
  const terminalTopology = useMemo(() => createPhase3Topology(), []);

  return (
    <div style={{ display: 'flex', width: '100%', height: '100%' }}>
      {/* Main view area */}
      <div style={{ flex: 1, position: 'relative', display: 'flex', flexDirection: 'column' }}>
        {/* View mode tabs */}
        <div style={{
          display: 'flex',
          background: '#0a0a14',
          borderBottom: '1px solid #333',
          padding: '0 8px',
        }}>
          {([
            { mode: 'tactical' as ViewMode, label: 'Tactical' },
            { mode: 'terminal_hierarchy' as ViewMode, label: 'Terminal Hierarchy' },
            { mode: 'terminal_spatial' as ViewMode, label: 'Terminal 3D' },
          ]).map(tab => (
            <button
              key={tab.mode}
              onClick={() => setViewMode(tab.mode)}
              style={{
                padding: '8px 16px',
                background: viewMode === tab.mode ? '#1a1a3a' : 'transparent',
                border: 'none',
                borderBottom: viewMode === tab.mode ? '2px solid #00ffff' : '2px solid transparent',
                color: viewMode === tab.mode ? '#00ffff' : '#666',
                cursor: 'pointer',
                fontSize: '12px',
                fontWeight: viewMode === tab.mode ? 'bold' : 'normal',
              }}
            >
              {tab.label}
            </button>
          ))}
        </div>

        {/* View content */}
        <div style={{ flex: 1, position: 'relative' }}>
          {viewMode === 'tactical' && (
            <>
              <Map3D
                terrain={demoState.terrain}
                pieces={demoState.pieces}
                capabilities={demoState.capabilities}
                objectives={demoState.objectives}
                showPieces={showPieces}
                selectedCapability={selectedCapability}
                selectedObjective={selectedObjective}
              />
              <div style={{
                position: 'absolute',
                top: '16px',
                left: '16px',
                background: 'rgba(0,0,0,0.7)',
                padding: '12px',
                borderRadius: '8px',
                color: 'white',
              }}>
                <h2 style={{ margin: 0, fontSize: '18px', color: '#00ffff' }}>HIVE Commander</h2>
                <div style={{ marginTop: '8px', fontSize: '14px' }}>
                  Turn: {turn} | Score: {score} | Phase: {phase.toUpperCase().replace('_', ' ')}
                </div>
                <button
                  onClick={() => setShowPieces(!showPieces)}
                  style={{
                    marginTop: '8px',
                    padding: '8px 16px',
                    background: showPieces ? '#00aaff' : '#333',
                    border: 'none',
                    borderRadius: '4px',
                    color: 'white',
                    cursor: 'pointer',
                  }}
                >
                  {showPieces ? 'Show Capabilities' : 'Show Pieces'}
                </button>
              </div>
            </>
          )}

          {viewMode === 'terminal_hierarchy' && (
            <HierarchyTree
              topology={terminalTopology}
              selectedZone={selectedZone}
              selectedNodeId={selectedNodeId}
              onNodeClick={setSelectedNodeId}
            />
          )}

          {viewMode === 'terminal_spatial' && (
            <BerthScene
              topology={terminalTopology}
              selectedZone={selectedZone}
            />
          )}
        </div>

        {/* Event stream at bottom for terminal views */}
        {viewMode !== 'tactical' && (
          <div style={{ height: '200px', borderTop: '1px solid #333', background: '#0a0a14' }}>
            <EventStream events={terminalTopology.events} selectedZone={selectedZone} />
          </div>
        )}
      </div>

      {/* Right sidebar */}
      <div style={{
        width: '320px',
        background: '#0a0a14',
        borderLeft: '1px solid #333',
        display: 'flex',
        flexDirection: 'column',
        overflow: 'hidden',
      }}>
        {viewMode === 'tactical' ? (
          <>
            <div style={{ flex: 1, overflow: 'auto', padding: '16px' }}>
              <h3 style={{ color: '#00ffff', margin: '0 0 12px 0', fontSize: '14px' }}>
                COMPOSED CAPABILITIES
              </h3>
              {demoState.capabilities.map((cap) => (
                <CapabilityCard
                  key={cap.id}
                  capability={cap}
                  isSelected={selectedCapability === cap.id}
                  onClick={() => setSelectedCapability(cap.id === selectedCapability ? undefined : cap.id)}
                />
              ))}
            </div>
            <div style={{ borderTop: '1px solid #333', padding: '16px', maxHeight: '40%', overflow: 'auto' }}>
              <h3 style={{ color: '#ff44ff', margin: '0 0 12px 0', fontSize: '14px' }}>
                OBJECTIVES
              </h3>
              {demoState.objectives.filter(o => !o.completed).map((obj, idx) => (
                <div
                  key={obj.id}
                  onClick={() => setSelectedObjective(obj.id === selectedObjective ? undefined : obj.id)}
                  style={{
                    background: selectedObjective === obj.id ? '#3a1a4a' : '#1a1a2a',
                    border: selectedObjective === obj.id ? '2px solid #ff44ff' : '1px solid #333',
                    borderRadius: '8px',
                    padding: '10px',
                    marginBottom: '8px',
                    cursor: 'pointer',
                  }}
                >
                  <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '4px' }}>
                    <span style={{ color: '#ff44ff', fontWeight: 'bold', fontSize: '13px' }}>
                      [{idx + 1}] {obj.name}
                    </span>
                    <span style={{ color: '#888', fontSize: '11px' }}>{obj.points} pts</span>
                  </div>
                  <div style={{ fontSize: '11px', color: '#888', marginBottom: '4px' }}>
                    ({obj.x}, {obj.y}) - {obj.description}
                  </div>
                  <div style={{ display: 'flex', gap: '8px', fontSize: '10px' }}>
                    {obj.detectRequired > 0 && <span style={{ color: '#44ff44' }}>DET:{obj.detectRequired}</span>}
                    {obj.trackRequired > 0 && <span style={{ color: '#ffff44' }}>TRK:{obj.trackRequired}</span>}
                    {obj.strikeRequired > 0 && <span style={{ color: '#ff4444' }}>STR:{obj.strikeRequired}</span>}
                    {obj.authorizeRequired > 0 && <span style={{ color: '#ff44ff' }}>AUTH:{obj.authorizeRequired}</span>}
                  </div>
                </div>
              ))}
            </div>
          </>
        ) : (
          <TeamSummaryHUD
            topology={terminalTopology}
            selectedZone={selectedZone}
            onZoneSelect={setSelectedZone}
          />
        )}
      </div>
    </div>
  );
}
