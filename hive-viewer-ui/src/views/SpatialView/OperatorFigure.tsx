import { useMemo } from 'react';
import { BufferGeometry, Float32BufferAttribute } from 'three';
import { OPERATOR_POSITIONS, CRANE_POSITIONS, COLORS } from '../../spatial/constants';
import type { OperatorVisualState } from '../../spatial/types';

interface Props {
  nodeId: string;
  operator: OperatorVisualState;
}

export default function OperatorFigure({ nodeId, operator }: Props) {
  const pos = OPERATOR_POSITIONS[nodeId] ?? { x: 0, z: -4 };
  const color = operator.assignedTo
    ? COLORS.operatorAssigned
    : operator.isOnBreak
      ? COLORS.operatorBreak
      : COLORS.operatorAvailable;

  const cranePos = operator.assignedTo ? CRANE_POSITIONS[operator.assignedTo] : null;

  const lineGeom = useMemo(() => {
    if (!cranePos) return null;
    const geom = new BufferGeometry();
    const positions = new Float32Array([
      0, 0.3, 0,
      cranePos.x - pos.x, 0.3, cranePos.z - pos.z,
    ]);
    geom.setAttribute('position', new Float32BufferAttribute(positions, 3));
    return geom;
  }, [cranePos, pos.x, pos.z]);

  return (
    <group position={[pos.x, 0.5, pos.z]}>
      {/* Head */}
      <mesh position={[0, 0.6, 0]}>
        <sphereGeometry args={[0.2, 8, 8]} />
        <meshStandardMaterial color={color} />
      </mesh>
      {/* Body */}
      <mesh position={[0, 0.15, 0]}>
        <cylinderGeometry args={[0.12, 0.15, 0.5, 8]} />
        <meshStandardMaterial color={color} />
      </mesh>
      {/* Assignment line to crane */}
      {lineGeom && (
        <line>
          <primitive object={lineGeom} attach="geometry" />
          <lineBasicMaterial color={COLORS.operatorAssigned} transparent opacity={0.4} />
        </line>
      )}
    </group>
  );
}
