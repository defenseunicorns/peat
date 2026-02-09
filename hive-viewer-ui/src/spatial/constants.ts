/** Layout positions and colors for port operations spatial scene. */

// Scene units: 1 unit ≈ 5m

export const CAMERA = {
  position: [0, 30, 20] as const,
  zoom: 20,
};

export const VESSEL = {
  length: 36,
  beam: 8,
  y: 0.25,
  color: '#1e3a5f',
  label: 'MV Ever Forward',
};

export const HOLDS = {
  count: 9,
  cols: 9,
  cellWidth: 3.6,
  cellDepth: 7,
  highlightIndex: 2, // hold-3 (0-indexed)
  startX: -14.4,
  z: 0,
};

export const CRANE_POSITIONS: Record<string, { x: number; z: number }> = {
  'crane-1': { x: -4, z: -6 },
  'crane-2': { x: 4, z: -6 },
};

export const OPERATOR_POSITIONS: Record<string, { x: number; z: number }> = {
  'op-1': { x: -6, z: -4 },
  'op-2': { x: 6, z: -4 },
  'op-3': { x: -3, z: -4 },
  'op-4': { x: 0, z: -4 },
  'op-5': { x: 3, z: -4 },
};

export const TRACTOR_POSITIONS: Record<string, { x: number; z: number }> = {
  'tractor-1': { x: -9, z: -9 },
  'tractor-2': { x: -3, z: -9 },
  'tractor-3': { x: 3, z: -9 },
  'tractor-4': { x: 9, z: -9 },
};

export const SENSOR_POSITIONS: Record<string, { x: number; z: number }> = {
  'load-cell-1': { x: -4, z: -4.5 },
  'rfid-1': { x: 4, z: -4.5 },
};

export const CONTAINER_GRID = {
  cols: 5,
  rows: 4,
  total: 20,
  hazmatCount: 3,
  cellSize: 0.6,
  gap: 0.15,
  // Grid centered within hold-3
  originX: HOLDS.startX + HOLDS.highlightIndex * HOLDS.cellWidth + HOLDS.cellWidth / 2,
  originZ: 0,
};

export const YARD = {
  blockCount: 6,
  blockWidth: 4,
  blockDepth: 2.5,
  gap: 1.5,
  z: -12,
  startX: -14,
  labels: ['YB-A', 'YB-B', 'YB-C', 'YB-D', 'YB-E', 'YB-F'],
};

export const COLORS = {
  water: '#0a1628',
  berth: '#2a2a2a',
  vessel: '#1e3a5f',
  vesselDeck: '#2a4a6f',

  containerPending: '#4a4a4a',
  containerInProgress: '#eab308',
  containerCompleted: '#22c55e',
  containerHazmat: '#ef4444',
  containerHazmatEmissive: '#991b1b',

  craneOperational: '#22d3ee',
  craneDegraded: '#eab308',
  craneFailed: '#ef4444',
  craneContention: '#f59e0b',

  holdDefault: '#1a2744',
  holdHighlight: '#1e3a5f',
  holdHighlightEmissive: '#0e2340',

  yard: '#333333',

  operatorAvailable: '#22c55e',
  operatorAssigned: '#22d3ee',
  operatorBreak: '#6b7280',

  tractorIdle: '#6b7280',
  tractorMoving: '#f59e0b',
  tractorCharging: '#eab308',

  sensorActive: '#3b82f6',
  sensorDrifting: '#f59e0b',

  schedulerActive: '#a78bfa',

  text: '#e5e7eb',
  textDim: '#6b7280',
};
