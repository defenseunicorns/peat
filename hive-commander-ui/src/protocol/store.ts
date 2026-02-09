/** Zustand store for HIVE Commander — fed by WebSocket lifecycle events. */

import { create } from 'zustand';
import type {
  ComposedCapability,
  HiveEvent,
  GapAnalysisReport,
  HiveEventType,
  EventCategory,
  EventPriority,
} from '../types';
import { getHealthStatus } from '../types';
import { ViewerConnection, defaultWsUrl, type ConnectionStatus } from './connection';

// ---- Wire event types from relay (matches Rust HiveEvent classification) ----

interface RelayHiveEvent {
  type: 'hive_event';
  event_type: string;
  source: string;
  priority: string;
  details: Record<string, unknown>;
  timestamp: string | null;
}

interface RelayStateSnapshot {
  type: 'state_snapshot';
  documents: Record<string, unknown>;
  events: Array<Record<string, unknown>>;
  sim_clock: { sim_time: string; real_elapsed_ms: number } | null;
}

interface RelaySimClock {
  type: 'sim_clock';
  sim_time: string;
  real_elapsed_ms: number;
}

type RelayEvent = RelayHiveEvent | RelayStateSnapshot | RelaySimClock | { type: string; [k: string]: unknown };

// ---- Store shape ----

export interface CommanderStore {
  status: ConnectionStatus;
  capabilities: ComposedCapability[];
  events: HiveEvent[];
  gapReports: GapAnalysisReport[];
  simClock: { simTime: string } | null;
  connection: ViewerConnection | null;
  _playbackTimer: ReturnType<typeof setInterval> | null;

  connect: (url?: string) => void;
  disconnect: () => void;
}

let nextEventId = 1000;

function mapPriority(p: string): EventPriority {
  const lower = p.toLowerCase();
  if (lower === 'critical') return 'critical';
  if (lower === 'high') return 'high';
  if (lower === 'low' || lower === 'routine') return 'low';
  return 'normal';
}

function mapEventType(rawType: string): { eventType: HiveEventType; category: EventCategory; eventClass: 'product' | 'anomaly' } {
  const t = rawType.toLowerCase();
  // Degradation events → anomaly + logistical
  if (t === 'capability_degraded') return { eventType: 'capability_degraded', category: 'logistical', eventClass: 'anomaly' };
  if (t === 'capability_restored') return { eventType: 'capability_restored', category: 'logistical', eventClass: 'product' };
  // Maintenance
  if (t === 'maintenance_scheduled') return { eventType: 'maintenance_scheduled', category: 'logistical', eventClass: 'product' };
  if (t === 'maintenance_started') return { eventType: 'maintenance_started', category: 'logistical', eventClass: 'product' };
  if (t === 'maintenance_complete') return { eventType: 'maintenance_complete', category: 'logistical', eventClass: 'product' };
  // Resupply
  if (t === 'resupply_requested' || t === 'resource_consumed') return { eventType: 'resupply_requested', category: 'logistical', eventClass: 'anomaly' };
  if (t === 'resupply_completed' || t === 'resupply_delivered') return { eventType: 'resupply_delivered', category: 'logistical', eventClass: 'product' };
  // Certification
  if (t === 'certification_expiring' || t === 'certification_expired') return { eventType: 'recertification_required', category: 'logistical', eventClass: 'anomaly' };
  if (t === 'recertification_completed') return { eventType: 'recertification_complete', category: 'logistical', eventClass: 'product' };
  // Shift
  if (t === 'shift_relief_requested') return { eventType: 'shift_started', category: 'logistical', eventClass: 'product' };
  if (t === 'shift_relief_arrived') return { eventType: 'shift_ended', category: 'logistical', eventClass: 'product' };
  // Container moves
  if (t === 'container_move_complete') return { eventType: 'container_move', category: 'operational', eventClass: 'product' };
  // Default: operational product
  return { eventType: t as HiveEventType, category: 'operational', eventClass: 'product' };
}

function relayToHiveEvent(relay: RelayHiveEvent): HiveEvent {
  const mapped = mapEventType(relay.event_type);
  const details = relay.details ?? {};
  const subsystem = details.subsystem as string | undefined;
  const before = details.before as number | undefined;
  const after = details.after as number | undefined;

  let message = `${relay.event_type} on ${relay.source}`;
  if (subsystem) message = `${subsystem}: ${before ?? '?'} → ${after ?? '?'}`;
  if (details.reason) message = details.reason as string;
  if (details.resource) message = `${details.resource}: ${before ?? '?'} → ${after ?? '?'}`;

  return {
    id: `evt-${nextEventId++}`,
    timestamp: Date.now(),
    sourceNodeId: relay.source,
    eventClass: mapped.eventClass,
    eventType: mapped.eventType,
    category: mapped.category,
    priority: mapPriority(relay.priority),
    message,
    metric: typeof after === 'number'
      ? { name: (subsystem ?? details.resource ?? relay.event_type) as string, value: after, unit: subsystem ? 'conf' : '%' }
      : undefined,
  };
}

// ---- Capability update helpers ----

function updateCapabilityFromDegradation(
  caps: ComposedCapability[],
  source: string,
  details: Record<string, unknown>,
): ComposedCapability[] {
  const subsystem = details.subsystem as string | undefined;
  const after = details.after as number | undefined;
  if (!subsystem || after === undefined) return caps;

  return caps.map((cap) => {
    // Match capability by name containing the source node
    if (!cap.name.toLowerCase().includes(source.replace('crane-', 'c').replace('hold-agg-', 'h'))) {
      // If no name match, update the first capability (for demo/simple setups)
      return cap;
    }
    const newHealth = cap.equipmentHealth.map((eh) =>
      eh.label.toLowerCase().includes(subsystem)
        ? { ...eh, confidence: after, status: getHealthStatus(after) }
        : eh,
    );
    // If subsystem not in existing health, add it
    if (!newHealth.some((eh) => eh.label.toLowerCase().includes(subsystem))) {
      newHealth.push({ label: subsystem, confidence: after, status: getHealthStatus(after) });
    }
    const worstEquip = Math.min(...newHealth.map((eh) => eh.confidence));
    return {
      ...cap,
      confidence: Math.min(cap.confidence, worstEquip),
      equipmentHealth: newHealth,
    };
  });
}

function updateGapReports(
  reports: GapAnalysisReport[],
  details: Record<string, unknown>,
): GapAnalysisReport[] {
  const locationId = (details.location_id as string) ?? 'unknown';
  const readinessScore = details.readiness_score as number ?? 0;
  const gaps = (details.gaps as Array<Record<string, unknown>>) ?? [];

  const newReport: GapAnalysisReport = {
    level: (details.level as 'H2' | 'H3') ?? 'H2',
    locationId,
    locationLabel: locationId,
    readinessScore,
    worstHealth: readinessScore >= 0.7 ? 'nominal' : readinessScore >= 0.4 ? 'degraded' : 'critical',
    operationalCount: gaps.filter((g) => (g.status as string) === 'NOMINAL').length,
    totalCount: gaps.length || 1,
    gaps: gaps.map((g) => ({
      capabilityName: (g.capability_name as string) ?? '',
      capabilityType: (g.capability_type as 'sensor' | 'compute' | 'communication' | 'mobility' | 'payload' | 'emergent') ?? 'payload',
      requiredConfidence: (g.required_confidence as number) ?? 0.7,
      currentConfidence: (g.current_confidence as number) ?? 0,
      decayRate: (g.decay_rate as number) ?? 0,
      etaThresholdBreach: null,
      reason: (g.status as string) ?? '',
      pendingActions: ((g.pending_actions as Array<Record<string, unknown>>) ?? []).map((a) => ({
        id: (a.id as string) ?? '',
        description: (a.description as string) ?? '',
        etaMinutes: (a.eta_minutes as number) ?? null,
        status: (a.status as 'pending' | 'in_progress' | 'blocked') ?? 'pending',
        blockedBy: (a.blocked_by as string) ?? null,
      })),
      requiresOversight: false,
      maxAuthority: null,
      contributorCount: 1,
    })),
    logisticalDependencies: [],
    aggregatedAt: new Date().toISOString(),
  };

  // Replace existing report for same location, or append
  const idx = reports.findIndex((r) => r.locationId === locationId);
  if (idx >= 0) {
    const updated = [...reports];
    updated[idx] = newReport;
    return updated;
  }
  return [...reports, newReport];
}

// ---- Store ----

export const useCommanderStore = create<CommanderStore>((set, get) => ({
  status: 'disconnected',
  capabilities: [],
  events: [],
  gapReports: [],
  simClock: null,
  connection: null,
  _playbackTimer: null,

  connect: (url?: string) => {
    const existing = get().connection;
    if (existing) existing.disconnect();

    const wsUrl = url ?? defaultWsUrl();
    console.log('[Commander] Connecting to:', wsUrl);

    const conn = new ViewerConnection({
      url: wsUrl,
      onMessage: (data) => {
        const event = data as RelayEvent;
        handleRelayEvent(event, set);
      },
      onStatusChange: (status) => {
        console.log('[Commander] Status:', status);
        set({ status });
      },
      onError: (error) => {
        console.error('[Commander] Error:', error);
      },
    });

    set({ connection: conn });
    conn.connect();
  },

  disconnect: () => {
    const { connection } = get();
    if (connection) connection.disconnect();
    set({ connection: null });
  },
}));

function handleRelayEvent(
  event: RelayEvent,
  set: (partial: Partial<CommanderStore> | ((s: CommanderStore) => Partial<CommanderStore>)) => void,
) {
  switch (event.type) {
    case 'state_snapshot': {
      const snap = event as RelayStateSnapshot;
      if (snap.sim_clock) {
        set({ simClock: { simTime: snap.sim_clock.sim_time } });
      }
      break;
    }

    case 'sim_clock': {
      const clk = event as RelaySimClock;
      set({ simClock: { simTime: clk.sim_time } });
      break;
    }

    case 'hive_event': {
      const relay = event as RelayHiveEvent;
      const hiveEvent = relayToHiveEvent(relay);

      set((s) => {
        let caps = s.capabilities;
        let gapReports = s.gapReports;

        // Update capabilities on degradation events
        if (relay.event_type === 'CAPABILITY_DEGRADED') {
          caps = updateCapabilityFromDegradation(caps, relay.source, relay.details);
        }

        // Update gap reports
        if (relay.event_type === 'GAP_ANALYSIS_REPORT') {
          gapReports = updateGapReports(gapReports, relay.details);
        }

        // Skip RESOURCE_CONSUMED from event stream (too noisy) but keep everything else
        const newEvents = relay.event_type === 'RESOURCE_CONSUMED'
          ? s.events
          : [...s.events, hiveEvent];

        return {
          capabilities: caps,
          events: newEvents,
          gapReports,
        };
      });
      break;
    }
  }
}
