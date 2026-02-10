import { useRef, useMemo } from 'react';
import { Canvas, useFrame } from '@react-three/fiber';
import { OrbitControls, Text } from '@react-three/drei';
import * as THREE from 'three';
import { TerminalTopology, BerthId, BerthOperation, YardBlock, roleColors } from '../../wire-types';

// ─── Vessel at a berth ───
function Vessel({ berthId, offsetX }: { berthId: BerthId; offsetX: number }) {
  return (
    <group position={[offsetX, 0, 0]}>
      {/* Hull */}
      <mesh position={[0, -0.3, 0]}>
        <boxGeometry args={[24, 0.6, 5]} />
        <meshStandardMaterial color="#2a2a3a" />
      </mesh>
      {/* Deck markings per hold */}
      {[-8, 0, 8].map((x, i) => (
        <mesh key={`deck-${i}`} position={[x, -0.01, 0]}>
          <boxGeometry args={[7, 0.02, 4.5]} />
          <meshStandardMaterial color="#1a1a2a" />
        </mesh>
      ))}
      {/* Hold labels */}
      {[-8, 0, 8].map((x, i) => (
        <Text key={`label-${i}`} position={[x, 0.2, 0]} fontSize={0.35} color="#cc44ff" anchorX="center">
          {`B${berthId} H${i + 1}`}
        </Text>
      ))}
      {/* Berth label */}
      <Text position={[0, 0.6, -2.5]} fontSize={0.5} color={berthId === 1 ? '#cc44ff' : '#9944ff'} anchorX="center">
        {`Berth ${berthId}`}
      </Text>
    </group>
  );
}

// ─── Crane pair per hold per berth ───
function CranePair({ holdIndex, offsetX }: { holdIndex: number; offsetX: number }) {
  const refA = useRef<THREE.Mesh>(null);
  const refB = useRef<THREE.Mesh>(null);
  const baseX = offsetX + (holdIndex - 1) * 8;

  useFrame((state) => {
    const t = state.clock.elapsedTime + holdIndex + offsetX * 0.1;
    if (refA.current) refA.current.position.x = baseX - 1.5 + Math.sin(t * 0.8) * 0.5;
    if (refB.current) refB.current.position.x = baseX + 1.5 + Math.sin(t * 0.8 + 1) * 0.5;
  });

  return (
    <group>
      {/* Crane A tower + boom */}
      <mesh position={[baseX - 1.5, 2.5, -3]}>
        <boxGeometry args={[0.3, 5, 0.3]} />
        <meshStandardMaterial color="#00ccff" />
      </mesh>
      <mesh ref={refA} position={[baseX - 1.5, 5, -1.5]}>
        <boxGeometry args={[0.2, 0.2, 5]} />
        <meshStandardMaterial color="#0088cc" />
      </mesh>
      {/* Crane B tower + boom */}
      <mesh position={[baseX + 1.5, 2.5, -3]}>
        <boxGeometry args={[0.3, 5, 0.3]} />
        <meshStandardMaterial color="#00ccff" />
      </mesh>
      <mesh ref={refB} position={[baseX + 1.5, 5, -1.5]}>
        <boxGeometry args={[0.2, 0.2, 5]} />
        <meshStandardMaterial color="#0088cc" />
      </mesh>
    </group>
  );
}

// ─── Container stacks per hold ───
function ContainerQueue({ holdIndex, offsetX }: { holdIndex: number; offsetX: number }) {
  const baseX = offsetX + (holdIndex - 1) * 8;
  const containers = useMemo(() => {
    const result: { x: number; y: number; z: number; color: string }[] = [];
    const colors = ['#cc3333', '#3333cc', '#33cc33', '#cccc33', '#cc33cc'];
    for (let row = 0; row < 3; row++) {
      for (let col = 0; col < 2; col++) {
        const height = Math.floor(Math.random() * 3) + 1;
        for (let h = 0; h < height; h++) {
          result.push({
            x: baseX - 2 + col * 2.5 + row * 0.8,
            y: 0.25 + h * 0.5,
            z: 0,
            color: colors[Math.floor(Math.random() * colors.length)],
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

// ─── Worker dots on the dock ───
function WorkerDots({ berth, offsetX }: { berth: BerthOperation; offsetX: number }) {
  const workers = useMemo(() => {
    const dots: { x: number; z: number; color: string }[] = [];
    berth.holds.forEach((hold) => {
      const baseX = offsetX + (hold.holdId - 2) * 8;
      hold.stevedores.workers.forEach((w, i) => {
        dots.push({ x: baseX - 2 + i * 0.6, z: 1.5, color: roleColors[w.role] });
      });
      hold.lashing.lashers.forEach((l, i) => {
        dots.push({ x: baseX + 2 + i * 0.5, z: -0.5, color: roleColors[l.role] });
      });
      dots.push({ x: baseX + 3.5, z: 2, color: roleColors.signaler });
    });
    return dots;
  }, [berth, offsetX]);

  return (
    <group>
      {workers.map((w, i) => (
        <mesh key={i} position={[w.x, 0.3, w.z]}>
          <sphereGeometry args={[0.12]} />
          <meshStandardMaterial color={w.color} emissive={w.color} emissiveIntensity={0.3} />
        </mesh>
      ))}
    </group>
  );
}

// ─── Yard block grid ───
function YardBlockMesh({ block, position }: { block: YardBlock; position: [number, number, number] }) {
  const fillRatio = block.filled / block.capacity;
  const filledSlots = Math.round(block.rows * block.cols * fillRatio);

  return (
    <group position={position}>
      <mesh position={[0, -0.05, 0]}>
        <boxGeometry args={[block.cols * 1.1, 0.1, block.rows * 1.1]} />
        <meshStandardMaterial color="#1a2a1a" />
      </mesh>
      {Array.from({ length: block.rows * block.cols }, (_, i) => {
        const row = Math.floor(i / block.cols);
        const col = i % block.cols;
        const isFilled = i < filledSlots;
        return (
          <mesh
            key={i}
            position={[
              (col - block.cols / 2 + 0.5) * 1.0,
              isFilled ? 0.2 : 0.02,
              (row - block.rows / 2 + 0.5) * 1.0,
            ]}
          >
            <boxGeometry args={[0.9, isFilled ? 0.4 : 0.03, 0.9]} />
            <meshStandardMaterial color={isFilled ? '#44aa44' : '#222'} opacity={isFilled ? 0.7 : 0.3} transparent />
          </mesh>
        );
      })}
      <Text position={[0, 0.8, 0]} fontSize={0.35} color="#88aaff" anchorX="center">
        {block.name} ({Math.round(fillRatio * 100)}%)
      </Text>
    </group>
  );
}

// ─── Stacking cranes (RTGs) in yard ───
function StackingCranes({ yardBlockPositions }: { yardBlockPositions: [number, number, number][] }) {
  const ref = useRef<THREE.Group>(null);

  useFrame((state) => {
    if (!ref.current) return;
    ref.current.children.forEach((child, i) => {
      const mesh = child as THREE.Mesh;
      const t = state.clock.elapsedTime * 0.3 + i * 1.5;
      // Slide along x across yard block
      mesh.position.x = yardBlockPositions[i % yardBlockPositions.length][0] + Math.sin(t) * 3;
    });
  });

  return (
    <group>
      {/* Fixed rail tracks */}
      {yardBlockPositions.map((pos, i) => (
        <group key={`sc-frame-${i}`}>
          {/* Left rail */}
          <mesh position={[pos[0] - 4.5, 0.05, pos[2]]}>
            <boxGeometry args={[10, 0.1, 0.15]} />
            <meshStandardMaterial color="#44ddbb" />
          </mesh>
          {/* Right rail */}
          <mesh position={[pos[0] + 4.5, 0.05, pos[2]]}>
            <boxGeometry args={[10, 0.1, 0.15]} />
            <meshStandardMaterial color="#44ddbb" />
          </mesh>
        </group>
      ))}
      {/* Moving cranes */}
      <group ref={ref}>
        {yardBlockPositions.map((pos, i) => (
          <group key={`sc-${i}`}>
            {/* Portal frame */}
            <mesh position={[pos[0], 2, pos[2]]}>
              <boxGeometry args={[0.2, 4, 6]} />
              <meshStandardMaterial color="#44ddbb" opacity={0.7} transparent />
            </mesh>
            {/* Spreader (cross bar) */}
            <mesh position={[pos[0], 3.8, pos[2]]}>
              <boxGeometry args={[8, 0.15, 0.15]} />
              <meshStandardMaterial color="#33bb99" />
            </mesh>
          </group>
        ))}
      </group>
    </group>
  );
}

// ─── Gate lanes with truck queues ───
function GateLanes({ gateX }: { gateX: number }) {
  const truckRef = useRef<THREE.Group>(null);

  useFrame((state) => {
    if (!truckRef.current) return;
    truckRef.current.children.forEach((child, i) => {
      const mesh = child as THREE.Mesh;
      const t = (state.clock.elapsedTime * 0.4 + i * 1.2) % 5;
      mesh.position.z = gateX + 4 - t * 2;
    });
  });

  return (
    <group>
      {/* Gate structures */}
      {[0, 1].map((gIdx) => (
        <group key={`gate-${gIdx}`}>
          {/* Gate booth */}
          <mesh position={[-20 + gIdx * 8, 1, gateX]}>
            <boxGeometry args={[3, 2, 1]} />
            <meshStandardMaterial color="#dd8844" />
          </mesh>
          {/* Gate label */}
          <Text position={[-20 + gIdx * 8, 2.5, gateX]} fontSize={0.4} color="#ddaa66" anchorX="center">
            {`Gate ${gIdx === 0 ? 'A' : 'B'}`}
          </Text>
          {/* Lane markings */}
          {Array.from({ length: 6 }, (_, i) => (
            <mesh key={`lane-${gIdx}-${i}`} position={[-20 + gIdx * 8, -0.03, gateX + 2 + i * 1.5]} rotation={[-Math.PI / 2, 0, 0]}>
              <planeGeometry args={[2.5, 0.08]} />
              <meshStandardMaterial color="#555" />
            </mesh>
          ))}
          {/* Scanner indicators */}
          <mesh position={[-20 + gIdx * 8 - 2, 0.5, gateX + 0.5]}>
            <boxGeometry args={[0.3, 1, 0.3]} />
            <meshStandardMaterial color="#ccaa44" emissive="#ccaa44" emissiveIntensity={0.3} />
          </mesh>
          <mesh position={[-20 + gIdx * 8 + 2, 0.5, gateX + 0.5]}>
            <boxGeometry args={[0.3, 1, 0.3]} />
            <meshStandardMaterial color="#aacc44" emissive="#aacc44" emissiveIntensity={0.3} />
          </mesh>
        </group>
      ))}

      {/* Truck queues */}
      <group ref={truckRef}>
        {Array.from({ length: 8 }, (_, i) => (
          <mesh key={`truck-${i}`} position={[-20 + (i % 2) * 8, 0.3, gateX + 3 + Math.floor(i / 2) * 2]}>
            <boxGeometry args={[1.2, 0.6, 2]} />
            <meshStandardMaterial color="#aa8833" />
          </mesh>
        ))}
      </group>

      {/* Rail siding */}
      <group>
        <Text position={[0, 2, gateX + 12]} fontSize={0.4} color="#997733" anchorX="center">
          Rail Siding
        </Text>
        {/* Track */}
        <mesh position={[0, 0.02, gateX + 12]} rotation={[-Math.PI / 2, 0, 0]}>
          <planeGeometry args={[30, 0.3]} />
          <meshStandardMaterial color="#997733" />
        </mesh>
        {/* Rail cars */}
        {Array.from({ length: 5 }, (_, i) => (
          <mesh key={`car-${i}`} position={[-10 + i * 5, 0.3, gateX + 12]}>
            <boxGeometry args={[4, 0.6, 1.5]} />
            <meshStandardMaterial color="#775533" />
          </mesh>
        ))}
      </group>
    </group>
  );
}

// ─── Tractor routes across terminal ───
function TractorRoutes({ tractorCount, dockZ, yardZ }: { tractorCount: number; dockZ: number; yardZ: number }) {
  const ref = useRef<THREE.Group>(null);

  useFrame((state) => {
    if (!ref.current) return;
    ref.current.children.forEach((child, i) => {
      const mesh = child as THREE.Mesh;
      const t = (state.clock.elapsedTime * 0.5 + i * 1.3) % 6;
      // Move between dock and yard
      mesh.position.z = dockZ + t * ((yardZ - dockZ) / 6);
      mesh.position.x = -15 + (i % 6) * 6;
    });
  });

  return (
    <group ref={ref}>
      {Array.from({ length: tractorCount }, (_, i) => (
        <mesh key={i} position={[-15 + (i % 6) * 6, 0.15, dockZ + 3]}>
          <boxGeometry args={[0.5, 0.25, 0.8]} />
          <meshStandardMaterial color={roleColors.tractor_driver} />
        </mesh>
      ))}
    </group>
  );
}

// ─── Terminal scene content ───
function TerminalSceneContent({ topology }: { topology: TerminalTopology }) {
  // Layout: two berths side by side, yard behind, gate further back
  const berth1X = -15;
  const berth2X = 15;
  const dockZ = 5;
  const yardZ = 16;
  const gateZ = 30;

  const yardBlockPositions: [number, number, number][] = [
    [-12, 0, yardZ],
    [-4, 0, yardZ],
    [4, 0, yardZ],
    [12, 0, yardZ],
  ];

  return (
    <>
      <ambientLight intensity={0.3} />
      <directionalLight position={[15, 20, 10]} intensity={0.7} />
      <pointLight position={[-15, 15, -10]} intensity={0.3} />

      {/* Two vessels */}
      <Vessel berthId={1} offsetX={berth1X} />
      <Vessel berthId={2} offsetX={berth2X} />

      {/* Cranes per berth (3 holds each) */}
      {[0, 1, 2].map(i => <CranePair key={`b1-crane-${i}`} holdIndex={i} offsetX={berth1X} />)}
      {[0, 1, 2].map(i => <CranePair key={`b2-crane-${i}`} holdIndex={i} offsetX={berth2X} />)}

      {/* Container queues */}
      {[0, 1, 2].map(i => <ContainerQueue key={`b1-cq-${i}`} holdIndex={i} offsetX={berth1X} />)}
      {[0, 1, 2].map(i => <ContainerQueue key={`b2-cq-${i}`} holdIndex={i} offsetX={berth2X} />)}

      {/* Worker dots per berth */}
      <WorkerDots berth={topology.berths[0]} offsetX={berth1X} />
      <WorkerDots berth={topology.berths[1]} offsetX={berth2X} />

      {/* Dock surface */}
      <mesh position={[0, -0.1, dockZ]} rotation={[-Math.PI / 2, 0, 0]}>
        <planeGeometry args={[60, 10]} />
        <meshStandardMaterial color="#1a1a2a" />
      </mesh>

      {/* Main road between dock and yard */}
      <mesh position={[0, -0.05, (dockZ + yardZ) / 2]} rotation={[-Math.PI / 2, 0, 0]}>
        <planeGeometry args={[60, yardZ - dockZ - 2]} />
        <meshStandardMaterial color="#1c1c28" />
      </mesh>
      {/* Road markings */}
      {Array.from({ length: 16 }, (_, i) => (
        <mesh key={`mark-${i}`} position={[-20 + i * 2.8, -0.03, (dockZ + yardZ) / 2]} rotation={[-Math.PI / 2, 0, 0]}>
          <planeGeometry args={[1.2, 0.08]} />
          <meshStandardMaterial color="#333" />
        </mesh>
      ))}

      {/* Tractor routes */}
      <TractorRoutes tractorCount={topology.tractorPool.drivers.length} dockZ={dockZ} yardZ={gateZ} />

      {/* Yard blocks */}
      {topology.yardBlocks.map((block, i) => (
        <YardBlockMesh key={block.id} block={block} position={yardBlockPositions[i] || [i * 10 - 15, 0, yardZ]} />
      ))}

      {/* Stacking cranes */}
      <StackingCranes yardBlockPositions={yardBlockPositions} />

      {/* Road between yard and gate */}
      <mesh position={[0, -0.05, (yardZ + gateZ) / 2]} rotation={[-Math.PI / 2, 0, 0]}>
        <planeGeometry args={[60, gateZ - yardZ - 4]} />
        <meshStandardMaterial color="#1a1a25" />
      </mesh>

      {/* Gate zone */}
      <GateLanes gateX={gateZ} />

      {/* Ground plane */}
      <mesh position={[0, -0.15, 20]} rotation={[-Math.PI / 2, 0, 0]}>
        <planeGeometry args={[80, 60]} />
        <meshStandardMaterial color="#0c0c18" />
      </mesh>

      <OrbitControls
        enablePan={true}
        enableZoom={true}
        enableRotate={true}
        minDistance={8}
        maxDistance={80}
        maxPolarAngle={Math.PI / 2.2}
        target={[0, 0, 15]}
      />
    </>
  );
}

interface TerminalSceneProps {
  topology: TerminalTopology;
  selectedZone?: string;
}

export default function BerthScene({ topology }: TerminalSceneProps) {
  return (
    <Canvas
      camera={{ position: [0, 35, 50], fov: 50 }}
      style={{ background: '#060610' }}
    >
      <TerminalSceneContent topology={topology} />
    </Canvas>
  );
}
