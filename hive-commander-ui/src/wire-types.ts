/**
 * Wire types for port-ops simulation viewer.
 *
 * Defines the role union and entity types exchanged between the simulation
 * backend and the commander UI. These types mirror the Python simulation
 * models and are used for real-time status display.
 */

/** Port-ops worker roles. */
export type PortOpsRole = 'signaler' | 'crane_operator' | 'stevedore' | 'supervisor';

/** Hierarchy levels in the port-ops simulation. */
export type HierarchyLevel = 1 | 2 | 3;

/** Signal types a signaler can issue. */
export type SignalType = 'HOIST' | 'LOWER' | 'STOP' | 'CLEAR';

/** Entity state on the wire. */
export interface PortOpsEntity {
  entityId: string;
  role: PortOpsRole;
  hierarchyLevel: HierarchyLevel;
  state: 'idle' | 'active' | 'signaling' | 'error';
  visibilityRangeM: number;
  lastAction?: string;
  lastActionTime?: string;
}

/** Signaler-specific wire state. */
export interface SignalerState extends PortOpsEntity {
  role: 'signaler';
  currentSignal?: SignalType;
  assignedCraneId?: string;
  groundClear: boolean;
}

/**
 * Infer the PortOpsRole from an entity ID or name.
 *
 * Entity names follow the pattern `<role>-<index>`, e.g. `signaler-0`,
 * `crane_operator-1`.
 */
export function inferRole(entityIdOrName: string): PortOpsRole | null {
  const lower = entityIdOrName.toLowerCase();

  if (lower.startsWith('signaler')) return 'signaler';
  if (lower.startsWith('crane_operator') || lower.startsWith('crane-operator')) return 'crane_operator';
  if (lower.startsWith('stevedore')) return 'stevedore';
  if (lower.startsWith('supervisor')) return 'supervisor';

  return null;
}
