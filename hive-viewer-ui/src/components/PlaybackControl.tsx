import { useViewerStore } from '../protocol/state';

const LIVE_SPEEDS = [
  { value: 0.5, label: '\u00BD\u00D7' },
  { value: 1, label: '1\u00D7' },
  { value: 2, label: '2\u00D7' },
  { value: 4, label: '4\u00D7' },
];

const REPLAY_SPEEDS = [
  { value: 0.5, label: '\u00BD\u00D7' },
  { value: 1, label: '1\u00D7' },
  { value: 2, label: '2\u00D7' },
  { value: 4, label: '4\u00D7' },
  { value: 8, label: '8\u00D7' },
  { value: 16, label: '16\u00D7' },
];

const btnBase = 'px-1.5 py-0 rounded text-[10px] leading-tight';
const btnActive = `${btnBase} text-cyan-400 bg-gray-800`;
const btnInactive = `${btnBase} text-gray-600 hover:text-gray-400`;

export default function PlaybackControl() {
  const speed = useViewerStore((s) => s.playbackSpeed);
  const queueLen = useViewerStore((s) => s.playbackQueue.length);
  const setSpeed = useViewerStore((s) => s.setPlaybackSpeed);
  const togglePlayPause = useViewerStore((s) => s.togglePlayPause);
  const restart = useViewerStore((s) => s.restart);
  const replayMode = useViewerStore((s) => s.replayMode);
  const replayCursor = useViewerStore((s) => s.replayCursor);
  const replayMeta = useViewerStore((s) => s.replayMeta);
  const seekTo = useViewerStore((s) => s.seekTo);
  const step = useViewerStore((s) => s.step);
  const simClock = useViewerStore((s) => s.simClock);

  const paused = speed === 0;

  if (replayMode) {
    const total = replayMeta?.totalFrames ?? 0;
    const speeds = REPLAY_SPEEDS;

    return (
      <div className="flex items-center gap-1">
        {/* Navigation buttons */}
        <button onClick={() => seekTo(0)} className={btnInactive} title="Home (seek to start)">
          {'\u23EE'}
        </button>
        <button onClick={() => step(-1)} className={btnInactive} title="Step back 1 frame">
          {'\u23EA'}
        </button>
        <button
          onClick={togglePlayPause}
          className={paused ? btnActive : btnInactive}
          title={paused ? 'Play' : 'Pause'}
        >
          {paused ? '\u23EF' : '\u23F8'}
        </button>
        <button onClick={() => step(1)} className={btnInactive} title="Step forward 1 frame">
          {'\u23E9'}
        </button>
        <button
          onClick={() => seekTo(total - 1)}
          className={btnInactive}
          title="End (seek to last frame)"
        >
          {'\u23ED'}
        </button>

        {/* Speed buttons */}
        {speeds.map((s) => (
          <button
            key={s.value}
            onClick={() => setSpeed(s.value)}
            className={speed === s.value ? btnActive : btnInactive}
          >
            {s.label}
          </button>
        ))}

        {/* Timeline scrubber */}
        <input
          type="range"
          min={0}
          max={Math.max(0, total - 1)}
          value={replayCursor}
          onChange={(e) => seekTo(Number(e.target.value))}
          className="w-32 h-2 mx-1 accent-cyan-400"
          title={`Frame ${replayCursor}/${total}`}
        />

        {/* Frame counter */}
        <span className="text-[10px] text-gray-400 tabular-nums min-w-[60px]">
          {replayCursor}/{total}
        </span>

        {/* Sim time */}
        {simClock?.sim_time && (
          <span className="text-[10px] text-cyan-600 ml-1">
            {simClock.sim_time}
          </span>
        )}
      </div>
    );
  }

  // Live mode — original controls
  return (
    <div className="flex items-center gap-1">
      <button
        onClick={togglePlayPause}
        className={paused ? btnActive : btnInactive}
        title={paused ? 'Play' : 'Pause'}
      >
        {paused ? '\u25B6' : '\u23F8'}
      </button>
      {LIVE_SPEEDS.map((s) => (
        <button
          key={s.value}
          onClick={() => setSpeed(s.value)}
          className={speed === s.value ? btnActive : btnInactive}
        >
          {s.label}
        </button>
      ))}
      <button
        onClick={restart}
        className={btnInactive}
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
