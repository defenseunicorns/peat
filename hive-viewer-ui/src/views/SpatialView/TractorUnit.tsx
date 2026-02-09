import { useRef } from 'react';
import { useFrame } from '@react-three/fiber';
import { Text } from '@react-three/drei';
import type { Group, Mesh } from 'three';
import type { TractorVisualState } from '../../spatial/types';
import type { TractorAnimation, TractorPhase } from '../../spatial/animationState';
import { useAnimationStore } from '../../spatial/animationState';
import { TRACTOR_POSITIONS, YARD, COLORS } from '../../spatial/constants';

function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

function easeInOut(t: number): number {
  return t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2;
}

/** Compute yard block center position from block name (e.g. "YB-A" → first block). */
function getYardBlockPosition(blockName: string): { x: number; z: number } {
  const idx = YARD.labels.indexOf(blockName);
  const i = idx >= 0 ? idx : 0;
  const x = YARD.startX + i * (YARD.blockWidth + YARD.gap) + YARD.blockWidth / 2;
  return { x, z: YARD.z };
}

interface Props {
  nodeId: string;
  tractor: TractorVisualState;
  animation: TractorAnimation | null;
}

export default function TractorUnit({ nodeId, tractor, animation: _animation }: Props) {
  const groupRef = useRef<Group>(null);
  const bodyRef = useRef<Mesh>(null);
  const cargoRef = useRef<Mesh>(null);
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

    // Read animation state imperatively
    const anim = useAnimationStore.getState().tractors[nodeId];
    const phase: TractorPhase = anim?.phase ?? 'idle';
    const t = anim ? easeInOut(anim.phaseProgress) : 0;

    // Position interpolation
    if (groupRef.current) {
      const home = { x: 0, z: 0 }; // group is already positioned at pos.x, pos.z
      if (phase === 'drive_to_yard') {
        const yard = getYardBlockPosition(anim!.destinationBlock);
        const targetX = yard.x - pos.x;
        const targetZ = yard.z - pos.z;
        groupRef.current.position.x = lerp(home.x, targetX, t);
        groupRef.current.position.z = lerp(home.z, targetZ, t);
      } else if (phase === 'unload') {
        const yard = getYardBlockPosition(anim!.destinationBlock);
        groupRef.current.position.x = yard.x - pos.x;
        groupRef.current.position.z = yard.z - pos.z;
      } else if (phase === 'drive_to_berth') {
        const yard = getYardBlockPosition(anim!.destinationBlock);
        const fromX = yard.x - pos.x;
        const fromZ = yard.z - pos.z;
        groupRef.current.position.x = lerp(fromX, home.x, t);
        groupRef.current.position.z = lerp(fromZ, home.z, t);
      } else {
        // idle: smoothly return to home
        groupRef.current.position.x += (home.x - groupRef.current.position.x) * delta * 3;
        groupRef.current.position.z += (home.z - groupRef.current.position.z) * delta * 3;
      }
    }

    // Gentle bob when moving
    if (bodyRef.current && (tractor.isMoving || phase !== 'idle')) {
      bodyRef.current.position.y = 0.3 + Math.sin(phaseRef.current) * 0.05;
    }

    // Cargo visibility
    if (cargoRef.current) {
      cargoRef.current.visible = phase === 'drive_to_yard' || phase === 'unload';
    }
  });

  const batteryWidth = Math.max(0, (tractor.batteryPct / 100) * 0.8);
  const batteryColor = tractor.batteryPct > 50 ? '#22c55e' : tractor.batteryPct > 25 ? '#eab308' : '#ef4444';

  return (
    <group position={[pos.x, 0, pos.z]}>
      <group ref={groupRef}>
        {/* Tractor body */}
        <mesh ref={bodyRef} position={[0, 0.3, 0]}>
          <boxGeometry args={[0.9, 0.35, 0.5]} />
          <meshStandardMaterial color={color} />
        </mesh>

        {/* Cab */}
        <mesh position={[0.25, 0.55, 0]}>
          <boxGeometry args={[0.3, 0.2, 0.45]} />
          <meshStandardMaterial color="#333333" />
        </mesh>

        {/* Container cargo (visible during transport) */}
        <mesh ref={cargoRef} position={[-0.15, 0.65, 0]} visible={false}>
          <boxGeometry args={[0.6, 0.3, 0.45]} />
          <meshStandardMaterial color={COLORS.containerInProgress} />
        </mesh>

        {/* Battery bar */}
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
    </group>
  );
}
