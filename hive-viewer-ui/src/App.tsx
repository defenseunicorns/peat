import { useEffect, useCallback, useRef, useState, lazy, Suspense } from 'react';
import { useViewerStore } from './protocol/state';
import { parseReplayFile } from './protocol/replay';
import ConnectionStatus from './components/ConnectionStatus';
import MetricsPanel from './components/MetricsPanel';
import PlaybackControl from './components/PlaybackControl';
import HierarchyTree from './views/ProtocolView/HierarchyTree';
import EventStream from './views/ProtocolView/EventStream';
import CapabilityCards from './views/ProtocolView/CapabilityCard';

const SpatialView = lazy(() => import('./views/SpatialView'));

const EVENT_STREAM_MIN = 200;
const EVENT_STREAM_MAX = 600;
const EVENT_STREAM_DEFAULT = 320;

const LEFT_PANEL_MIN = 240;
const LEFT_PANEL_MAX = 500;
const LEFT_PANEL_DEFAULT = 320;

const SPEED_STEPS = [0.5, 1, 2, 4, 8, 16];

export default function App() {
  const connect = useViewerStore((s) => s.connect);
  const disconnect = useViewerStore((s) => s.disconnect);
  const replayMode = useViewerStore((s) => s.replayMode);
  const loadReplay = useViewerStore((s) => s.loadReplay);
  const exitReplay = useViewerStore((s) => s.exitReplay);
  const [esWidth, setEsWidth] = useState(EVENT_STREAM_DEFAULT);
  const [leftWidth, setLeftWidth] = useState(LEFT_PANEL_DEFAULT);
  const dragging = useRef<'left' | 'right' | false>(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  // Connect on mount (unless loading from ?replay= URL)
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const replayUrl = params.get('replay');
    if (replayUrl) {
      fetch(replayUrl)
        .then((r) => r.text())
        .then((text) => {
          const events = parseReplayFile(text);
          if (events.length > 0) loadReplay(events);
          else connect();
        })
        .catch(() => connect());
    } else {
      connect();
    }
    return () => disconnect();
  }, [connect, disconnect, loadReplay]);

  // Keyboard shortcuts (only in replay mode)
  useEffect(() => {
    if (!replayMode) return;
    const handler = (e: KeyboardEvent) => {
      // Don't intercept when typing in an input
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
      const store = useViewerStore.getState();
      switch (e.key) {
        case ' ':
          e.preventDefault();
          store.togglePlayPause();
          break;
        case 'ArrowLeft':
          e.preventDefault();
          store.step(-1);
          break;
        case 'ArrowRight':
          e.preventDefault();
          store.step(1);
          break;
        case 'Home':
          e.preventDefault();
          store.seekTo(0);
          break;
        case 'End':
          e.preventDefault();
          store.seekTo(store.replayLog.length - 1);
          break;
        case '[': {
          e.preventDefault();
          const cur = store.playbackSpeed || store._lastSpeed;
          const idx = SPEED_STEPS.indexOf(cur);
          if (idx > 0) store.setPlaybackSpeed(SPEED_STEPS[idx - 1]);
          break;
        }
        case ']': {
          e.preventDefault();
          const cur = store.playbackSpeed || store._lastSpeed;
          const idx = SPEED_STEPS.indexOf(cur);
          if (idx < SPEED_STEPS.length - 1) store.setPlaybackSpeed(SPEED_STEPS[idx + 1]);
          break;
        }
        case 'Escape':
          store.exitReplay();
          break;
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [replayMode]);

  const onDragStart = useCallback((side: 'left' | 'right', e: React.MouseEvent) => {
    e.preventDefault();
    dragging.current = side;
    const onMove = (ev: MouseEvent) => {
      if (!dragging.current) return;
      if (dragging.current === 'right') {
        setEsWidth(Math.min(EVENT_STREAM_MAX, Math.max(EVENT_STREAM_MIN, window.innerWidth - ev.clientX)));
      } else {
        setLeftWidth(Math.min(LEFT_PANEL_MAX, Math.max(LEFT_PANEL_MIN, ev.clientX)));
      }
    };
    const onUp = () => {
      dragging.current = false;
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }, []);

  const handleFileLoad = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      const text = reader.result as string;
      const events = parseReplayFile(text);
      if (events.length > 0) loadReplay(events);
    };
    reader.readAsText(file);
    // Reset input so the same file can be re-selected
    e.target.value = '';
  }, [loadReplay]);

  return (
    <div className="h-screen flex flex-col bg-gray-950 text-gray-100">
      {/* Top bar */}
      <header className="flex items-center justify-between px-4 py-2 border-b border-gray-800 bg-gray-900 shrink-0">
        <div className="flex items-center gap-3">
          <h1 className="text-sm font-bold tracking-wide">
            <span className="text-cyan-400">HIVE</span>{' '}
            <span className="text-gray-400">Viewer</span>
          </h1>
          <ConnectionStatus />
          {/* File loader */}
          <input
            ref={fileInputRef}
            type="file"
            accept=".jsonl,.json"
            onChange={handleFileLoad}
            className="hidden"
          />
          {replayMode ? (
            <button
              onClick={exitReplay}
              className="px-2 py-0.5 rounded text-[10px] leading-tight bg-amber-800 text-amber-200 hover:bg-amber-700"
            >
              Exit Replay
            </button>
          ) : (
            <button
              onClick={() => fileInputRef.current?.click()}
              className="px-2 py-0.5 rounded text-[10px] leading-tight bg-gray-800 text-gray-400 hover:text-gray-200 hover:bg-gray-700"
            >
              Load JSONL
            </button>
          )}
          {replayMode && (
            <span className="text-[10px] text-cyan-600">REPLAY</span>
          )}
        </div>
        <MetricsPanel />
      </header>

      {/* Main content — 3 column layout */}
      <main className="flex-1 flex overflow-hidden">
        {/* Left: Hierarchy + Capability Cards */}
        <aside className="border-r border-gray-800 flex flex-col shrink-0 overflow-hidden" style={{ width: leftWidth }}>
          <div className="h-80 border-b border-gray-800 shrink-0">
            <HierarchyTree />
          </div>
          <div className="flex-1 overflow-hidden">
            <CapabilityCards />
          </div>
        </aside>

        {/* Left drag handle */}
        <div
          onMouseDown={(e) => onDragStart('left', e)}
          className="w-1 shrink-0 cursor-col-resize bg-gray-800 hover:bg-cyan-700 transition-colors"
        />

        {/* Center: Spatial View */}
        <section className="flex-1 min-w-[400px] border-l border-gray-800 overflow-hidden">
          <Suspense
            fallback={
              <div className="flex items-center justify-center h-full text-gray-600 text-sm">
                Loading spatial view...
              </div>
            }
          >
            <SpatialView />
          </Suspense>
        </section>

        {/* Right drag handle */}
        <div
          onMouseDown={(e) => onDragStart('right', e)}
          className="w-1 shrink-0 cursor-col-resize bg-gray-800 hover:bg-cyan-700 transition-colors"
        />

        {/* Right: Event stream */}
        <section className="shrink-0 overflow-hidden" style={{ width: esWidth }}>
          <EventStream />
        </section>
      </main>

      {/* Bottom status bar */}
      <footer className="flex items-center justify-between px-4 py-1 border-t border-gray-800 bg-gray-900 text-[10px] text-gray-600 shrink-0">
        <div className="flex items-center gap-3">
          <span>HIVE Operational Viewer (ADR-053)</span>
          <PlaybackControl />
        </div>
        <span>(r)evolve</span>
      </footer>
    </div>
  );
}
