import { useRef } from 'react';
import { useFrame } from '@react-three/fiber';
import { Text } from '@react-three/drei';
import type { Group, Mesh, MeshStandardMaterial } from 'three';
import type { CraneVisualState } from '../../spatial/types';
import { CRANE_POSITIONS, COLORS } from '../../spatial/constants';

interface Props {
  nodeId: string;
  crane: CraneVisualState;
}

export default function CraneGantry({ nodeId, crane }: Props) {
  const spreaderRef = useRef<Group>(null);
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

    // Spreader oscillation when active
    if (spreaderRef.current) {
      if (crane.isActive) {
        spreaderRef.current.position.y = 3.5 + Math.sin(phaseRef.current) * 0.4;
      } else {
        spreaderRef.current.position.y = 4.2;
      }
    }

    // Contention ring expansion
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
      {/* Tower legs */}
      <mesh position={[-0.4, 2.5, 0]}>
        <boxGeometry args={[0.2, 5, 0.2]} />
        <meshStandardMaterial color={statusColor} />
      </mesh>
      <mesh position={[0.4, 2.5, 0]}>
        <boxGeometry args={[0.2, 5, 0.2]} />
        <meshStandardMaterial color={statusColor} />
      </mesh>

      {/* Cross beam */}
      <mesh position={[0, 5, 0]}>
        <boxGeometry args={[1.2, 0.2, 0.2]} />
        <meshStandardMaterial color={statusColor} />
      </mesh>

      {/* Boom (extends toward vessel) */}
      <mesh position={[0, 5, 2.5]}>
        <boxGeometry args={[0.15, 0.15, 5]} />
        <meshStandardMaterial color={statusColor} />
      </mesh>

      {/* Spreader (animated) */}
      <group ref={spreaderRef} position={[0, 4.2, 1.5]}>
        <mesh>
          <boxGeometry args={[0.8, 0.1, 0.4]} />
          <meshStandardMaterial color="#ffffff" />
        </mesh>
        {/* Cable */}
        <mesh position={[0, 0.5, 0]}>
          <cylinderGeometry args={[0.02, 0.02, 1]} />
          <meshStandardMaterial color="#888888" />
        </mesh>
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

