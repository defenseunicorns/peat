import { useMemo } from 'react';
import { OrbitControls, OrthographicCamera } from '@react-three/drei';
import type { SpatialDerivedState } from '../../spatial/types';
import { CAMERA, COLORS } from '../../spatial/constants';
import { useAnimationStore } from '../../spatial/animationState';
import { useAnimationTick } from '../../spatial/useAnimationTick';
import VesselHull from './VesselHull';
import HoldGrid from './HoldGrid';
import ContainerQueue from './ContainerQueue';
import CraneGantry from './CraneGantry';
import OperatorFigure from './OperatorFigure';
import TractorUnit from './TractorUnit';
import SensorBeacon from './SensorBeacon';
import YardBlocks from './YardBlocks';
import ContentionFlash from './ContentionFlash';

interface Props {
  state: SpatialDerivedState;
}

export default function BerthScene({ state }: Props) {
  const anyContention = Object.values(state.cranes).some((c) => c.isContending);

  // Drive animation state machines every frame
  useAnimationTick();

  const craneAnims = useAnimationStore((s) => s.cranes);
  const tractorAnims = useAnimationStore((s) => s.tractors);

  // Collect container indices currently being animated by cranes
  const animatingContainerIndices = useMemo(() => {
    const indices = new Set<number>();
    for (const anim of Object.values(craneAnims)) {
      if (anim.phase !== 'idle') {
        indices.add(anim.containerIndex);
      }
    }
    return indices;
  }, [craneAnims]);

  return (
    <>
      {/* Camera */}
      <OrthographicCamera
        makeDefault
        position={[CAMERA.position[0], CAMERA.position[1], CAMERA.position[2]]}
        zoom={CAMERA.zoom}
        near={0.1}
        far={100}
      />

      {/* Controls — constrained rotation for 2.5D feel */}
      <OrbitControls
        enableRotate
        maxPolarAngle={Math.PI / 3}
        minPolarAngle={Math.PI / 6}
        enablePan
        enableZoom
        minZoom={10}
        maxZoom={60}
      />

      {/* Lighting */}
      <ambientLight intensity={0.3} />
      <directionalLight position={[10, 20, 10]} intensity={0.8} />

      {/* Water plane */}
      <mesh position={[0, -0.05, 0]} rotation={[-Math.PI / 2, 0, 0]}>
        <planeGeometry args={[80, 60]} />
        <meshStandardMaterial color={COLORS.water} />
      </mesh>

      {/* Berth platform */}
      <mesh position={[0, 0.02, -5]}>
        <boxGeometry args={[40, 0.04, 4]} />
        <meshStandardMaterial color={COLORS.berth} />
      </mesh>

      {/* Scene elements */}
      <VesselHull />
      <HoldGrid />
      <ContainerQueue state={state} animatingIndices={animatingContainerIndices} />

      {/* Cranes */}
      {Object.entries(state.cranes).map(([nodeId, crane]) => (
        <CraneGantry
          key={nodeId}
          nodeId={nodeId}
          crane={crane}
          animation={craneAnims[nodeId] ?? null}
        />
      ))}

      {/* Operators */}
      {Object.entries(state.operators).map(([nodeId, op]) => (
        <OperatorFigure key={nodeId} nodeId={nodeId} operator={op} />
      ))}

      {/* Tractors */}
      {Object.entries(state.tractors).map(([nodeId, tractor]) => (
        <TractorUnit
          key={nodeId}
          nodeId={nodeId}
          tractor={tractor}
          animation={tractorAnims[nodeId] ?? null}
        />
      ))}

      {/* Sensors */}
      {Object.entries(state.sensors).map(([nodeId, sensor]) => (
        <SensorBeacon key={nodeId} nodeId={nodeId} sensor={sensor} />
      ))}

      <YardBlocks />
      <ContentionFlash active={anyContention} />
    </>
  );
}
