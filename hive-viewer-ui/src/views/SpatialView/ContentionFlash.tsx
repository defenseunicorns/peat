import { useRef } from 'react';
import { useFrame } from '@react-three/fiber';
import type { Mesh, MeshStandardMaterial } from 'three';
import { CRANE_POSITIONS, COLORS } from '../../spatial/constants';

interface Props {
  active: boolean;
}

export default function ContentionFlash({ active }: Props) {
  const meshRef = useRef<Mesh>(null);
  const fadeRef = useRef(0);

  const c1 = CRANE_POSITIONS['crane-1'];
  const c2 = CRANE_POSITIONS['crane-2'];
  if (!c1 || !c2) return null;

  const midX = (c1.x + c2.x) / 2;
  const midZ = (c1.z + c2.z) / 2;
  const width = Math.abs(c2.x - c1.x);

  useFrame((_, delta) => {
    if (!meshRef.current) return;
    if (active) fadeRef.current = 1;
    else fadeRef.current = Math.max(0, fadeRef.current - delta);

    meshRef.current.visible = fadeRef.current > 0.01;
    const mat = meshRef.current.material as MeshStandardMaterial;
    mat.opacity = fadeRef.current * 0.5;
  });

  return (
    <mesh ref={meshRef} position={[midX, 3, midZ]} rotation={[-Math.PI / 2, 0, 0]} visible={false}>
      <planeGeometry args={[width + 2, 2]} />
      <meshStandardMaterial
        color={COLORS.craneContention}
        transparent
        opacity={0}
        emissive={COLORS.craneContention}
        emissiveIntensity={0.5}
      />
    </mesh>
  );
}
