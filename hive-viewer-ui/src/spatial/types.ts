/** Derived spatial types for port operations Three.js visualization. */

export type ContainerStatus = 'pending' | 'in_progress' | 'completed';

export interface DerivedContainer {
  index: number;
  isHazmat: boolean;
  status: ContainerStatus;
  completedBy: string | null;
}

export type EquipmentStatus = 'operational' | 'degraded' | 'failed';

export interface CraneVisualState {
  isActive: boolean;
  isContending: boolean;
  equipmentStatus: EquipmentStatus;
  moveCount: number;
  targetContainerIndex: number | null;
}

export interface OperatorVisualState {
  isAvailable: boolean;
  assignedTo: string | null;
  isOnBreak: boolean;
  hazmatCertified: boolean;
}

export interface HoldSummaryState {
  movesPerHour: number;
  movesCompleted: number;
  movesRemaining: number;
  gapCount: number;
}

export interface SpatialDerivedState {
  containers: DerivedContainer[];
  cranes: Record<string, CraneVisualState>;
  operators: Record<string, OperatorVisualState>;
  holdSummary: HoldSummaryState;
  aggregatorActive: boolean;
}
