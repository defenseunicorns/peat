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
  role: 'crane' | 'aggregator' | 'operator' | 'unknown';
  /** All cycles received for this node. */
  history: OodaCycleData[];
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

export function inferRole(nodeId: string): 'crane' | 'aggregator' | 'operator' | 'unknown' {
  if (nodeId.startsWith('crane')) return 'crane';
  if (nodeId.startsWith('op-')) return 'operator';
  if (nodeId.includes('agg')) return 'aggregator';
  return 'unknown';
}
