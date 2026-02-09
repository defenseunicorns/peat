import { useViewerStore } from '../protocol/state';

const SPEEDS = [
  { value: 0.5, label: '\u00BD\u00D7' },
  { value: 1, label: '1\u00D7' },
  { value: 2, label: '2\u00D7' },
  { value: 4, label: '4\u00D7' },
];

export default function PlaybackControl() {
  const speed = useViewerStore((s) => s.playbackSpeed);
  const queueLen = useViewerStore((s) => s.playbackQueue.length);
  const setSpeed = useViewerStore((s) => s.setPlaybackSpeed);
  const togglePlayPause = useViewerStore((s) => s.togglePlayPause);
  const restart = useViewerStore((s) => s.restart);

  const paused = speed === 0;

  return (
    <div className="flex items-center gap-1">
      <button
        onClick={togglePlayPause}
        className={`px-1.5 py-0 rounded text-[10px] leading-tight ${
          paused
            ? 'text-cyan-400 bg-gray-800'
            : 'text-gray-600 hover:text-gray-400'
        }`}
        title={paused ? 'Play' : 'Pause'}
      >
        {paused ? '\u25B6' : '\u23F8'}
      </button>
      {SPEEDS.map((s) => (
        <button
          key={s.value}
          onClick={() => setSpeed(s.value)}
          className={`px-1.5 py-0 rounded text-[10px] leading-tight ${
            speed === s.value
              ? 'text-cyan-400 bg-gray-800'
              : 'text-gray-600 hover:text-gray-400'
          }`}
        >
          {s.label}
        </button>
      ))}
      <button
        onClick={restart}
        className="px-1.5 py-0 rounded text-[10px] leading-tight text-gray-600 hover:text-gray-400"
        title="Restart"
      >
        {'\u27F2'}
      </button>
      {paused && queueLen > 0 && (
        <span className="text-[10px] text-yellow-500 ml-1">{queueLen} queued</span>
      )}
    </div>
  );
}
