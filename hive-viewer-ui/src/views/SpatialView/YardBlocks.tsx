import { Text } from '@react-three/drei';
import { YARD, COLORS } from '../../spatial/constants';

export default function YardBlocks() {
  const blocks = [];
  for (let i = 0; i < YARD.blockCount; i++) {
    const x = YARD.startX + i * (YARD.blockWidth + YARD.gap) + YARD.blockWidth / 2;
    blocks.push(
      <group key={i} position={[x, 0.1, YARD.z]}>
        <mesh>
          <boxGeometry args={[YARD.blockWidth, 0.2, YARD.blockDepth]} />
          <meshStandardMaterial color={COLORS.yard} />
        </mesh>
        <Text
          position={[0, 0.25, 0]}
          fontSize={0.3}
          color={COLORS.textDim}
          anchorX="center"
          anchorY="middle"
          rotation={[-Math.PI / 2, 0, 0]}
        >
          {YARD.labels[i]}
        </Text>
      </group>,
    );
  }
  return <group>{blocks}</group>;
}
