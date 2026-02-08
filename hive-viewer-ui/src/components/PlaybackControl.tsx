import { useViewerStore } from '../protocol/state';

const SPEEDS = [
  { value: 0, label: '\u23F8' },
  { value: 0.5, label: '\u00BD\u00D7' },
  { value: 1, label: '1\u00D7' },
  { value: 2, label: '2\u00D7' },
  { value: 4, label: '4\u00D7' },
];

export default function PlaybackControl() {
  const speed = useViewerStore((s) => s.playbackSpeed);
  const queueLen = useViewerStore((s) => s.playbackQueue.length);
  const setSpeed = useViewerStore((s) => s.setPlaybackSpeed);

  return (
    <div className="flex items-center gap-1">
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
      {speed === 0 && queueLen > 0 && (
        <span className="text-[10px] text-yellow-500 ml-1">{queueLen} queued</span>
      )}
    </div>
  );
}
