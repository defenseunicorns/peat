import { useRef } from 'react';
import { useFrame } from '@react-three/fiber';
import type { Mesh, MeshStandardMaterial } from 'three';
import type { SpatialDerivedState } from '../../spatial/types';
import { CONTAINER_GRID, COLORS } from '../../spatial/constants';

interface Props {
  state: SpatialDerivedState;
}

function ContainerBox({
  index,
  col,
  row,
  isHazmat,
  status,
}: {
  index: number;
  col: number;
  row: number;
  isHazmat: boolean;
  status: 'pending' | 'in_progress' | 'completed';
}) {
  const meshRef = useRef<Mesh>(null);
  const phaseRef = useRef(Math.random() * Math.PI * 2);

  useFrame((_, delta) => {
    if (!meshRef.current) return;
    phaseRef.current += delta * 2;

    if (status === 'in_progress') {
      // Gentle bob
      meshRef.current.position.y = 0.85 + Math.sin(phaseRef.current) * 0.05;
    } else if (status === 'completed') {
      // Brief scale pulse (settles to 1)
      const s = meshRef.current.scale.x;
      if (s > 1.01) {
        meshRef.current.scale.setScalar(s + (1 - s) * delta * 4);
      }
    }

    // Hazmat emissive pulse
    if (isHazmat && status === 'pending') {
      const mat = meshRef.current.material as MeshStandardMaterial;
      mat.emissiveIntensity = 0.3 + Math.sin(phaseRef.current * 1.5) * 0.2;
    }
  });

  const cellStep = CONTAINER_GRID.cellSize + CONTAINER_GRID.gap;
  const gridW = (CONTAINER_GRID.cols - 1) * cellStep;
  const gridD = (CONTAINER_GRID.rows - 1) * cellStep;
  const x = CONTAINER_GRID.originX + col * cellStep - gridW / 2;
  const z = CONTAINER_GRID.originZ + row * cellStep - gridD / 2;

  let color = COLORS.containerPending;
  let emissive = '#000000';
  if (status === 'completed') color = COLORS.containerCompleted;
  else if (status === 'in_progress') color = COLORS.containerInProgress;
  else if (isHazmat) {
    color = COLORS.containerHazmat;
    emissive = COLORS.containerHazmatEmissive;
  }

  return (
    <mesh
      ref={meshRef}
      key={index}
      position={[x, 0.85, z]}
      scale={status === 'completed' ? 1.15 : 1}
    >
      <boxGeometry args={[CONTAINER_GRID.cellSize, 0.35, CONTAINER_GRID.cellSize]} />
      <meshStandardMaterial
        color={color}
        emissive={emissive}
        emissiveIntensity={0.3}
      />
    </mesh>
  );
}

export default function ContainerQueue({ state }: Props) {
  return (
    <group>
      {state.containers.map((c) => {
        const col = c.index % CONTAINER_GRID.cols;
        const row = Math.floor(c.index / CONTAINER_GRID.cols);
        return (
          <ContainerBox
            key={c.index}
            index={c.index}
            col={col}
            row={row}
            isHazmat={c.isHazmat}
            status={c.status}
          />
        );
      })}
    </group>
  );
}
