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

// Health status thresholds per ADR-053:
// confidence >= 0.7 → NOMINAL (green)
// confidence >= 0.4 → DEGRADED (yellow/amber)
// confidence > 0.0  → CRITICAL (red)
// confidence == 0.0 → OFFLINE (gray)

export interface EquipmentHealth {
  label: string;         // e.g. "hydraulic", "electrical", "comms"
  confidence: number;    // 0.0 - 1.0
  status: HealthStatus;
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
  // Health / confidence (capability lifecycle)
  confidence: number;          // 0.0 - 1.0 overall confidence
  decayRate: number;           // per-turn decay rate (negative = decaying, 0 = stable, positive = recovering)
  equipmentHealth: EquipmentHealth[];
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

// Event types matching hive-schema/proto/event.proto
export type EventClass = 'product' | 'anomaly' | 'telemetry' | 'command';
export type EventPriority = 'critical' | 'high' | 'normal' | 'low';

// Logistical event subtypes
export type LogisticalEventType =
  | 'maintenance_scheduled'
  | 'maintenance_started'
  | 'maintenance_complete'
  | 'resupply_requested'
  | 'resupply_delivered'
  | 'recertification_required'
  | 'recertification_complete'
  | 'shift_started'
  | 'shift_ended'
  | 'capability_degraded'
  | 'capability_restored';

// Operational event subtypes
export type OperationalEventType =
  | 'detection'
  | 'track_new'
  | 'track_update'
  | 'track_lost'
  | 'classification'
  | 'container_move'
  | 'ooda_observe'
  | 'ooda_orient'
  | 'ooda_decide'
  | 'ooda_act'
  | 'engagement_active'
  | 'effector_fired';

export type HiveEventType = LogisticalEventType | OperationalEventType;

export type EventCategory = 'operational' | 'logistical';

export interface HiveEvent {
  id: string;
  timestamp: number;
  sourceNodeId: string;
  eventClass: EventClass;
  eventType: HiveEventType;
  category: EventCategory;
  priority: EventPriority;
  message: string;
  // For cause-effect chains
  causeEventId?: string;
  effectEventIds?: string[];
  // Metric context (e.g., hydraulic_pct: 65)
  metric?: { name: string; value: number; unit: string };
}

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

// ============================================================================
// Gap Analysis & Logistical Dependencies (hi-9r0.7)
// Matches Rust hive-protocol/src/cell/capability_aggregation.rs
// ============================================================================

export type CapabilityType =
  | 'sensor'
  | 'compute'
  | 'communication'
  | 'mobility'
  | 'payload'
  | 'emergent';

export type HealthStatus = 'nominal' | 'degraded' | 'critical' | 'failed' | 'offline';

export type AuthorityLevel = 'observer' | 'advisor' | 'supervisor' | 'commander';

export type HierarchyLevel = 'H2' | 'H3';

/** A single capability gap reported by the hold aggregator */
export interface CapabilityGap {
  /** Capability name (e.g. "HAZMAT_HANDLING") */
  capabilityName: string;
  capabilityType: CapabilityType;
  /** Required confidence threshold (0.0–1.0) */
  requiredConfidence: number;
  /** Current aggregated confidence (0.0–1.0) */
  currentConfidence: number;
  /** Confidence decay per turn (negative value) */
  decayRate: number;
  /** Estimated turns until confidence drops below requiredConfidence */
  etaThresholdBreach: number | null;
  /** Why the gap exists */
  reason: string;
  /** Pending actions that will restore this capability */
  pendingActions: LogisticalAction[];
  /** Whether human oversight is required */
  requiresOversight: boolean;
  /** Max authority level among contributors */
  maxAuthority: AuthorityLevel | null;
  /** Number of contributing platforms */
  contributorCount: number;
}

/** A logistical action that can restore or maintain a capability */
export interface LogisticalAction {
  id: string;
  description: string;
  /** Expected time to completion (minutes) */
  etaMinutes: number | null;
  status: 'pending' | 'in_progress' | 'blocked';
  /** What blocks this action, if anything */
  blockedBy: string | null;
}

/** A logistical dependency between resources */
export interface LogisticalDependency {
  /** Resource name (e.g. "Crane-2", "Maintenance Crew Alpha") */
  resourceName: string;
  /** Current status */
  status: 'available' | 'unavailable' | 'degraded';
  /** Human-readable explanation */
  reason: string;
  /** When the resource becomes available (minutes from now, null if unknown) */
  availableInMinutes: number | null;
  /** Capabilities affected by this dependency */
  affectedCapabilities: string[];
}

/** Hold-level (H2) or berth-level (H3) gap analysis report */
export interface GapAnalysisReport {
  /** Hierarchy level */
  level: HierarchyLevel;
  /** Hold or berth identifier */
  locationId: string;
  /** Human-readable location label */
  locationLabel: string;
  /** Overall readiness score (0.0–1.0) */
  readinessScore: number;
  /** Worst health status across members */
  worstHealth: HealthStatus;
  /** Operational member count / total */
  operationalCount: number;
  totalCount: number;
  /** Capability gaps */
  gaps: CapabilityGap[];
  /** Cross-cutting logistical dependencies */
  logisticalDependencies: LogisticalDependency[];
  /** Timestamp of last aggregation */
  aggregatedAt: string;
}

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

// Health visualization utilities
export function getHealthStatus(confidence: number): HealthStatus {
  if (confidence <= 0) return 'offline';
  if (confidence < 0.4) return 'critical';
  if (confidence < 0.7) return 'degraded';
  return 'nominal';
}

export function getHealthColor(confidence: number): string {
  if (confidence <= 0) return '#666666';   // gray - offline
  if (confidence < 0.4) return '#ff4444';  // red - critical
  if (confidence < 0.7) return '#ffaa00';  // yellow/amber - degraded
  return '#44ff44';                         // green - nominal
}

export function getWorstConfidence(cap: ComposedCapability): number {
  if (cap.equipmentHealth.length === 0) return cap.confidence;
  return Math.min(cap.confidence, ...cap.equipmentHealth.map(e => e.confidence));
}
