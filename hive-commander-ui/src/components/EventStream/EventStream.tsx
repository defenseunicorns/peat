import { useState, useMemo, useCallback } from 'react';
import { HiveEvent, EventPriority, HiveEventType } from '../../types';

// Icon and color mapping for event types
const EVENT_STYLE: Record<HiveEventType, { icon: string; color: string }> = {
  // Operational events (blue/green tones)
  detection:         { icon: '\u25c6', color: '#44ff44' },
  track_new:         { icon: '\u2295', color: '#00ffff' },
  track_update:      { icon: '\u2192', color: '#44aaff' },
  track_lost:        { icon: '\u2717', color: '#ffaa00' },
  classification:    { icon: '\u25ca', color: '#44ffaa' },
  container_move:    { icon: '\u25a1', color: '#44aaff' },
  ooda_observe:      { icon: '\u25ce', color: '#00ccff' },
  ooda_orient:       { icon: '\u25c8', color: '#00aaff' },
  ooda_decide:       { icon: '\u25c9', color: '#0088ff' },
  ooda_act:          { icon: '\u25cf', color: '#0066ff' },
  engagement_active: { icon: '\u2694', color: '#ff4444' },
  effector_fired:    { icon: '\u26a1', color: '#ff44ff' },
  // Logistical events (amber/orange tones)
  maintenance_scheduled:    { icon: '\u2691', color: '#ffaa44' },
  maintenance_started:      { icon: '\u2692', color: '#ff8800' },
  maintenance_complete:     { icon: '\u2713', color: '#44ff44' },
  resupply_requested:       { icon: '\u25b2', color: '#ffaa44' },
  resupply_delivered:       { icon: '\u25bc', color: '#44ff44' },
  recertification_required: { icon: '\u2611', color: '#ffaa00' },
  recertification_complete: { icon: '\u2714', color: '#44ff44' },
  shift_started:            { icon: '\u21bb', color: '#aaff44' },
  shift_ended:              { icon: '\u21ba', color: '#88cc44' },
  capability_degraded:      { icon: '\u26a0', color: '#ff8800' },
  capability_restored:      { icon: '\u2714', color: '#44ff44' },
};

const PRIORITY_STYLE: Record<EventPriority, { label: string; color: string }> = {
  critical: { label: 'CRIT', color: '#ff4444' },
  high:     { label: 'HIGH', color: '#ffaa00' },
  normal:   { label: 'NORM', color: '#888888' },
  low:      { label: 'LOW',  color: '#555555' },
};

function formatTime(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

function formatEventLabel(eventType: HiveEventType): string {
  return eventType.replace(/_/g, ' ').replace(/\b\w/g, c => c.toUpperCase());
}

// Build cause-effect chain for an event
function buildChain(event: HiveEvent, allEvents: HiveEvent[]): HiveEvent[] {
  const chain: HiveEvent[] = [];
  const visited = new Set<string>();

  // Walk backward to root cause
  let current: HiveEvent | undefined = event;
  const ancestors: HiveEvent[] = [];
  while (current?.causeEventId && !visited.has(current.causeEventId)) {
    visited.add(current.causeEventId);
    current = allEvents.find(e => e.id === current!.causeEventId);
    if (current) ancestors.unshift(current);
  }

  chain.push(...ancestors, event);

  // Walk forward to effects
  const queue = event.effectEventIds ? [...event.effectEventIds] : [];
  while (queue.length > 0) {
    const effectId = queue.shift()!;
    if (visited.has(effectId)) continue;
    visited.add(effectId);
    const effect = allEvents.find(e => e.id === effectId);
    if (effect) {
      chain.push(effect);
      if (effect.effectEventIds) queue.push(...effect.effectEventIds);
    }
  }

  return chain;
}

interface EventStreamProps {
  events: HiveEvent[];
  playbackSpeed: number;
}

export default function EventStream({ events, playbackSpeed }: EventStreamProps) {
  const [showOperational, setShowOperational] = useState(true);
  const [showLogistical, setShowLogistical] = useState(true);
  const [expandedChain, setExpandedChain] = useState<string | null>(null);

  const isPaused = playbackSpeed === 0;

  const filteredEvents = useMemo(() => {
    return events.filter(e => {
      if (e.category === 'operational' && !showOperational) return false;
      if (e.category === 'logistical' && !showLogistical) return false;
      return true;
    });
  }, [events, showOperational, showLogistical]);

  // Count queued events by category when paused
  const queuedCounts = useMemo(() => {
    if (!isPaused) return null;
    let operational = 0;
    let logistical = 0;
    for (const e of events) {
      if (e.category === 'operational') operational++;
      else logistical++;
    }
    return { operational, logistical, total: events.length };
  }, [events, isPaused]);

  const toggleChain = useCallback((eventId: string) => {
    setExpandedChain(prev => prev === eventId ? null : eventId);
  }, []);

  return (
    <div style={{
      display: 'flex',
      flexDirection: 'column',
      height: '100%',
      color: '#ccc',
      fontSize: '12px',
    }}>
      {/* Header */}
      <div style={{
        padding: '8px 12px',
        borderBottom: '1px solid #333',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
      }}>
        <span style={{ color: '#ffaa44', fontWeight: 'bold', fontSize: '14px' }}>EVENT STREAM</span>
        {isPaused && (
          <span style={{ color: '#ff8800', fontSize: '11px' }}>PAUSED</span>
        )}
      </div>

      {/* Filter toggles */}
      <div style={{
        padding: '6px 12px',
        borderBottom: '1px solid #222',
        display: 'flex',
        gap: '8px',
      }}>
        <FilterToggle
          label="Operational"
          active={showOperational}
          color="#00ffff"
          count={isPaused ? queuedCounts?.operational : undefined}
          onClick={() => setShowOperational(!showOperational)}
        />
        <FilterToggle
          label="Logistical"
          active={showLogistical}
          color="#ffaa44"
          count={isPaused ? queuedCounts?.logistical : undefined}
          onClick={() => setShowLogistical(!showLogistical)}
        />
      </div>

      {/* Paused queue summary */}
      {isPaused && queuedCounts && (
        <div style={{
          padding: '6px 12px',
          background: '#1a1400',
          borderBottom: '1px solid #333',
          fontSize: '11px',
          color: '#ff8800',
        }}>
          Queue: {queuedCounts.total} events ({queuedCounts.operational} operational, {queuedCounts.logistical} logistical)
        </div>
      )}

      {/* Event list */}
      <div style={{ flex: 1, overflow: 'auto', padding: '4px 0' }}>
        {filteredEvents.length === 0 ? (
          <div style={{ padding: '12px', color: '#555', textAlign: 'center' }}>
            No events to display
          </div>
        ) : (
          filteredEvents.map(event => (
            <EventRow
              key={event.id}
              event={event}
              allEvents={events}
              isChainExpanded={expandedChain === event.id}
              onToggleChain={toggleChain}
            />
          ))
        )}
      </div>
    </div>
  );
}

interface FilterToggleProps {
  label: string;
  active: boolean;
  color: string;
  count?: number;
  onClick: () => void;
}

function FilterToggle({ label, active, color, count, onClick }: FilterToggleProps) {
  return (
    <button
      onClick={onClick}
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: '4px',
        padding: '2px 8px',
        background: active ? `${color}22` : '#111',
        border: `1px solid ${active ? color : '#333'}`,
        borderRadius: '4px',
        color: active ? color : '#555',
        cursor: 'pointer',
        fontSize: '11px',
      }}
    >
      <span style={{
        width: '6px',
        height: '6px',
        borderRadius: '50%',
        background: active ? color : '#333',
      }} />
      {label}
      {count !== undefined && (
        <span style={{ color: '#888', marginLeft: '2px' }}>({count})</span>
      )}
    </button>
  );
}

interface EventRowProps {
  event: HiveEvent;
  allEvents: HiveEvent[];
  isChainExpanded: boolean;
  onToggleChain: (id: string) => void;
}

function EventRow({ event, allEvents, isChainExpanded, onToggleChain }: EventRowProps) {
  const style = EVENT_STYLE[event.eventType] || { icon: '\u25cf', color: '#888' };
  const priorityStyle = PRIORITY_STYLE[event.priority];
  const hasChain = !!(event.causeEventId || (event.effectEventIds && event.effectEventIds.length > 0));
  const chain = isChainExpanded ? buildChain(event, allEvents) : [];
  const categoryColor = event.category === 'logistical' ? '#ffaa44' : '#00ffff';

  return (
    <div style={{ borderBottom: '1px solid #1a1a1a' }}>
      <div
        onClick={hasChain ? () => onToggleChain(event.id) : undefined}
        style={{
          display: 'flex',
          alignItems: 'flex-start',
          padding: '4px 12px',
          cursor: hasChain ? 'pointer' : 'default',
          background: isChainExpanded ? '#1a1a2a' : 'transparent',
        }}
      >
        {/* Timestamp */}
        <span style={{ color: '#555', width: '62px', flexShrink: 0, fontFamily: 'monospace' }}>
          {formatTime(event.timestamp)}
        </span>

        {/* Category indicator */}
        <span style={{
          width: '3px',
          height: '14px',
          background: categoryColor,
          borderRadius: '1px',
          marginRight: '6px',
          marginTop: '1px',
          flexShrink: 0,
        }} />

        {/* Event icon */}
        <span style={{ color: style.color, width: '16px', textAlign: 'center', flexShrink: 0 }}>
          {style.icon}
        </span>

        {/* Message */}
        <span style={{ flex: 1, marginLeft: '6px', lineHeight: '16px' }}>
          <span style={{ color: style.color }}>{formatEventLabel(event.eventType)}</span>
          <span style={{ color: '#888', marginLeft: '6px' }}>{event.message}</span>
          {event.metric && (
            <span style={{ color: '#aa88ff', marginLeft: '6px' }}>
              [{event.metric.name}: {event.metric.value}{event.metric.unit}]
            </span>
          )}
        </span>

        {/* Priority badge */}
        {event.priority !== 'normal' && (
          <span style={{
            color: priorityStyle.color,
            fontSize: '9px',
            fontWeight: 'bold',
            padding: '0 4px',
            flexShrink: 0,
          }}>
            {priorityStyle.label}
          </span>
        )}

        {/* Chain indicator */}
        {hasChain && (
          <span style={{ color: '#555', fontSize: '10px', flexShrink: 0, marginLeft: '4px' }}>
            {isChainExpanded ? '\u25b4' : '\u25be'}
          </span>
        )}
      </div>

      {/* Expanded cause-effect chain */}
      {isChainExpanded && chain.length > 1 && (
        <div style={{
          padding: '4px 12px 8px 80px',
          background: '#111118',
          borderLeft: `2px solid ${categoryColor}`,
          marginLeft: '12px',
        }}>
          <div style={{ color: '#888', fontSize: '10px', marginBottom: '4px' }}>Cause-Effect Chain:</div>
          {chain.map((chainEvent, idx) => {
            const ceStyle = EVENT_STYLE[chainEvent.eventType] || { icon: '\u25cf', color: '#888' };
            const isCurrent = chainEvent.id === event.id;
            return (
              <div
                key={chainEvent.id}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  padding: '2px 0',
                  opacity: isCurrent ? 1 : 0.7,
                  fontWeight: isCurrent ? 'bold' : 'normal',
                }}
              >
                <span style={{ color: ceStyle.color, width: '14px', textAlign: 'center' }}>{ceStyle.icon}</span>
                <span style={{ color: ceStyle.color, marginLeft: '4px' }}>
                  {formatEventLabel(chainEvent.eventType)}
                </span>
                {chainEvent.metric && (
                  <span style={{ color: '#aa88ff', marginLeft: '4px' }}>
                    {chainEvent.metric.name}: {chainEvent.metric.value}{chainEvent.metric.unit}
                  </span>
                )}
                {chainEvent.message && (
                  <span style={{ color: '#666', marginLeft: '4px' }}>{chainEvent.message}</span>
                )}
                {idx < chain.length - 1 && (
                  <span style={{ color: '#444', margin: '0 6px' }}>{'\u2192'}</span>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
