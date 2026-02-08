import { useEffect, lazy, Suspense } from 'react';
import { useViewerStore } from './protocol/state';
import ConnectionStatus from './components/ConnectionStatus';
import MetricsPanel from './components/MetricsPanel';
import PlaybackControl from './components/PlaybackControl';
import HierarchyTree from './views/ProtocolView/HierarchyTree';
import EventStream from './views/ProtocolView/EventStream';
import CapabilityCards from './views/ProtocolView/CapabilityCard';

const SpatialView = lazy(() => import('./views/SpatialView'));

export default function App() {
  const connect = useViewerStore((s) => s.connect);
  const disconnect = useViewerStore((s) => s.disconnect);

  useEffect(() => {
    connect();
    return () => disconnect();
  }, [connect, disconnect]);

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

        {/* Center: Event stream */}
        <section className="flex-1 min-w-[280px] overflow-hidden">
          <EventStream />
        </section>

        {/* Right: Spatial View */}
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
