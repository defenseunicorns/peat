// Domain types matching Rust hive-protocol/src/models/domain.rs
export type Domain = 'subsurface' | 'surface' | 'air';

// Terrain types matching Rust hive-commander
export type TerrainType =
  | 'deep_water'
  | 'shallow_water'
  | 'plains'
  | 'forest'
  | 'hills'
  | 'mountain'
  | 'urban'
  | 'base';

// Piece types matching Rust hive-commander
export type PieceType =
  | { type: 'sensor'; mode: DetectionMode }
  | { type: 'scout' }
  | { type: 'striker' }
  | { type: 'support' }
  | { type: 'authority' }
  | { type: 'analyst' };

export type DetectionMode = 'eo' | 'ir' | 'radar' | 'acoustic' | 'sigint';

export type Team = 'blue' | 'red';

export interface Piece {
  id: number;
  pieceType: PieceType;
  team: Team;
  x: number;
  y: number;
  fuel: number;
  maxFuel: number;
}

export interface ComposedCapability {
  id: number;
  name: string;
  pieceIds: number[];
  centerX: number;
  centerY: number;
  // Bonuses
  detectBonus: number;
  trackBonus: number;
  strikeBonus: number;
  reconBonus: number;
  authorizeBonus: number;
  relayBonus: number;
  // Analyst bonuses
  classifyBonus: number;
  predictBonus: number;
  fuseBonus: number;
  // Status
  totalFuel: number;
  maxFuel: number;
  team: Team;
}

export interface Objective {
  id: number;
  name: string;
  description: string;
  x: number;
  y: number;
  // Requirements
  detectRequired: number;
  trackRequired: number;
  strikeRequired: number;
  authorizeRequired: number;
  // Status
  completed: boolean;
  assignedCapability?: number;
  turnsRemaining: number;
  points: number;
}

export interface CourseOfAction {
  capabilityId: number;
  capabilityName: string;
  objectiveId: number;
  turnsToComplete: number;
  fuelCost: number;
  successChance: number;
  riskLevel: 'low' | 'medium' | 'high';
  description: string;
}

export type GamePhase = 'select_objective' | 'select_coa' | 'executing' | 'enemy_turn';

export interface GameState {
  width: number;
  height: number;
  terrain: TerrainType[][];
  pieces: Piece[];
  capabilities: ComposedCapability[];
  objectives: Objective[];
  currentCoas: CourseOfAction[];
  turn: number;
  phase: GamePhase;
  selectedObjective?: number;
  selectedCoa?: number;
  message: string;
  score: number;
}

// Capability lifecycle event types for viewer relay
// Matches Rust hive-transport/src/ws/protocol.rs ViewerEvent enum
// Confidence values use the canonical [0.0, 1.0] range from capability.proto

export interface CapabilityDegradationEvent {
  type: 'capability_degradation';
  /** Node whose capability degraded */
  node_id: string;
  /** Type of capability affected (e.g., "sensor", "compute", "mobility") */
  capability_type: string;
  /** Confidence score before degradation [0.0, 1.0] */
  confidence_before: number;
  /** Confidence score after degradation [0.0, 1.0] */
  confidence_after: number;
  /** Human-readable cause of degradation */
  cause: string;
  /** Rate of confidence decay per hour */
  decay_rate_per_hour: number;
}

export interface LogisticalEventData {
  type: 'logistical_event';
  /** Node involved in the logistical event */
  node_id: string;
  /** Specific event subtype (e.g., "maintenance_scheduled", "resupply_delivered") */
  event_subtype: string;
  /** Capability being sustained or restored */
  capability_sustained: string;
  /** Estimated time to restore capability (seconds), if applicable */
  eta_restore: number | null;
  /** Human-readable details about the logistical event */
  details: string;
}

export type ViewerEvent = CapabilityDegradationEvent | LogisticalEventData;

// Terrain utilities
export const terrainColors: Record<TerrainType, string> = {
  deep_water: '#141428',
  shallow_water: '#1e1e32',
  plains: '#4a5c3c',
  forest: '#1a4a1a',
  hills: '#8a7a50',
  mountain: '#3c3c3c',
  urban: '#5a5a5a',
  base: '#8a2a8a',
};

export const terrainElevation: Record<TerrainType, number> = {
  deep_water: -1,
  shallow_water: 0,
  plains: 0,
  forest: 0.1,
  hills: 0.5,
  mountain: 1,
  urban: 0.1,
  base: 0,
};
