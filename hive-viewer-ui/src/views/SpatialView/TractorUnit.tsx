import { useRef } from 'react';
import { useFrame } from '@react-three/fiber';
import { Text } from '@react-three/drei';
import type { Mesh } from 'three';
import type { TractorVisualState } from '../../spatial/types';
import { TRACTOR_POSITIONS, COLORS } from '../../spatial/constants';

interface Props {
  nodeId: string;
  tractor: TractorVisualState;
}

export default function TractorUnit({ nodeId, tractor }: Props) {
  const bodyRef = useRef<Mesh>(null);
  const phaseRef = useRef(0);
  const pos = TRACTOR_POSITIONS[nodeId];
  if (!pos) return null;

  const color = tractor.isCharging
    ? COLORS.tractorCharging
    : tractor.isMoving
      ? COLORS.tractorMoving
      : COLORS.tractorIdle;

  useFrame((_, delta) => {
    phaseRef.current += delta * 2;
    // Gentle bob when moving
    if (bodyRef.current && tractor.isMoving) {
      bodyRef.current.position.y = 0.3 + Math.sin(phaseRef.current) * 0.05;
    }
  });

  const batteryWidth = Math.max(0, (tractor.batteryPct / 100) * 0.8);
  const batteryColor = tractor.batteryPct > 50 ? '#22c55e' : tractor.batteryPct > 25 ? '#eab308' : '#ef4444';

  return (
    <group position={[pos.x, 0, pos.z]}>
      {/* Tractor body — rectangular */}
      <mesh ref={bodyRef} position={[0, 0.3, 0]}>
        <boxGeometry args={[0.9, 0.35, 0.5]} />
        <meshStandardMaterial color={color} />
      </mesh>

      {/* Cab */}
      <mesh position={[0.25, 0.55, 0]}>
        <boxGeometry args={[0.3, 0.2, 0.45]} />
        <meshStandardMaterial color="#333333" />
      </mesh>

      {/* Battery bar (below body) */}
      <mesh position={[-0.4 + batteryWidth / 2, 0.08, 0]}>
        <boxGeometry args={[batteryWidth, 0.06, 0.3]} />
        <meshStandardMaterial color={batteryColor} />
      </mesh>

      {/* Label */}
      <Text
        position={[0, 0.9, 0]}
        fontSize={0.25}
        color={COLORS.text}
        anchorX="center"
        anchorY="middle"
      >
        {`${nodeId} [${tractor.tripsCompleted}]`}
      </Text>
    </group>
  );
}
