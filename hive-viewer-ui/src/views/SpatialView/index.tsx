import { Canvas } from '@react-three/fiber';
import { useSpatialState } from '../../spatial/useSpatialState';
import BerthScene from './BerthScene';
import TeamSummaryHUD from './TeamSummaryHUD';

export default function SpatialView() {
  const state = useSpatialState();

  return (
    <div className="relative w-full h-full">
      <Canvas
        orthographic
        gl={{ antialias: true }}
        style={{ background: '#0a1628' }}
      >
        <BerthScene state={state} />
      </Canvas>
      <TeamSummaryHUD />
    </div>
  );
}
