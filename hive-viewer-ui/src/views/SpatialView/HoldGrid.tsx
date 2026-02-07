import { useMemo } from 'react';
import { BoxGeometry } from 'three';
import { Text } from '@react-three/drei';
import { HOLDS, COLORS } from '../../spatial/constants';

export default function HoldGrid() {
  const edgeGeo = useMemo(
    () => new BoxGeometry(HOLDS.cellWidth - 0.1, 0.06, HOLDS.cellDepth - 0.1),
    [],
  );
  const cells = [];
  for (let i = 0; i < HOLDS.count; i++) {
    const x = HOLDS.startX + i * HOLDS.cellWidth + HOLDS.cellWidth / 2;
    const isHighlighted = i === HOLDS.highlightIndex;
    cells.push(
      <group key={i} position={[x, 0.52, HOLDS.z]}>
        {/* Hold cell */}
        <mesh>
          <boxGeometry args={[HOLDS.cellWidth - 0.2, 0.05, HOLDS.cellDepth - 0.2]} />
          <meshStandardMaterial
            color={isHighlighted ? COLORS.holdHighlight : COLORS.holdDefault}
            emissive={isHighlighted ? COLORS.holdHighlightEmissive : '#000000'}
            emissiveIntensity={isHighlighted ? 0.4 : 0}
          />
        </mesh>

        {/* Hold border */}
        <lineSegments>
          <edgesGeometry args={[edgeGeo]} />
          <lineBasicMaterial color={isHighlighted ? '#3b82f6' : '#374151'} />
        </lineSegments>

        {/* Label */}
        <Text
          position={[0, 0.1, 0]}
          fontSize={0.4}
          color={isHighlighted ? '#60a5fa' : COLORS.textDim}
          anchorX="center"
          anchorY="middle"
          rotation={[-Math.PI / 2, 0, 0]}
        >
          {`H${i + 1}`}
        </Text>
      </group>,
    );
  }
  return <group>{cells}</group>;
}
