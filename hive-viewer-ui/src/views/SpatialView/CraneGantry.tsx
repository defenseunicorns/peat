import { useRef } from 'react';
import { useFrame } from '@react-three/fiber';
import { Text } from '@react-three/drei';
import type { Group, Mesh, MeshStandardMaterial } from 'three';
import type { CraneVisualState } from '../../spatial/types';
import type { CraneAnimation, CranePhase } from '../../spatial/animationState';
import { useAnimationStore } from '../../spatial/animationState';
import { CRANE_POSITIONS, CRANE_ANIM, COLORS } from '../../spatial/constants';

function easeInOut(t: number): number {
  return t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2;
}

function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

interface Props {
  nodeId: string;
  crane: CraneVisualState;
  animation: CraneAnimation | null;
}

export default function CraneGantry({ nodeId, crane, animation: _animation }: Props) {
  const boomRef = useRef<Group>(null);
  const trolleyRef = useRef<Group>(null);
  const spreaderRef = useRef<Group>(null);
  const containerRef = useRef<Mesh>(null);
  const ringRef = useRef<Group>(null);
  const phaseRef = useRef(0);
  const pos = CRANE_POSITIONS[nodeId];
  if (!pos) return null;

  const statusColor =
    crane.equipmentStatus === 'failed'
      ? COLORS.craneFailed
      : crane.equipmentStatus === 'degraded'
        ? COLORS.craneDegraded
        : COLORS.craneOperational;

  useFrame((_, delta) => {
    phaseRef.current += delta * 3;

    // Read animation state imperatively (avoids React re-renders)
    const anim = useAnimationStore.getState().cranes[nodeId];
    const phase: CranePhase = anim?.phase ?? 'idle';
    const t = anim ? easeInOut(anim.phaseProgress) : 0;

    // --- Boom rotation ---
    if (boomRef.current) {
      let targetAngle = 0;
      if (phase === 'boom_to_ship') {
        targetAngle = lerp(0, CRANE_ANIM.BOOM_SHIP_ANGLE, t);
      } else if (
        phase === 'trolley_extend' || phase === 'spreader_lower' ||
        phase === 'grip' || phase === 'spreader_hoist'
      ) {
        targetAngle = CRANE_ANIM.BOOM_SHIP_ANGLE;
      } else if (phase === 'boom_to_shore') {
        targetAngle = lerp(CRANE_ANIM.BOOM_SHIP_ANGLE, CRANE_ANIM.BOOM_SHORE_ANGLE, t);
      } else if (
        phase === 'spreader_lower_truck' || phase === 'release' || phase === 'spreader_return'
      ) {
        targetAngle = CRANE_ANIM.BOOM_SHORE_ANGLE;
      }
      boomRef.current.rotation.y = targetAngle;
    }

    // --- Trolley position along boom ---
    if (trolleyRef.current) {
      let targetZ = CRANE_ANIM.TROLLEY_RETRACTED_Z;
      if (phase === 'trolley_extend') {
        targetZ = lerp(CRANE_ANIM.TROLLEY_RETRACTED_Z, CRANE_ANIM.TROLLEY_EXTENDED_Z, t);
      } else if (
        phase === 'spreader_lower' || phase === 'grip' || phase === 'spreader_hoist'
      ) {
        targetZ = CRANE_ANIM.TROLLEY_EXTENDED_Z;
      } else if (
        phase === 'boom_to_shore' || phase === 'spreader_lower_truck' ||
        phase === 'release' || phase === 'spreader_return'
      ) {
        targetZ = CRANE_ANIM.TROLLEY_RETRACTED_Z;
      }
      trolleyRef.current.position.z = targetZ;
    }

    // --- Spreader vertical (local Y relative to boom at Y=5) ---
    if (spreaderRef.current) {
      const BOOM_Y = 5;
      const restLocal = CRANE_ANIM.SPREADER_REST_Y - BOOM_Y;   // -0.8
      const grabLocal = CRANE_ANIM.SPREADER_GRAB_Y - BOOM_Y;   // -4.0
      const truckLocal = CRANE_ANIM.SPREADER_TRUCK_Y - BOOM_Y; // -3.5

      let targetY = restLocal;
      if (phase === 'spreader_lower') {
        targetY = lerp(restLocal, grabLocal, t);
      } else if (phase === 'grip') {
        targetY = grabLocal;
      } else if (phase === 'spreader_hoist') {
        targetY = lerp(grabLocal, restLocal, t);
      } else if (phase === 'boom_to_shore' || phase === 'boom_to_ship') {
        targetY = restLocal;
      } else if (phase === 'spreader_lower_truck') {
        targetY = lerp(restLocal, truckLocal, t);
      } else if (phase === 'release') {
        targetY = truckLocal;
      } else if (phase === 'spreader_return') {
        targetY = lerp(truckLocal, restLocal, t);
      } else if (phase === 'idle' && crane.isActive) {
        // Fallback: gentle oscillation when active but no spatial animation
        targetY = (3.5 - BOOM_Y) + Math.sin(phaseRef.current) * 0.4;
      }
      spreaderRef.current.position.y = targetY;
    }

    // --- Container visibility on spreader ---
    if (containerRef.current) {
      const gripped =
        phase === 'grip' || phase === 'spreader_hoist' ||
        phase === 'boom_to_shore' || phase === 'spreader_lower_truck';
      containerRef.current.visible = gripped;
    }

    // --- Contention ring ---
    if (ringRef.current) {
      if (crane.isContending) {
        ringRef.current.visible = true;
        const scale = 1 + (phaseRef.current % 2) * 0.8;
        const opacity = Math.max(0, 1 - (phaseRef.current % 2) / 2);
        ringRef.current.scale.setScalar(scale);
        const mat = (ringRef.current.children[0] as Mesh)
          ?.material as MeshStandardMaterial;
        if (mat) mat.opacity = opacity;
      } else {
        ringRef.current.visible = false;
      }
    }
  });

  return (
    <group position={[pos.x, 0, pos.z]}>
      {/* Tower legs (static) */}
      <mesh position={[-0.4, 2.5, 0]}>
        <boxGeometry args={[0.2, 5, 0.2]} />
        <meshStandardMaterial color={statusColor} />
      </mesh>
      <mesh position={[0.4, 2.5, 0]}>
        <boxGeometry args={[0.2, 5, 0.2]} />
        <meshStandardMaterial color={statusColor} />
      </mesh>

      {/* Cross beam (static) */}
      <mesh position={[0, 5, 0]}>
        <boxGeometry args={[1.2, 0.2, 0.2]} />
        <meshStandardMaterial color={statusColor} />
      </mesh>

      {/* Boom (rotates around Y) */}
      <group ref={boomRef} position={[0, 5, 0]}>
        {/* Boom bar */}
        <mesh position={[0, 0, 2.5]}>
          <boxGeometry args={[0.15, 0.15, 5]} />
          <meshStandardMaterial color={statusColor} />
        </mesh>

        {/* Trolley (slides along boom Z) */}
        <group ref={trolleyRef} position={[0, 0, CRANE_ANIM.TROLLEY_RETRACTED_Z]}>
          {/* Spreader (moves vertically) */}
          <group ref={spreaderRef} position={[0, CRANE_ANIM.SPREADER_REST_Y - 5, 0]}>
            {/* Spreader frame */}
            <mesh>
              <boxGeometry args={[0.8, 0.1, 0.4]} />
              <meshStandardMaterial color="#ffffff" />
            </mesh>
            {/* Cable */}
            <mesh position={[0, 0.5, 0]}>
              <cylinderGeometry args={[0.02, 0.02, 1]} />
              <meshStandardMaterial color="#888888" />
            </mesh>
            {/* Container (visible only when gripped) */}
            <mesh ref={containerRef} position={[0, -0.25, 0]} visible={false}>
              <boxGeometry args={CRANE_ANIM.CONTAINER_SIZE} />
              <meshStandardMaterial color={COLORS.containerInProgress} />
            </mesh>
          </group>
        </group>
      </group>

      {/* Contention ring */}
      <group ref={ringRef} position={[0, 2, 0]} visible={false}>
        <mesh rotation={[-Math.PI / 2, 0, 0]}>
          <ringGeometry args={[0.8, 1.0, 32]} />
          <meshStandardMaterial
            color={COLORS.craneContention}
            transparent
            opacity={0.6}
          />
        </mesh>
      </group>

      {/* Label */}
      <Text
        position={[0, 5.8, 0]}
        fontSize={0.35}
        color={COLORS.text}
        anchorX="center"
        anchorY="middle"
      >
        {`${nodeId} [${crane.moveCount}]`}
      </Text>
    </group>
  );
}
