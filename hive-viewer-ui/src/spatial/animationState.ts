/** Zustand store for fire-and-forget spatial animations.
 *
 * Animations are triggered by spatial_update events and run independently
 * via useFrame tick, persisting beyond single-frame OODA cycles.
 */

import { create } from 'zustand';

// ── Crane animation phases ──────────────────────────────────────────────────

export type CranePhase =
  | 'idle'
  | 'boom_to_ship'
  | 'trolley_extend'
  | 'spreader_lower'
  | 'grip'
  | 'spreader_hoist'
  | 'boom_to_shore'
  | 'spreader_lower_truck'
  | 'release'
  | 'spreader_return';

const CRANE_PHASE_DURATIONS: Record<CranePhase, number> = {
  idle: Infinity,
  boom_to_ship: 0.6,
  trolley_extend: 0.6,
  spreader_lower: 0.5,
  grip: 0.3,
  spreader_hoist: 0.5,
  boom_to_shore: 0.8,
  spreader_lower_truck: 0.5,
  release: 0.3,
  spreader_return: 0.4,
};

const CRANE_PHASE_ORDER: CranePhase[] = [
  'boom_to_ship',
  'trolley_extend',
  'spreader_lower',
  'grip',
  'spreader_hoist',
  'boom_to_shore',
  'spreader_lower_truck',
  'release',
  'spreader_return',
  'idle',
];

export interface CraneAnimation {
  phase: CranePhase;
  phaseProgress: number; // 0-1
  containerId: string;
  containerIndex: number;
  destinationBlock: string;
}

// ── Tractor animation phases ────────────────────────────────────────────────

export type TractorPhase = 'idle' | 'drive_to_yard' | 'unload' | 'drive_to_berth';

const TRACTOR_PHASE_DURATIONS: Record<TractorPhase, number> = {
  idle: Infinity,
  drive_to_yard: 2.0,
  unload: 0.5,
  drive_to_berth: 2.0,
};

const TRACTOR_PHASE_ORDER: TractorPhase[] = [
  'drive_to_yard',
  'unload',
  'drive_to_berth',
  'idle',
];

export interface TractorAnimation {
  phase: TractorPhase;
  phaseProgress: number; // 0-1
  containerId: string;
  destinationBlock: string;
}

// ── Store ───────────────────────────────────────────────────────────────────

export interface AnimationStore {
  cranes: Record<string, CraneAnimation>;
  tractors: Record<string, TractorAnimation>;
  triggerCraneDischarge: (
    craneId: string,
    containerId: string,
    containerIndex: number,
    destBlock: string,
  ) => void;
  triggerTractorTransport: (
    tractorId: string,
    containerId: string,
    destBlock: string,
  ) => void;
  tick: (delta: number) => void;
}

function advanceCranePhase(anim: CraneAnimation, delta: number): CraneAnimation {
  if (anim.phase === 'idle') return anim;

  const duration = CRANE_PHASE_DURATIONS[anim.phase];
  const newProgress = anim.phaseProgress + delta / duration;

  if (newProgress >= 1) {
    const idx = CRANE_PHASE_ORDER.indexOf(anim.phase);
    const nextPhase = CRANE_PHASE_ORDER[idx + 1] ?? 'idle';
    return { ...anim, phase: nextPhase, phaseProgress: 0 };
  }

  return { ...anim, phaseProgress: newProgress };
}

function advanceTractorPhase(anim: TractorAnimation, delta: number): TractorAnimation {
  if (anim.phase === 'idle') return anim;

  const duration = TRACTOR_PHASE_DURATIONS[anim.phase];
  const newProgress = anim.phaseProgress + delta / duration;

  if (newProgress >= 1) {
    const idx = TRACTOR_PHASE_ORDER.indexOf(anim.phase);
    const nextPhase = TRACTOR_PHASE_ORDER[idx + 1] ?? 'idle';
    return { ...anim, phase: nextPhase, phaseProgress: 0 };
  }

  return { ...anim, phaseProgress: newProgress };
}

export const useAnimationStore = create<AnimationStore>((set, get) => ({
  cranes: {},
  tractors: {},

  triggerCraneDischarge: (craneId, containerId, containerIndex, destBlock) => {
    set((state) => ({
      cranes: {
        ...state.cranes,
        [craneId]: {
          phase: 'boom_to_ship' as CranePhase,
          phaseProgress: 0,
          containerId,
          containerIndex,
          destinationBlock: destBlock,
        },
      },
    }));
  },

  triggerTractorTransport: (tractorId, containerId, destBlock) => {
    set((state) => ({
      tractors: {
        ...state.tractors,
        [tractorId]: {
          phase: 'drive_to_yard' as TractorPhase,
          phaseProgress: 0,
          containerId,
          destinationBlock: destBlock,
        },
      },
    }));
  },

  tick: (delta: number) => {
    const { cranes, tractors } = get();
    let craneDirty = false;
    let tractorDirty = false;
    const nextCranes: Record<string, CraneAnimation> = {};
    const nextTractors: Record<string, TractorAnimation> = {};

    for (const [id, anim] of Object.entries(cranes)) {
      if (anim.phase === 'idle') {
        nextCranes[id] = anim;
      } else {
        nextCranes[id] = advanceCranePhase(anim, delta);
        craneDirty = true;
      }
    }

    for (const [id, anim] of Object.entries(tractors)) {
      if (anim.phase === 'idle') {
        nextTractors[id] = anim;
      } else {
        nextTractors[id] = advanceTractorPhase(anim, delta);
        tractorDirty = true;
      }
    }

    if (craneDirty || tractorDirty) {
      set({
        ...(craneDirty ? { cranes: nextCranes } : {}),
        ...(tractorDirty ? { tractors: nextTractors } : {}),
      });
    }
  },
}));
