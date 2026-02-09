/** Pure function deriving spatial state from OODA data + HIVE events. */

import type { NodeState, HiveEvent } from '../protocol/types';
import type {
  SpatialDerivedState,
  CraneVisualState,
  OperatorVisualState,
  TractorVisualState,
  SensorVisualState,
  DerivedContainer,
  HoldSummaryState,
  EquipmentStatus,
} from './types';
import { CONTAINER_GRID } from './constants';

export function deriveSpatialState(
  nodes: Record<string, NodeState>,
  events: HiveEvent[],
): SpatialDerivedState {
  // --- Crane states ---
  const cranes: Record<string, CraneVisualState> = {};
  let totalCompleted = 0;

  for (const [nodeId, node] of Object.entries(nodes)) {
    if (node.role !== 'crane') continue;

    const isActive = node.action === 'complete_container_move';
    const isContending = node.contention_retry;
    const moveCount = node.history.filter(
      (c) => c.action === 'complete_container_move' && c.success,
    ).length;
    totalCompleted += moveCount;

    // Equipment status from HIVE events
    const equipmentStatus = getLatestEquipmentStatus(events, nodeId);

    // Target container: next uncompleted slot for this crane
    const targetContainerIndex = isActive
      ? Math.min(totalCompleted, CONTAINER_GRID.total - 1)
      : null;

    cranes[nodeId] = {
      isActive,
      isContending,
      equipmentStatus,
      moveCount,
      targetContainerIndex,
    };
  }

  // --- Container queue ---
  const containers: DerivedContainer[] = [];
  const craneIds = Object.keys(cranes);

  for (let i = 0; i < CONTAINER_GRID.total; i++) {
    const isHazmat = i < CONTAINER_GRID.hazmatCount;
    let status: DerivedContainer['status'];
    let completedBy: string | null = null;

    if (i < totalCompleted) {
      status = 'completed';
      // Attribute to cranes round-robin
      completedBy = craneIds.length > 0 ? craneIds[i % craneIds.length] : null;
    } else if (i === totalCompleted && Object.values(cranes).some((c) => c.isActive)) {
      status = 'in_progress';
    } else {
      status = 'pending';
    }

    containers.push({ index: i, isHazmat, status, completedBy });
  }

  // --- Hold summary from HIVE events ---
  const holdSummary = getHoldSummary(events, totalCompleted);

  // --- Operator states ---
  const operators: Record<string, OperatorVisualState> = {};
  for (const [nodeId, node] of Object.entries(nodes)) {
    if (node.role !== 'operator') continue;

    // Check if assigned by looking at accept_assignment vs complete_assignment in recent history
    const assignIdx = [...node.history].reverse().findIndex((h) => h.action === 'accept_assignment');
    const releaseIdx = [...node.history].reverse().findIndex((h) => h.action === 'complete_assignment');
    const isAssigned = assignIdx !== -1 && (releaseIdx === -1 || assignIdx < releaseIdx);

    operators[nodeId] = {
      isAvailable: node.action !== 'accept_assignment' && !isAssigned,
      assignedTo: isAssigned ? nodeId.replace('op-', 'crane-') : null,
      isOnBreak: node.action === 'wait' && !isAssigned,
      hazmatCertified: nodeId === 'op-1', // matches sim: op-1 is hazmat certified
    };
  }

  // --- Tractor states ---
  const tractors: Record<string, TractorVisualState> = {};
  for (const [nodeId, node] of Object.entries(nodes)) {
    if (node.role !== 'tractor') continue;
    const isMoving = node.action === 'transport_container';
    const isCharging = node.action === 'request_charge';
    const tripsCompleted = node.history.filter(
      (c) => c.action === 'transport_container' && c.success,
    ).length;
    // Battery from lifecycle resources
    const batteryRes = node.lifecycle.resources['battery_pct'];
    const batteryPct = batteryRes ? batteryRes.value : 100;
    tractors[nodeId] = { isMoving, batteryPct, isCharging, tripsCompleted };
  }

  // --- Sensor states ---
  const sensors: Record<string, SensorVisualState> = {};
  for (const [nodeId, node] of Object.entries(nodes)) {
    if (node.role !== 'sensor') continue;
    const isEmitting = node.action === 'emit_reading';
    const sensorType = nodeId.startsWith('load-cell') ? 'LOAD_CELL' : 'RFID';
    // Calibration from lifecycle subsystems
    const calSub = node.lifecycle.subsystems['calibration'];
    const calibrationPct = calSub ? calSub.confidence * 100 : 100;
    sensors[nodeId] = { isEmitting, sensorType, calibrationPct };
  }

  // --- Aggregator active ---
  const aggregatorActive = Object.values(nodes).some(
    (n) => n.role === 'aggregator' && n.action !== 'wait',
  );

  // --- Scheduler active ---
  const schedulerActive = Object.values(nodes).some(
    (n) => n.role === 'scheduler' && n.action !== 'wait',
  );

  return { containers, cranes, operators, tractors, sensors, holdSummary, aggregatorActive, schedulerActive };
}

function getLatestEquipmentStatus(
  events: HiveEvent[],
  nodeId: string,
): EquipmentStatus {
  for (let i = events.length - 1; i >= 0; i--) {
    const e = events[i];
    if (e.event_type === 'equipment_status_change' && e.source === nodeId) {
      const details = e.details as Record<string, unknown> | null;
      const status = details?.status as string | undefined;
      if (status === 'degraded') return 'degraded';
      if (status === 'failed') return 'failed';
      return 'operational';
    }
  }
  return 'operational';
}

function getHoldSummary(
  events: HiveEvent[],
  totalCompleted: number,
): HoldSummaryState {
  // Find latest hold_summary_update event
  for (let i = events.length - 1; i >= 0; i--) {
    const e = events[i];
    if (e.event_type === 'hold_summary_update') {
      const d = e.details as Record<string, unknown> | null;
      return {
        movesPerHour: (d?.moves_per_hour as number) ?? 0,
        movesCompleted: (d?.moves_completed as number) ?? totalCompleted,
        movesRemaining: (d?.moves_remaining as number) ?? CONTAINER_GRID.total - totalCompleted,
        gapCount: (d?.gap_count as number) ?? 0,
      };
    }
  }
  return {
    movesPerHour: 0,
    movesCompleted: totalCompleted,
    movesRemaining: CONTAINER_GRID.total - totalCompleted,
    gapCount: 0,
  };
}
