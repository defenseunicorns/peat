/** Event protocol types — mirrors Rust ViewerEvent types from hive-viewer. */

export type ViewerEvent =
  | StateSnapshotEvent
  | OodaCycleEvent
  | DocumentUpdateEvent
  | HiveEventEvent
  | SimClockEvent;

export interface StateSnapshotEvent {
  type: 'state_snapshot';
  documents: Record<string, unknown>;
  events: HiveEvent[];
  sim_clock: SimClock | null;
}

export interface OodaCycleEvent {
  type: 'ooda_cycle';
  node_id: string;
  cycle: number;
  sim_time: string;
  action: string;
  success: boolean;
  contention_retry: boolean;
  observe_ms: number;
  decide_ms: number;
  act_ms: number;
  /** Extra fields from the sim (container_id, total_ms, etc.) */
  [key: string]: unknown;
}

export interface DocumentUpdateEvent {
  type: 'document_update';
  collection: string;
  doc_id: string;
  fields: unknown;
}

export interface HiveEventEvent {
  type: 'hive_event';
  event_type: string;
  source: string;
  priority: string;
  details: unknown;
  timestamp: string | null;
}

export interface HiveEvent {
  event_type: string;
  source: string;
  priority: string;
  details: unknown;
  timestamp: string | null;
}

export interface SimClockEvent {
  type: 'sim_clock';
  sim_time: string;
  real_elapsed_ms: number;
}

export interface SimClock {
  sim_time: string;
  real_elapsed_ms: number;
}

/** Per-subsystem health state (from CAPABILITY_DEGRADED events). */
export interface SubsystemHealth {
  confidence: number;
  status: 'NOMINAL' | 'DEGRADED' | 'CRITICAL' | 'OFFLINE';
}

/** Per-resource level (from RESOURCE_CONSUMED events). */
export interface ResourceLevel {
  value: number;  // 0-100 percent
}

/** Lifecycle state aggregated from hive_event messages. */
export interface LifecycleState {
  subsystems: Record<string, SubsystemHealth>;
  resources: Record<string, ResourceLevel>;
  equipmentState: string;  // OPERATIONAL | RESUPPLYING
  maintenanceJobs: string[];  // active subsystem maintenance
  gapReport: {
    readinessScore: number;
    gaps: Array<{ name: string; confidence: number; required: number; status: string }>;
  } | null;
}

/** Parsed OODA cycle data stored per-node. */
export interface NodeState {
  node_id: string;
  cycle: number;
  sim_time: string;
  action: string;
  success: boolean;
  contention_retry: boolean;
  observe_ms: number;
  decide_ms: number;
  act_ms: number;
  total_ms?: number;
  /** Role inferred from node_id pattern. */
  role: 'crane' | 'aggregator' | 'operator' | 'tractor' | 'scheduler' | 'sensor' | 'berth_manager' | 'yard_block' | 'unknown';
  /** All cycles received for this node. */
  history: OodaCycleData[];
  /** Lifecycle state from degradation/resource/gap events. */
  lifecycle: LifecycleState;
}

export interface OodaCycleData {
  cycle: number;
  sim_time: string;
  action: string;
  success: boolean;
  contention_retry: boolean;
  observe_ms: number;
  decide_ms: number;
  act_ms: number;
  total_ms?: number;
}

export function inferRole(nodeId: string): 'crane' | 'aggregator' | 'operator' | 'tractor' | 'scheduler' | 'sensor' | 'berth_manager' | 'yard_block' | 'unknown' {
  if (nodeId.startsWith('crane')) return 'crane';
  if (nodeId.startsWith('op-')) return 'operator';
  if (nodeId.startsWith('tractor-')) return 'tractor';
  if (nodeId.startsWith('scheduler')) return 'scheduler';
  if (nodeId.startsWith('load-cell-') || nodeId.startsWith('rfid-')) return 'sensor';
  if (nodeId.startsWith('berth-mgr')) return 'berth_manager';
  if (nodeId.startsWith('yard-blk')) return 'yard_block';
  if (nodeId.includes('agg')) return 'aggregator';
  return 'unknown';
}
