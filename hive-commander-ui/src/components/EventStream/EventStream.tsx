import { useState, useMemo } from 'react';
import { BerthEvent, HoldId } from '../../wire-types';

interface EventStreamProps {
  events: BerthEvent[];
  selectedHold?: HoldId;
}

const typeColors: Record<BerthEvent['type'], string> = {
  container_move: '#44ff44',
  crane_cycle: '#00ccff',
  tractor_dispatch: '#ff44aa',
  lashing_complete: '#ffcc00',
  yard_store: '#88aaff',
  status_change: '#888',
};

const typeLabels: Record<BerthEvent['type'], string> = {
  container_move: 'MOVE',
  crane_cycle: 'CRANE',
  tractor_dispatch: 'TRCT',
  lashing_complete: 'LASH',
  yard_store: 'YARD',
  status_change: 'STAT',
};

function formatTime(ts: number): string {
  const d = new Date(ts);
  return `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}:${d.getSeconds().toString().padStart(2, '0')}`;
}

export default function EventStream({ events, selectedHold }: EventStreamProps) {
  const [filterType, setFilterType] = useState<BerthEvent['type'] | 'all'>('all');

  const filteredEvents = useMemo(() => {
    let result = events;
    if (selectedHold !== undefined) {
      result = result.filter(e => e.holdId === selectedHold || e.holdId === undefined);
    }
    if (filterType !== 'all') {
      result = result.filter(e => e.type === filterType);
    }
    return result.slice().reverse(); // newest first
  }, [events, selectedHold, filterType]);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}>
      {/* Header */}
      <div style={{
        padding: '8px 12px',
        borderBottom: '1px solid #333',
        display: 'flex',
        justifyContent: 'space-between',
        alignItems: 'center',
      }}>
        <span style={{ color: '#00ffff', fontSize: '12px', fontWeight: 'bold' }}>EVENT STREAM</span>
        <span style={{ color: '#666', fontSize: '10px' }}>
          {filteredEvents.length}/{events.length}
          {selectedHold !== undefined && ` (Hold ${selectedHold})`}
        </span>
      </div>

      {/* Type filters */}
      <div style={{
        padding: '6px 12px',
        display: 'flex',
        gap: '4px',
        flexWrap: 'wrap',
        borderBottom: '1px solid #222',
      }}>
        <FilterChip
          label="ALL"
          color="#aaa"
          active={filterType === 'all'}
          onClick={() => setFilterType('all')}
        />
        {(Object.keys(typeLabels) as BerthEvent['type'][]).map(type => (
          <FilterChip
            key={type}
            label={typeLabels[type]}
            color={typeColors[type]}
            active={filterType === type}
            onClick={() => setFilterType(filterType === type ? 'all' : type)}
          />
        ))}
      </div>

      {/* Event list */}
      <div style={{ flex: 1, overflow: 'auto', padding: '4px 0' }}>
        {filteredEvents.map(event => (
          <div
            key={event.id}
            style={{
              padding: '4px 12px',
              borderBottom: '1px solid #1a1a2a',
              fontSize: '11px',
              fontFamily: 'monospace',
            }}
          >
            <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
              <span style={{ color: '#555', minWidth: '58px' }}>{formatTime(event.timestamp)}</span>
              <span style={{
                color: typeColors[event.type],
                background: `${typeColors[event.type]}15`,
                padding: '0 4px',
                borderRadius: '2px',
                fontSize: '9px',
                fontWeight: 'bold',
                minWidth: '36px',
                textAlign: 'center',
              }}>
                {typeLabels[event.type]}
              </span>
              {event.holdId && (
                <span style={{ color: '#cc44ff', fontSize: '9px' }}>H{event.holdId}</span>
              )}
              <span style={{ color: '#aaa', flex: 1 }}>{event.message}</span>
            </div>
          </div>
        ))}
        {filteredEvents.length === 0 && (
          <div style={{ padding: '20px', textAlign: 'center', color: '#555', fontSize: '12px' }}>
            No events{selectedHold !== undefined ? ` for Hold ${selectedHold}` : ''}
          </div>
        )}
      </div>
    </div>
  );
}

function FilterChip({ label, color, active, onClick }: { label: string; color: string; active: boolean; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      style={{
        padding: '2px 6px',
        fontSize: '9px',
        fontWeight: 'bold',
        fontFamily: 'monospace',
        background: active ? `${color}22` : 'transparent',
        border: `1px solid ${active ? color : '#333'}`,
        borderRadius: '3px',
        color: active ? color : '#666',
        cursor: 'pointer',
      }}
    >
      {label}
    </button>
  );
}
