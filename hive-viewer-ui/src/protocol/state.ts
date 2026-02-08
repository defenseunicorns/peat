/** Zustand store for HIVE viewer state. */

import { create } from 'zustand';
import type {
  ViewerEvent,
  NodeState,
  HiveEvent,
  OodaCycleData,
  SimClock,
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

  /** Set playback speed. */
  setPlaybackSpeed: (speed: number) => void;

  /** Connect to the relay server. */
  connect: (url?: string) => void;

  /** Disconnect from the relay server. */
  disconnect: () => void;

  /** Apply an incoming ViewerEvent. */
  applyEvent: (event: ViewerEvent) => void;
}

const PLAYBACK_TICK_MS = 50;

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

  setPlaybackSpeed: (speed: number) => set({ playbackSpeed: speed }),

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
        console.error('[HIVE Viewer] Error:', error);
        set({ lastError: error });
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
              };
            }
          }
        }
        console.log('[HIVE Viewer] Snapshot applied:', Object.keys(nodes));
        set({
          nodes,
          events: event.events ?? [],
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
              },
            },
            totalCycles: state.totalCycles + 1,
          };
        });
        break;
      }

      case 'hive_event': {
        set((state) => ({
          events: [...state.events, {
            event_type: event.event_type,
            source: event.source,
            priority: event.priority,
            details: event.details,
            timestamp: event.timestamp,
          }],
        }));
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
