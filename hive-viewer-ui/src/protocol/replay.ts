/** JSONL replay file parser — TS port of Rust IngestLine.classify(). */

import type {
  ViewerEvent,
  OodaCycleEvent,
  HiveEventEvent,
  DocumentUpdateEvent,
} from './types';

export interface ReplayMeta {
  totalFrames: number;
  nodeIds: string[];
  simTimeRange: [string, string];
}

/**
 * Classify a parsed JSON object into a ViewerEvent.
 * Mirrors hive-viewer/src/ws/protocol.rs IngestLine::classify().
 */
export function classifyLine(json: Record<string, unknown>): ViewerEvent | null {
  // METRICS type → OodaCycleEvent
  if (json.type === 'METRICS') {
    const nodeId = json.node_id;
    if (typeof nodeId !== 'string') return null;
    return {
      type: 'ooda_cycle',
      node_id: nodeId,
      cycle: (json.cycle as number) ?? 0,
      sim_time: (json.sim_time as string) ?? '',
      action: (json.action as string) ?? 'unknown',
      success: (json.success as boolean) ?? false,
      contention_retry: (json.contention_retry as boolean) ?? false,
      observe_ms: (json.observe_ms as number) ?? 0,
      decide_ms: (json.decide_ms as number) ?? 0,
      act_ms: (json.act_ms as number) ?? 0,
    } as OodaCycleEvent;
  }

  // event_type field → HiveEventEvent
  if ('event_type' in json) {
    return {
      type: 'hive_event',
      event_type: (json.event_type as string) ?? '',
      source: (json.source as string) ?? '',
      priority: (json.priority as string) ?? 'ROUTINE',
      details: json.details ?? null,
      timestamp: (json.timestamp as string) ?? null,
    } as HiveEventEvent;
  }

  // collection + doc_id → DocumentUpdateEvent
  if ('collection' in json && 'doc_id' in json) {
    return {
      type: 'document_update',
      collection: json.collection as string,
      doc_id: json.doc_id as string,
      fields: json.fields ?? null,
    } as DocumentUpdateEvent;
  }

  return null;
}

/** Parse a JSONL text blob into ViewerEvent[]. */
export function parseReplayFile(text: string): ViewerEvent[] {
  const events: ViewerEvent[] = [];
  for (const line of text.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    try {
      const json = JSON.parse(trimmed) as Record<string, unknown>;
      const event = classifyLine(json);
      if (event) events.push(event);
    } catch {
      // skip malformed lines
    }
  }
  return events;
}

/** Extract metadata from a parsed replay event array. */
export function extractMeta(events: ViewerEvent[]): ReplayMeta {
  const nodeIds = new Set<string>();
  let firstTime = '';
  let lastTime = '';

  for (const ev of events) {
    if (ev.type === 'ooda_cycle') {
      nodeIds.add(ev.node_id);
      if (ev.sim_time) {
        if (!firstTime) firstTime = ev.sim_time;
        lastTime = ev.sim_time;
      }
    } else if (ev.type === 'hive_event') {
      if (ev.source) nodeIds.add(ev.source);
      if (ev.timestamp) {
        if (!firstTime) firstTime = ev.timestamp;
        lastTime = ev.timestamp;
      }
    } else if (ev.type === 'sim_clock') {
      if (ev.sim_time) {
        if (!firstTime) firstTime = ev.sim_time;
        lastTime = ev.sim_time;
      }
    }
  }

  return {
    totalFrames: events.length,
    nodeIds: [...nodeIds].sort(),
    simTimeRange: [firstTime, lastTime],
  };
}
