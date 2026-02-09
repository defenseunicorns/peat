/** Connection status indicator — shows green/yellow/red dot with label. */

import { useCommanderStore } from '../../protocol/store';

const STATUS_CONFIG = {
  connected: { color: '#44ff44', label: 'LIVE', bg: 'rgba(0,80,0,0.3)' },
  connecting: { color: '#ffaa00', label: 'CONNECTING...', bg: 'rgba(80,60,0,0.3)' },
  disconnected: { color: '#ff4444', label: 'OFFLINE', bg: 'rgba(80,0,0,0.3)' },
} as const;

export function ConnectionStatus() {
  const status = useCommanderStore((s) => s.status);
  const connect = useCommanderStore((s) => s.connect);
  const cfg = STATUS_CONFIG[status];

  return (
    <div
      onClick={() => { if (status === 'disconnected') connect(); }}
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: '6px',
        padding: '4px 10px',
        borderRadius: '4px',
        background: cfg.bg,
        cursor: status === 'disconnected' ? 'pointer' : 'default',
        fontSize: '11px',
        fontWeight: 'bold',
        color: cfg.color,
        userSelect: 'none',
      }}
      title={status === 'disconnected' ? 'Click to reconnect' : `Status: ${status}`}
    >
      <span style={{
        width: '8px',
        height: '8px',
        borderRadius: '50%',
        background: cfg.color,
        boxShadow: status === 'connected' ? `0 0 6px ${cfg.color}` : 'none',
      }} />
      {cfg.label}
    </div>
  );
}
