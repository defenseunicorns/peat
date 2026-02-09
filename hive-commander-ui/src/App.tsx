import { useState, useMemo, useEffect } from 'react';
import Map3D from './components/Map3D/Map3D';
import { CapabilityCard } from './components/PieceCard/PieceCard';
import EventStream from './components/EventStream/EventStream';
import { GapAnalysisPanel } from './components/GapAnalysisPanel/GapAnalysisPanel';
import { ConnectionStatus } from './components/ConnectionStatus/ConnectionStatus';
import { useCommanderStore } from './protocol/store';
import { TerrainType, Piece, ComposedCapability, Objective, GamePhase, HiveEvent, GapAnalysisReport } from './types';

// Generate simple terrain for demo
function generateTerrain(width: number, height: number): TerrainType[][] {
  const terrain: TerrainType[][] = [];
  for (let y = 0; y < height; y++) {
    const row: TerrainType[] = [];
    for (let x = 0; x < width; x++) {
      // Simple pattern for demo
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
  // Add bases
  terrain[height / 2][2] = 'base';
  terrain[height / 2][width - 3] = 'base';
  // Add urban
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
    // Red pieces
    { id: 10, pieceType: { type: 'sensor', mode: 'ir' }, team: 'red', x: 15, y: 4, fuel: 10, maxFuel: 10 },
    { id: 11, pieceType: { type: 'striker' }, team: 'red', x: 16, y: 5, fuel: 10, maxFuel: 10 },
  ];

  const capabilities: ComposedCapability[] = [
    {
      id: 0,
      name: 'AI_ISR-1',
      pieceIds: [0, 1, 2, 8],
      centerX: 4,
      centerY: 5,
      detectBonus: 10,
      trackBonus: 8,
      strikeBonus: 0,
      reconBonus: 1,
      authorizeBonus: 0,
      relayBonus: 0,
      classifyBonus: 3,
      predictBonus: 2,
      fuseBonus: 3,
      totalFuel: 35,
      maxFuel: 40,
      team: 'blue',
      confidence: 0.92,
      decayRate: -0.02,
      equipmentHealth: [{ label: 'UAV-1 Sensor', confidence: 0.95, status: 'nominal' as const }, { label: 'UAV-2 Optics', confidence: 0.88, status: 'nominal' as const }],
    },
    {
      id: 1,
      name: 'STRIKE-2',
      pieceIds: [4, 5, 7],
      centerX: 6,
      centerY: 5,
      detectBonus: 0,
      trackBonus: 0,
      strikeBonus: 8,
      reconBonus: 0,
      authorizeBonus: 5,
      relayBonus: 0,
      classifyBonus: 0,
      predictBonus: 0,
      fuseBonus: 0,
      totalFuel: 29,
      maxFuel: 30,
      team: 'blue',
      confidence: 0.78,
      decayRate: -0.03,
      equipmentHealth: [{ label: 'Effector-1', confidence: 0.80, status: 'degraded' as const }, { label: 'Comms Relay', confidence: 0.75, status: 'degraded' as const }],
    },
    {
      id: 2,
      name: 'RECON-3',
      pieceIds: [3, 6],
      centerX: 5,
      centerY: 6,
      detectBonus: 3,
      trackBonus: 2,
      strikeBonus: 0,
      reconBonus: 3,
      authorizeBonus: 0,
      relayBonus: 3,
      classifyBonus: 0,
      predictBonus: 0,
      fuseBonus: 0,
      totalFuel: 16,
      maxFuel: 20,
      team: 'blue',
      confidence: 0.95,
      decayRate: -0.01,
      equipmentHealth: [{ label: 'Scout Drone', confidence: 0.97, status: 'nominal' as const }],
    },
  ];

  const objectives: Objective[] = [
    {
      id: 0,
      name: 'TRACK HVT',
      description: 'Locate and track high-value target',
      x: 10,
      y: 4,
      detectRequired: 2,
      trackRequired: 2,
      strikeRequired: 0,
      authorizeRequired: 0,
      completed: false,
      turnsRemaining: 0,
      points: 100,
    },
    {
      id: 1,
      name: 'SECURE AREA',
      description: 'Establish presence and secure zone',
      x: 12,
      y: 7,
      detectRequired: 1,
      trackRequired: 0,
      strikeRequired: 2,
      authorizeRequired: 2,
      completed: false,
      turnsRemaining: 0,
      points: 150,
    },
    {
      id: 2,
      name: 'ANALYZE TARGET',
      description: 'AI-assisted target analysis',
      x: 8,
      y: 5,
      detectRequired: 2,
      trackRequired: 1,
      strikeRequired: 0,
      authorizeRequired: 0,
      completed: false,
      turnsRemaining: 0,
      points: 75,
    },
  ];

  const now = Date.now();
  const events: HiveEvent[] = [
    {
      id: 'evt-001', timestamp: now - 120000, sourceNodeId: 'Alpha-1',
      eventClass: 'product', eventType: 'detection', category: 'operational',
      priority: 'normal', message: 'vehicle at (10,5), confidence: 0.92',
    },
    {
      id: 'evt-002', timestamp: now - 110000, sourceNodeId: 'Alpha-1',
      eventClass: 'product', eventType: 'track_new', category: 'operational',
      priority: 'normal', message: 'Track #42 established',
      causeEventId: 'evt-001',
    },
    {
      id: 'evt-003', timestamp: now - 95000, sourceNodeId: 'Alpha-2',
      eventClass: 'product', eventType: 'classification', category: 'operational',
      priority: 'normal', message: 'Track #42 classified: armored vehicle',
    },
    {
      id: 'evt-004', timestamp: now - 80000, sourceNodeId: 'Alpha-1',
      eventClass: 'product', eventType: 'ooda_observe', category: 'operational',
      priority: 'normal', message: 'Sector 7 sweep complete',
    },
    {
      id: 'evt-005', timestamp: now - 70000, sourceNodeId: 'Alpha-3',
      eventClass: 'product', eventType: 'container_move', category: 'operational',
      priority: 'low', message: 'Sensor package relocated to grid (8,4)',
    },
    {
      id: 'evt-010', timestamp: now - 60000, sourceNodeId: 'crane-2',
      eventClass: 'anomaly', eventType: 'capability_degraded', category: 'logistical',
      priority: 'high', message: 'hydraulic_pct dropped',
      metric: { name: 'hydraulic_pct', value: 65, unit: '%' },
      effectEventIds: ['evt-011'],
    },
    {
      id: 'evt-011', timestamp: now - 55000, sourceNodeId: 'crane-2',
      eventClass: 'product', eventType: 'maintenance_scheduled', category: 'logistical',
      priority: 'high', message: 'Scheduled for crane-2 hydraulic system',
      causeEventId: 'evt-010', effectEventIds: ['evt-012'],
    },
    {
      id: 'evt-012', timestamp: now - 40000, sourceNodeId: 'crane-2',
      eventClass: 'product', eventType: 'maintenance_started', category: 'logistical',
      priority: 'normal', message: 'Technician dispatched to crane-2',
      causeEventId: 'evt-011', effectEventIds: ['evt-013'],
    },
    {
      id: 'evt-013', timestamp: now - 20000, sourceNodeId: 'crane-2',
      eventClass: 'product', eventType: 'maintenance_complete', category: 'logistical',
      priority: 'normal', message: 'Hydraulic system restored',
      causeEventId: 'evt-012', effectEventIds: ['evt-014'],
    },
    {
      id: 'evt-014', timestamp: now - 15000, sourceNodeId: 'crane-2',
      eventClass: 'product', eventType: 'capability_restored', category: 'logistical',
      priority: 'normal', message: 'crane-2 operational',
      metric: { name: 'hydraulic_pct', value: 95, unit: '%' },
      causeEventId: 'evt-013',
    },
    {
      id: 'evt-020', timestamp: now - 50000, sourceNodeId: 'Bravo-2',
      eventClass: 'anomaly', eventType: 'resupply_requested', category: 'logistical',
      priority: 'high', message: 'Ammunition below threshold',
      metric: { name: 'ammo', value: 2, unit: '/30' },
    },
    {
      id: 'evt-021', timestamp: now - 30000, sourceNodeId: 'Bravo-2',
      eventClass: 'product', eventType: 'resupply_delivered', category: 'logistical',
      priority: 'normal', message: 'Ammunition resupplied',
      causeEventId: 'evt-020',
    },
    {
      id: 'evt-030', timestamp: now - 45000, sourceNodeId: 'Charlie-3',
      eventClass: 'product', eventType: 'shift_started', category: 'logistical',
      priority: 'normal', message: 'Watch rotation Alpha to Bravo',
    },
    {
      id: 'evt-040', timestamp: now - 35000, sourceNodeId: 'Delta-1',
      eventClass: 'anomaly', eventType: 'recertification_required', category: 'logistical',
      priority: 'normal', message: 'Sensor calibration due',
    },
    {
      id: 'evt-050', timestamp: now - 10000, sourceNodeId: 'Alpha-1',
      eventClass: 'product', eventType: 'track_lost', category: 'operational',
      priority: 'high', message: 'Track #42 lost, last seen (12,6)',
    },
    {
      id: 'evt-051', timestamp: now - 5000, sourceNodeId: 'Alpha-3',
      eventClass: 'product', eventType: 'engagement_active', category: 'operational',
      priority: 'critical', message: 'Platform Alpha-3 engaging target sector 9',
    },
  ];

  events.sort((a, b) => a.timestamp - b.timestamp);

  const gapReports: GapAnalysisReport[] = [
    {
      level: 'H2',
      locationId: 'hold-3',
      locationLabel: 'Hold 3 — Alpha Squad',
      readinessScore: 0.62,
      worstHealth: 'degraded',
      operationalCount: 6,
      totalCount: 8,
      gaps: [
        {
          capabilityName: 'HAZMAT_HANDLING',
          capabilityType: 'payload',
          requiredConfidence: 0.8,
          currentConfidence: 0.65,
          decayRate: -0.05,
          etaThresholdBreach: null,
          reason: 'op-2 HAZMAT cert expired, recertification in progress',
          requiresOversight: true,
          maxAuthority: 'supervisor',
          contributorCount: 2,
          pendingActions: [
            {
              id: 'act-1',
              description: 'Recertify op-2 HAZMAT handling',
              etaMinutes: 20,
              status: 'in_progress',
              blockedBy: null,
            },
          ],
        },
        {
          capabilityName: 'SENSOR_FUSION',
          capabilityType: 'sensor',
          requiredConfidence: 0.7,
          currentConfidence: 0.58,
          decayRate: -0.04,
          etaThresholdBreach: null,
          reason: 'IR sensor-3 offline, radar-1 degraded calibration',
          requiresOversight: false,
          maxAuthority: null,
          contributorCount: 1,
          pendingActions: [
            {
              id: 'act-2',
              description: 'Replace IR sensor-3 unit',
              etaMinutes: 45,
              status: 'blocked',
              blockedBy: 'Spare parts in transit from depot',
            },
            {
              id: 'act-3',
              description: 'Recalibrate radar-1',
              etaMinutes: 15,
              status: 'in_progress',
              blockedBy: null,
            },
          ],
        },
        {
          capabilityName: 'COMMS_RELAY',
          capabilityType: 'communication',
          requiredConfidence: 0.8,
          currentConfidence: 0.72,
          decayRate: -0.02,
          etaThresholdBreach: 4,
          reason: 'Relay node battery at 18%, backup relay not positioned',
          requiresOversight: true,
          maxAuthority: 'commander',
          contributorCount: 3,
          pendingActions: [
            {
              id: 'act-4',
              description: 'Hot-swap relay node battery',
              etaMinutes: 10,
              status: 'pending',
              blockedBy: null,
            },
          ],
        },
      ],
      logisticalDependencies: [
        {
          resourceName: 'Crane-2',
          status: 'unavailable',
          reason: 'Needs maintenance — no maintenance crew available until shift change',
          availableInMinutes: 90,
          affectedCapabilities: ['HEAVY_LIFT', 'CARGO_TRANSFER'],
        },
        {
          resourceName: 'Maintenance Crew Bravo',
          status: 'unavailable',
          reason: 'Assigned to Hold 1 emergency repairs',
          availableInMinutes: 45,
          affectedCapabilities: ['SENSOR_FUSION', 'HAZMAT_HANDLING'],
        },
      ],
      aggregatedAt: new Date().toISOString(),
    },
    {
      level: 'H3',
      locationId: 'berth-7A',
      locationLabel: 'Berth 7A — Dock Wing',
      readinessScore: 0.81,
      worstHealth: 'nominal',
      operationalCount: 22,
      totalCount: 24,
      gaps: [
        {
          capabilityName: 'AI_TARGET_ANALYSIS',
          capabilityType: 'compute',
          requiredConfidence: 0.75,
          currentConfidence: 0.68,
          decayRate: -0.01,
          etaThresholdBreach: 7,
          reason: 'GPU node-4 thermal throttling, reduced inference throughput',
          requiresOversight: false,
          maxAuthority: null,
          contributorCount: 3,
          pendingActions: [
            {
              id: 'act-5',
              description: 'Cool down and restart GPU node-4',
              etaMinutes: 8,
              status: 'in_progress',
              blockedBy: null,
            },
          ],
        },
      ],
      logisticalDependencies: [
        {
          resourceName: 'Power Grid Sector-7',
          status: 'degraded',
          reason: 'Running on backup generator, 60% capacity',
          availableInMinutes: null,
          affectedCapabilities: ['AI_TARGET_ANALYSIS', 'COMMS_RELAY'],
        },
      ],
      aggregatedAt: new Date().toISOString(),
    },
    {
      level: 'H2',
      locationId: 'hold-1',
      locationLabel: 'Hold 1 — Bravo Squad',
      readinessScore: 0.91,
      worstHealth: 'nominal',
      operationalCount: 8,
      totalCount: 8,
      gaps: [],
      logisticalDependencies: [],
      aggregatedAt: new Date().toISOString(),
    },
  ];

  return { terrain, pieces, capabilities, objectives, events, gapReports };
}

export default function App() {
  const [showPieces, setShowPieces] = useState(false);
  const [selectedCapability, setSelectedCapability] = useState<number | undefined>(undefined);
  const [selectedObjective, setSelectedObjective] = useState<number | undefined>(undefined);
  const [phase] = useState<GamePhase>('select_objective');
  const [turn] = useState(1);
  const [score] = useState(0);
  const [playbackSpeed, setPlaybackSpeed] = useState(1);
  const [sidebarTab, setSidebarTab] = useState<'status' | 'gaps'>('status');

  const demoState = useMemo(() => createDemoState(), []);

  // Live data from WebSocket store (falls back to demo data when offline)
  const storeStatus = useCommanderStore((s) => s.status);
  const storeCapabilities = useCommanderStore((s) => s.capabilities);
  const storeEvents = useCommanderStore((s) => s.events);
  const storeGapReports = useCommanderStore((s) => s.gapReports);
  const connect = useCommanderStore((s) => s.connect);
  const disconnect = useCommanderStore((s) => s.disconnect);

  // Connect on mount, disconnect on unmount
  useEffect(() => {
    connect();
    return () => disconnect();
  }, [connect, disconnect]);

  // Use live data when connected and store has data, otherwise fall back to demo
  const isLive = storeStatus === 'connected';
  const capabilities = isLive && storeCapabilities.length > 0 ? storeCapabilities : demoState.capabilities;
  const events = isLive && storeEvents.length > 0 ? storeEvents : demoState.events;
  const gapReports = isLive && storeGapReports.length > 0 ? storeGapReports : demoState.gapReports;

  return (
    <div style={{ display: 'flex', width: '100%', height: '100%' }}>
      {/* Main 3D Map */}
      <div style={{ flex: 1, position: 'relative' }}>
        <Map3D
          terrain={demoState.terrain}
          pieces={demoState.pieces}
          capabilities={demoState.capabilities}
          objectives={demoState.objectives}
          showPieces={showPieces}
          selectedCapability={selectedCapability}
          selectedObjective={selectedObjective}
        />

        {/* Overlay controls */}
        <div style={{
          position: 'absolute',
          top: '16px',
          left: '16px',
          background: 'rgba(0,0,0,0.7)',
          padding: '12px',
          borderRadius: '8px',
          color: 'white',
        }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
            <h2 style={{ margin: 0, fontSize: '18px', color: '#00ffff' }}>HIVE Commander</h2>
            <ConnectionStatus />
          </div>
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
        {/* Sidebar tab bar */}
        <div style={{
          display: 'flex',
          borderBottom: '1px solid #333',
          background: '#0a0a14',
          flexShrink: 0,
        }}>
          <button
            onClick={() => setSidebarTab('status')}
            style={{
              flex: 1,
              padding: '10px',
              border: 'none',
              borderBottom: sidebarTab === 'status' ? '2px solid #00ffff' : '2px solid transparent',
              background: 'transparent',
              color: sidebarTab === 'status' ? '#00ffff' : '#555',
              fontSize: '12px',
              fontWeight: 'bold',
              cursor: 'pointer',
            }}
          >
            STATUS
          </button>
          <button
            onClick={() => setSidebarTab('gaps')}
            style={{
              flex: 1,
              padding: '10px',
              border: 'none',
              borderBottom: sidebarTab === 'gaps' ? '2px solid #ff8844' : '2px solid transparent',
              background: 'transparent',
              color: sidebarTab === 'gaps' ? '#ff8844' : '#555',
              fontSize: '12px',
              fontWeight: 'bold',
              cursor: 'pointer',
              position: 'relative',
            }}
          >
            GAPS
            {gapReports.reduce((sum, r) => sum + r.gaps.filter(g => g.currentConfidence < g.requiredConfidence).length, 0) > 0 && (
              <span style={{
                position: 'absolute',
                top: '6px',
                right: '20px',
                width: '8px',
                height: '8px',
                borderRadius: '50%',
                background: '#ff4444',
              }} />
            )}
          </button>
        </div>

        {sidebarTab === 'status' ? (
          <>
            {/* Capabilities panel */}
            <div style={{ flex: 1, overflow: 'auto', padding: '16px' }}>
              <h3 style={{ color: '#00ffff', margin: '0 0 12px 0', fontSize: '14px' }}>
                COMPOSED CAPABILITIES
              </h3>
              {capabilities.map((cap) => (
                <CapabilityCard
                  key={cap.id}
                  capability={cap}
                  isSelected={selectedCapability === cap.id}
                  onClick={() => setSelectedCapability(cap.id === selectedCapability ? undefined : cap.id)}
                />
              ))}
            </div>

            {/* Objectives panel */}
            <div style={{ borderTop: '1px solid #333', padding: '16px', maxHeight: '30%', overflow: 'auto' }}>
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
                    <span style={{ color: '#888', fontSize: '11px' }}>
                      {obj.points} pts
                    </span>
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

            {/* Event Stream panel */}
            <div style={{ borderTop: '1px solid #333', height: '35%', display: 'flex', flexDirection: 'column' }}>
              {/* Playback controls */}
              <div style={{
                padding: '6px 12px',
                borderBottom: '1px solid #222',
                display: 'flex',
                alignItems: 'center',
                gap: '6px',
                fontSize: '12px',
              }}>
                <button
                  onClick={() => setPlaybackSpeed(playbackSpeed === 0 ? 1 : 0)}
                  style={{
                    padding: '2px 8px',
                    background: playbackSpeed === 0 ? '#332200' : '#1a1a2a',
                    border: `1px solid ${playbackSpeed === 0 ? '#ff8800' : '#333'}`,
                    borderRadius: '3px',
                    color: playbackSpeed === 0 ? '#ff8800' : '#ccc',
                    cursor: 'pointer',
                    fontSize: '11px',
                  }}
                >
                  {playbackSpeed === 0 ? '▶ Play' : '⏸ Pause'}
                </button>
                <span style={{ color: '#555', fontSize: '10px' }}>Speed:</span>
                {[0.5, 1, 2, 4].map(speed => (
                  <button
                    key={speed}
                    onClick={() => setPlaybackSpeed(speed)}
                    style={{
                      padding: '1px 5px',
                      background: playbackSpeed === speed ? '#1a2a3a' : 'transparent',
                      border: `1px solid ${playbackSpeed === speed ? '#00aaff' : '#333'}`,
                      borderRadius: '3px',
                      color: playbackSpeed === speed ? '#00aaff' : '#555',
                      cursor: 'pointer',
                      fontSize: '10px',
                    }}
                  >
                    {speed}x
                  </button>
                ))}
              </div>
              <div style={{ flex: 1, overflow: 'hidden' }}>
                <EventStream events={events} playbackSpeed={playbackSpeed} />
              </div>
            </div>
          </>
        ) : (
          /* Gap Analysis panel */
          <div style={{ flex: 1, overflow: 'auto', padding: '16px' }}>
            <GapAnalysisPanel reports={gapReports} />
          </div>
        )}
      </div>
    </div>
  );
}
