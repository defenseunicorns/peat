import { useRef } from 'react';
import { useFrame } from '@react-three/fiber';
import { Text } from '@react-three/drei';
import type { Mesh, MeshStandardMaterial } from 'three';
import type { SensorVisualState } from '../../spatial/types';
import { SENSOR_POSITIONS, COLORS } from '../../spatial/constants';

interface Props {
  nodeId: string;
  sensor: SensorVisualState;
}

export default function SensorBeacon({ nodeId, sensor }: Props) {
  const domeRef = useRef<Mesh>(null);
  const phaseRef = useRef(0);
  const pos = SENSOR_POSITIONS[nodeId];
  if (!pos) return null;

  const color = sensor.calibrationPct >= 95
    ? COLORS.sensorActive
    : COLORS.sensorDrifting;

  useFrame((_, delta) => {
    phaseRef.current += delta * 4;
    // Pulse when emitting
    if (domeRef.current) {
      const mat = domeRef.current.material as MeshStandardMaterial;
      if (sensor.isEmitting) {
        const pulse = 0.5 + Math.sin(phaseRef.current) * 0.5;
        mat.emissiveIntensity = pulse * 0.6;
      } else {
        mat.emissiveIntensity = 0.1;
      }
    }
  });

  return (
    <group position={[pos.x, 0, pos.z]}>
      {/* Cylindrical base */}
      <mesh position={[0, 0.15, 0]}>
        <cylinderGeometry args={[0.2, 0.25, 0.3, 12]} />
        <meshStandardMaterial color="#444444" />
      </mesh>

      {/* Dome (animated emission) */}
      <mesh ref={domeRef} position={[0, 0.4, 0]}>
        <sphereGeometry args={[0.15, 16, 12, 0, Math.PI * 2, 0, Math.PI / 2]} />
        <meshStandardMaterial
          color={color}
          emissive={color}
          emissiveIntensity={0.1}
        />
      </mesh>

      {/* Label */}
      <Text
        position={[0, 0.75, 0]}
        fontSize={0.2}
        color={COLORS.text}
        anchorX="center"
        anchorY="middle"
      >
        {nodeId}
      </Text>
    </group>
  );
}
