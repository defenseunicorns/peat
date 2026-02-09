import { useEffect, useCallback, useRef, useState, lazy, Suspense } from 'react';
import { useViewerStore } from './protocol/state';
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

export default function App() {
  const connect = useViewerStore((s) => s.connect);
  const disconnect = useViewerStore((s) => s.disconnect);
  const [esWidth, setEsWidth] = useState(EVENT_STREAM_DEFAULT);
  const dragging = useRef(false);

  useEffect(() => {
    connect();
    return () => disconnect();
  }, [connect, disconnect]);

  const onDragStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    dragging.current = true;
    const onMove = (ev: MouseEvent) => {
      if (!dragging.current) return;
      const newWidth = Math.min(EVENT_STREAM_MAX, Math.max(EVENT_STREAM_MIN, window.innerWidth - ev.clientX));
      setEsWidth(newWidth);
    };
    const onUp = () => {
      dragging.current = false;
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }, []);

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
        </div>
        <MetricsPanel />
      </header>

      {/* Main content — 3 column layout */}
      <main className="flex-1 flex overflow-hidden">
        {/* Left: Hierarchy + Capability Cards */}
        <aside className="w-60 border-r border-gray-800 flex flex-col shrink-0 overflow-hidden">
          <div className="h-72 border-b border-gray-800 shrink-0">
            <HierarchyTree />
          </div>
          <div className="flex-1 overflow-hidden">
            <CapabilityCards />
          </div>
        </aside>

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

        {/* Drag handle */}
        <div
          onMouseDown={onDragStart}
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
