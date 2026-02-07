import { Text } from '@react-three/drei';
import { VESSEL, COLORS } from '../../spatial/constants';

export default function VesselHull() {
  return (
    <group>
      {/* Hull */}
      <mesh position={[0, VESSEL.y, 0]}>
        <boxGeometry args={[VESSEL.length, 0.5, VESSEL.beam]} />
        <meshStandardMaterial color={VESSEL.color} />
      </mesh>

      {/* Deck edge rails */}
      <mesh position={[0, 0.55, VESSEL.beam / 2]}>
        <boxGeometry args={[VESSEL.length, 0.1, 0.1]} />
        <meshStandardMaterial color={COLORS.vesselDeck} />
      </mesh>
      <mesh position={[0, 0.55, -VESSEL.beam / 2]}>
        <boxGeometry args={[VESSEL.length, 0.1, 0.1]} />
        <meshStandardMaterial color={COLORS.vesselDeck} />
      </mesh>

      {/* Label */}
      <Text
        position={[0, 0.6, -VESSEL.beam / 2 - 0.8]}
        fontSize={0.5}
        color={COLORS.textDim}
        anchorX="center"
        anchorY="middle"
      >
        {VESSEL.label}
      </Text>
    </group>
  );
}
