import { useRef, useMemo } from 'react';
import { Canvas, useFrame } from '@react-three/fiber';
import { OrbitControls, Text } from '@react-three/drei';
import * as THREE from 'three';
import { BerthTopology, HoldId, YardBlock, roleColors } from '../../wire-types';

// Vessel body
function Vessel() {
  return (
    <group position={[0, 0, 0]}>
      {/* Hull */}
      <mesh position={[0, -0.3, 0]}>
        <boxGeometry args={[24, 0.6, 5]} />
        <meshStandardMaterial color="#2a2a3a" />
      </mesh>
      {/* Deck markings */}
      {[-8, 0, 8].map((x, i) => (
        <mesh key={`hold-deck-${i}`} position={[x, -0.01, 0]}>
          <boxGeometry args={[7, 0.02, 4.5]} />
          <meshStandardMaterial color="#1a1a2a" />
        </mesh>
      ))}
      {/* Hold labels */}
      {[-8, 0, 8].map((x, i) => (
        <Text
          key={`hold-label-${i}`}
          position={[x, 0.2, 0]}
          fontSize={0.4}
          color="#cc44ff"
          anchorX="center"
        >
          {`Hold ${i + 1}`}
        </Text>
      ))}
    </group>
  );
}

// Crane pair per hold
function CranePair({ holdIndex }: { holdIndex: number }) {
  const refA = useRef<THREE.Mesh>(null);
  const refB = useRef<THREE.Mesh>(null);
  const baseX = (holdIndex - 1) * 8;

  useFrame((state) => {
    const t = state.clock.elapsedTime + holdIndex;
    if (refA.current) {
      refA.current.position.x = baseX - 1.5 + Math.sin(t * 0.8) * 0.5;
    }
    if (refB.current) {
      refB.current.position.x = baseX + 1.5 + Math.sin(t * 0.8 + 1) * 0.5;
    }
  });

  return (
    <group>
      {/* Crane A */}
      <group>
        {/* Tower */}
        <mesh position={[baseX - 1.5, 2.5, -3]}>
          <boxGeometry args={[0.3, 5, 0.3]} />
          <meshStandardMaterial color="#00ccff" />
        </mesh>
        {/* Boom */}
        <mesh ref={refA} position={[baseX - 1.5, 5, -1.5]}>
          <boxGeometry args={[0.2, 0.2, 5]} />
          <meshStandardMaterial color="#0088cc" />
        </mesh>
      </group>
      {/* Crane B */}
      <group>
        <mesh position={[baseX + 1.5, 2.5, -3]}>
          <boxGeometry args={[0.3, 5, 0.3]} />
          <meshStandardMaterial color="#00ccff" />
        </mesh>
        <mesh ref={refB} position={[baseX + 1.5, 5, -1.5]}>
          <boxGeometry args={[0.2, 0.2, 5]} />
          <meshStandardMaterial color="#0088cc" />
        </mesh>
      </group>
    </group>
  );
}

// Container stacks per hold
function ContainerQueue({ holdIndex }: { holdIndex: number }) {
  const baseX = (holdIndex - 1) * 8;
  const containers = useMemo(() => {
    const result: { x: number; y: number; z: number; color: string }[] = [];
    for (let row = 0; row < 3; row++) {
      for (let col = 0; col < 2; col++) {
        const height = Math.floor(Math.random() * 3) + 1;
        for (let h = 0; h < height; h++) {
          result.push({
            x: baseX - 2 + col * 2.5 + row * 0.8,
            y: 0.25 + h * 0.5,
            z: 0,
            color: ['#cc3333', '#3333cc', '#33cc33', '#cccc33', '#cc33cc'][Math.floor(Math.random() * 5)],
          });
        }
      }
    }
    return result;
  }, [baseX]);

  return (
    <group>
      {containers.map((c, i) => (
        <mesh key={i} position={[c.x, c.y, c.z]}>
          <boxGeometry args={[2, 0.45, 0.8]} />
          <meshStandardMaterial color={c.color} opacity={0.8} transparent />
        </mesh>
      ))}
    </group>
  );
}

// Yard block grid showing fill level
function YardBlockMesh({ block, position }: { block: YardBlock; position: [number, number, number] }) {
  const fillRatio = block.filled / block.capacity;
  const filledSlots = Math.round(block.rows * block.cols * fillRatio);

  return (
    <group position={position}>
      {/* Ground pad */}
      <mesh position={[0, -0.05, 0]}>
        <boxGeometry args={[block.cols * 1.2, 0.1, block.rows * 1.2]} />
        <meshStandardMaterial color="#1a2a1a" />
      </mesh>
      {/* Grid slots */}
      {Array.from({ length: block.rows * block.cols }, (_, i) => {
        const row = Math.floor(i / block.cols);
        const col = i % block.cols;
        const isFilled = i < filledSlots;
        return (
          <mesh
            key={i}
            position={[
              (col - block.cols / 2 + 0.5) * 1.1,
              isFilled ? 0.25 : 0.02,
              (row - block.rows / 2 + 0.5) * 1.1,
            ]}
          >
            <boxGeometry args={[1, isFilled ? 0.5 : 0.04, 1]} />
            <meshStandardMaterial
              color={isFilled ? '#44aa44' : '#222'}
              opacity={isFilled ? 0.7 : 0.3}
              transparent
            />
          </mesh>
        );
      })}
      {/* Label */}
      <Text
        position={[0, 1, 0]}
        fontSize={0.4}
        color="#88aaff"
        anchorX="center"
      >
        {block.name} ({Math.round(fillRatio * 100)}%)
      </Text>
    </group>
  );
}

// Tractor routes between holds and yard
function TractorRoutes() {
  const ref = useRef<THREE.Group>(null);

  useFrame((state) => {
    if (ref.current) {
      ref.current.children.forEach((child, i) => {
        const t = (state.clock.elapsedTime * 0.5 + i * 2) % 4;
        const mesh = child as THREE.Mesh;
        // Move along Z axis (dock to yard)
        mesh.position.z = 3 + t * 2.5;
        mesh.position.x = -8 + i * 4;
      });
    }
  });

  return (
    <group ref={ref}>
      {[0, 1, 2, 3, 4].map((i) => (
        <mesh key={i} position={[-8 + i * 4, 0.15, 5]}>
          <boxGeometry args={[0.6, 0.3, 1]} />
          <meshStandardMaterial color={roleColors.tractor_driver} />
        </mesh>
      ))}
    </group>
  );
}

// Worker dots on the dock
function WorkerDots({ topology }: { topology: BerthTopology }) {
  const workers = useMemo(() => {
    const dots: { x: number; z: number; color: string }[] = [];
    topology.holds.forEach((hold) => {
      const baseX = (hold.holdId - 2) * 8;
      // Stevedores near containers
      hold.stevedores.workers.forEach((w, i) => {
        dots.push({ x: baseX - 1 + i * 0.8, z: 1.5, color: roleColors[w.role] });
      });
      // Lashers on vessel
      hold.lashing.lashers.forEach((l, i) => {
        dots.push({ x: baseX + 2 + i * 0.6, z: -0.5, color: roleColors[l.role] });
      });
      // Signaler
      dots.push({ x: baseX + 3, z: 2, color: roleColors.signaler });
    });
    return dots;
  }, [topology]);

  return (
    <group>
      {workers.map((w, i) => (
        <mesh key={i} position={[w.x, 0.3, w.z]}>
          <sphereGeometry args={[0.15]} />
          <meshStandardMaterial color={w.color} emissive={w.color} emissiveIntensity={0.3} />
        </mesh>
      ))}
    </group>
  );
}

function BerthSceneContent({ topology }: { topology: BerthTopology }) {
  return (
    <>
      <ambientLight intensity={0.3} />
      <directionalLight position={[10, 15, 5]} intensity={0.7} />
      <pointLight position={[-10, 10, -10]} intensity={0.3} />

      <Vessel />

      {/* 3 crane pairs */}
      {[0, 1, 2].map(i => <CranePair key={i} holdIndex={i} />)}

      {/* Container queues per hold */}
      {[0, 1, 2].map(i => <ContainerQueue key={`cq-${i}`} holdIndex={i} />)}

      {/* Tractor routes */}
      <TractorRoutes />

      {/* Worker dots */}
      <WorkerDots topology={topology} />

      {/* Dock surface */}
      <mesh position={[0, -0.1, 5]} rotation={[-Math.PI / 2, 0, 0]}>
        <planeGeometry args={[30, 10]} />
        <meshStandardMaterial color="#1a1a2a" />
      </mesh>

      {/* Yard blocks */}
      {topology.yardBlocks.map((block, i) => (
        <YardBlockMesh
          key={block.id}
          block={block}
          position={[(i - 0.5) * 10, 0, 14]}
        />
      ))}

      {/* Road between dock and yard */}
      <mesh position={[0, -0.05, 10]} rotation={[-Math.PI / 2, 0, 0]}>
        <planeGeometry args={[30, 4]} />
        <meshStandardMaterial color="#222233" />
      </mesh>
      {/* Road markings */}
      {Array.from({ length: 10 }, (_, i) => (
        <mesh key={`mark-${i}`} position={[-12 + i * 2.8, -0.03, 10]} rotation={[-Math.PI / 2, 0, 0]}>
          <planeGeometry args={[1.2, 0.1]} />
          <meshStandardMaterial color="#444" />
        </mesh>
      ))}

      <OrbitControls
        enablePan={true}
        enableZoom={true}
        enableRotate={true}
        minDistance={5}
        maxDistance={60}
        maxPolarAngle={Math.PI / 2.2}
      />
    </>
  );
}

interface BerthSceneProps {
  topology: BerthTopology;
  selectedHold?: HoldId;
}

export default function BerthScene({ topology }: BerthSceneProps) {
  return (
    <Canvas
      camera={{ position: [0, 20, 25], fov: 50 }}
      style={{ background: '#060610' }}
    >
      <BerthSceneContent topology={topology} />
    </Canvas>
  );
}
