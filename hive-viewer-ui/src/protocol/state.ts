/** Zustand store for HIVE viewer state. */

import { create } from 'zustand';
import type {
  ViewerEvent,
  NodeState,
  HiveEvent,
  OodaCycleData,
  SimClock,
  LifecycleState,
} from './types';
import { inferRole } from './types';
import { ViewerConnection, defaultWsUrl, type ConnectionStatus } from './connection';

export interface ViewerStore {
  /** WebSocket connection status. */
  status: ConnectionStatus;

  /** WebSocket URL being used. */
  wsUrl: string;

  /** Last error message. */
  lastError: string | null;

  /** Per-node OODA cycle state (latest + history). */
  nodes: Record<string, NodeState>;

  /** HIVE events (capability changes, contention, etc.). */
  events: HiveEvent[];

  /** Current simulation clock. */
  simClock: SimClock | null;

  /** Total OODA cycles received. */
  totalCycles: number;

  /** Connection instance (not serialized). */
  connection: ViewerConnection | null;

  /** Playback speed: 0=paused, 0.5=slow, 1=realtime, 2=fast, 4=fastest. */
  playbackSpeed: number;

  /** Buffered events waiting to be applied. */
  playbackQueue: ViewerEvent[];

  /** Interval ID for the playback drain loop. */
  _playbackTimer: ReturnType<typeof setInterval> | null;

  /** Last non-zero speed (for play/pause toggle). */
  _lastSpeed: number;

  /** Set playback speed. */
  setPlaybackSpeed: (speed: number) => void;

  /** Toggle play/pause. */
  togglePlayPause: () => void;

  /** Restart: disconnect, clear state, reconnect. */
  restart: () => void;

  /** Connect to the relay server. */
  connect: (url?: string) => void;

  /** Disconnect from the relay server. */
  disconnect: () => void;

  /** Apply an incoming ViewerEvent. */
  applyEvent: (event: ViewerEvent) => void;
}

const PLAYBACK_TICK_MS = 150;

export const useViewerStore = create<ViewerStore>((set, get) => ({
  status: 'disconnected',
  wsUrl: '',
  lastError: null,
  nodes: {},
  events: [],
  simClock: null,
  totalCycles: 0,
  connection: null,
  playbackSpeed: 1,
  playbackQueue: [],
  _playbackTimer: null,
  _lastSpeed: 1,

  setPlaybackSpeed: (speed: number) => {
    if (speed > 0) set({ playbackSpeed: speed, _lastSpeed: speed });
    else set({ playbackSpeed: 0 });
  },

  togglePlayPause: () => {
    const { playbackSpeed, _lastSpeed } = get();
    if (playbackSpeed === 0) {
      set({ playbackSpeed: _lastSpeed || 1 });
    } else {
      set({ playbackSpeed: 0 });
    }
  },

  restart: () => {
    const wsUrl = get().wsUrl || undefined;
    get().disconnect();
    set({
      nodes: {},
      events: [],
      simClock: null,
      totalCycles: 0,
      playbackSpeed: 1,
      _lastSpeed: 1,
      status: 'disconnected',
      lastError: null,
    });
    // Defer reconnect so the old WebSocket fully closes first
    setTimeout(() => get().connect(wsUrl), 100);
  },

  connect: (url?: string) => {
    const existing = get().connection;
    if (existing) existing.disconnect();

    const wsUrl = url ?? defaultWsUrl();
    console.log('[HIVE Viewer] Connecting to:', wsUrl);

    const conn = new ViewerConnection({
      url: wsUrl,
      onMessage: (data) => {
        const event = data as ViewerEvent;
        console.log('[HIVE Viewer] Message received:', (data as Record<string, unknown>).type);
        // state_snapshot bypasses queue — apply immediately so initial state loads fast
        if (event.type === 'state_snapshot') {
          get().applyEvent(event);
        } else {
          set((s) => ({ playbackQueue: [...s.playbackQueue, event] }));
        }
      },
      onStatusChange: (status) => {
        console.log('[HIVE Viewer] Status:', status);
        set({ status });
      },
      onError: (error) => {
        if (error) console.error('[HIVE Viewer] Error:', error);
        set({ lastError: error || null });
      },
    });

    // Start playback drain loop
    const timer = setInterval(() => {
      const { playbackSpeed, playbackQueue, applyEvent } = get();
      if (playbackSpeed === 0 || playbackQueue.length === 0) return;
      const n = Math.ceil(playbackSpeed);
      const toApply = playbackQueue.slice(0, n);
      set({ playbackQueue: playbackQueue.slice(n) });
      for (const ev of toApply) applyEvent(ev);
    }, PLAYBACK_TICK_MS);

    set({ connection: conn, wsUrl, lastError: null, _playbackTimer: timer });
    conn.connect();
  },

  disconnect: () => {
    const { connection, _playbackTimer } = get();
    if (connection) connection.disconnect();
    if (_playbackTimer) clearInterval(_playbackTimer);
    set({ connection: null, _playbackTimer: null, playbackQueue: [] });
  },

  applyEvent: (event: ViewerEvent) => {
    switch (event.type) {
      case 'state_snapshot': {
        const nodes: Record<string, NodeState> = {};
        // Parse OODA cycle documents from snapshot
        for (const [key, value] of Object.entries(event.documents)) {
          if (key.startsWith('ooda_cycles/') && typeof value === 'object' && value !== null) {
            const v = value as Record<string, unknown>;
            const nodeId = v.node_id as string;
            if (nodeId) {
              const cycleData = parseCycleData(v);
              nodes[nodeId] = {
                node_id: nodeId,
                ...cycleData,
                role: inferRole(nodeId),
                history: [cycleData],
                lifecycle: emptyLifecycle(),
              };
            }
          }
        }
        // Replay buffered events to build lifecycle state from snapshot
        const snapshotEvents = event.events ?? [];
        for (const e of snapshotEvents) {
          const source = e.source;
          if (source && nodes[source]) {
            const details = (e.details ?? {}) as Record<string, unknown>;
            nodes[source].lifecycle = applyLifecycleEvent(
              nodes[source].lifecycle, e.event_type, details,
            );
          }
        }
        console.log('[HIVE Viewer] Snapshot applied:', Object.keys(nodes));
        set({
          nodes,
          events: snapshotEvents,
          simClock: event.sim_clock,
          totalCycles: Object.values(nodes).reduce((sum, n) => sum + n.cycle, 0),
        });
        break;
      }

      case 'ooda_cycle': {
        const nodeId = event.node_id;
        const cycleData = parseCycleData(event);
        set((state) => {
          const existing = state.nodes[nodeId];
          const history = existing ? [...existing.history, cycleData] : [cycleData];
          return {
            nodes: {
              ...state.nodes,
              [nodeId]: {
                node_id: nodeId,
                ...cycleData,
                role: inferRole(nodeId),
                history,
                lifecycle: existing?.lifecycle ?? emptyLifecycle(),
              },
            },
            totalCycles: state.totalCycles + 1,
          };
        });
        break;
      }

      case 'hive_event': {
        set((state) => {
          const details = (event.details ?? {}) as Record<string, unknown>;
          const source = event.source;
          const existingNode = state.nodes[source];
          // Update lifecycle state for the source node
          const updatedNodes = existingNode
            ? {
                ...state.nodes,
                [source]: {
                  ...existingNode,
                  lifecycle: applyLifecycleEvent(existingNode.lifecycle, event.event_type, details),
                },
              }
            : state.nodes;
          return {
            nodes: updatedNodes,
            events: [...state.events, {
              event_type: event.event_type,
              source: event.source,
              priority: event.priority,
              details: event.details,
              timestamp: event.timestamp,
            }],
          };
        });
        break;
      }

      case 'document_update': {
        // Store raw document updates (future use)
        break;
      }

      case 'sim_clock': {
        set({ simClock: { sim_time: event.sim_time, real_elapsed_ms: event.real_elapsed_ms } });
        break;
      }
    }
  },
}));

function parseCycleData(v: Record<string, unknown>): OodaCycleData & { total_ms?: number } {
  return {
    cycle: (v.cycle as number) ?? 0,
    sim_time: (v.sim_time as string) ?? '',
    action: (v.action as string) ?? '',
    success: (v.success as boolean) ?? false,
    contention_retry: (v.contention_retry as boolean) ?? false,
    observe_ms: (v.observe_ms as number) ?? 0,
    decide_ms: (v.decide_ms as number) ?? 0,
    act_ms: (v.act_ms as number) ?? 0,
    total_ms: v.total_ms as number | undefined,
  };
}

function emptyLifecycle(): LifecycleState {
  return {
    subsystems: {},
    resources: {},
    equipmentState: 'OPERATIONAL',
    maintenanceJobs: [],
    gapReport: null,
  };
}

/** Apply a lifecycle event to a node's lifecycle state (immutable update). */
function applyLifecycleEvent(
  prev: LifecycleState,
  eventType: string,
  details: Record<string, unknown>,
): LifecycleState {
  const next = {
    ...prev,
    subsystems: { ...prev.subsystems },
    resources: { ...prev.resources },
    maintenanceJobs: [...prev.maintenanceJobs],
  };

  switch (eventType) {
    case 'CAPABILITY_DEGRADED': {
      const sub = details.subsystem as string;
      const after = details.after as number;
      const status = details.status as string ?? 'NOMINAL';
      if (sub) {
        next.subsystems[sub] = { confidence: after, status: status as LifecycleState['subsystems'][string]['status'] };
      }
      break;
    }
    case 'RESOURCE_CONSUMED': {
      const resource = details.resource as string;
      const after = details.after as number;
      if (resource) {
        next.resources[resource] = { value: after };
      }
      break;
    }
    case 'RESUPPLY_REQUESTED':
      next.equipmentState = 'RESUPPLYING';
      break;
    case 'RESUPPLY_COMPLETED':
      next.equipmentState = 'OPERATIONAL';
      // Reset all resources to 100
      for (const key of Object.keys(next.resources)) {
        next.resources[key] = { value: 100 };
      }
      break;
    case 'MAINTENANCE_SCHEDULED': {
      const sub = details.subsystem as string;
      if (sub && !next.maintenanceJobs.includes(sub)) {
        next.maintenanceJobs.push(sub);
      }
      break;
    }
    case 'MAINTENANCE_COMPLETE': {
      const sub = details.subsystem as string;
      next.maintenanceJobs = next.maintenanceJobs.filter((j) => j !== sub);
      // Restore subsystem confidence
      const restored = details.restored_confidence as number;
      if (sub && restored !== undefined) {
        const status = restored >= 0.7 ? 'NOMINAL' : restored >= 0.4 ? 'DEGRADED' : 'CRITICAL';
        next.subsystems[sub] = { confidence: restored, status: status as LifecycleState['subsystems'][string]['status'] };
      }
      break;
    }
    case 'GAP_ANALYSIS_REPORT': {
      const gaps = (details.gaps as Array<Record<string, unknown>>) ?? [];
      next.gapReport = {
        readinessScore: (details.readiness_score as number) ?? 1.0,
        gaps: gaps.map((g) => ({
          name: (g.capability_name as string) ?? '',
          confidence: (g.current_confidence as number) ?? 0,
          required: (g.required_confidence as number) ?? 0.7,
          status: (g.status as string) ?? '',
        })),
      };
      break;
    }
  }
  return next;
}
