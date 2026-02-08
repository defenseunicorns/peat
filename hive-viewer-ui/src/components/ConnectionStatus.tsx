/** Connection status indicator with debug info. */

import { useViewerStore } from '../protocol/state';

const STATUS_STYLES = {
  connected: { dot: 'bg-green-400', label: 'Connected', labelClass: 'text-green-400' },
  connecting: { dot: 'bg-yellow-400 animate-pulse', label: 'Connecting...', labelClass: 'text-yellow-400' },
  disconnected: { dot: 'bg-red-400', label: 'Disconnected', labelClass: 'text-red-400' },
};

export default function ConnectionStatus() {
  const status = useViewerStore((s) => s.status);
  const wsUrl = useViewerStore((s) => s.wsUrl);
  const lastError = useViewerStore((s) => s.lastError);
  const style = STATUS_STYLES[status];

  return (
    <div className="flex items-center gap-2 text-xs">
      <div className={`w-2 h-2 rounded-full ${style.dot}`} />
      <span className={style.labelClass}>{style.label}</span>
      {wsUrl && (
        <span className="text-gray-600 font-mono text-[10px]">{wsUrl}</span>
      )}
      {lastError && (
        <span className="text-red-500 text-[10px]">{lastError}</span>
      )}
    </div>
  );
}
