import { useRef, useMemo } from 'react';
import { Canvas, useFrame } from '@react-three/fiber';
import { OrbitControls, Text, Box } from '@react-three/drei';
import * as THREE from 'three';
import { TerrainType, terrainColors, terrainElevation, Piece, ComposedCapability, Objective } from '../../types';

interface TerrainCellProps {
  terrain: TerrainType;
  x: number;
  y: number;
}

function TerrainCell({ terrain, x, y }: TerrainCellProps) {
  const color = terrainColors[terrain];
  const elevation = terrainElevation[terrain];

  return (
    <Box
      position={[x, elevation * 0.5, y]}
      args={[0.95, 0.1 + elevation, 0.95]}
    >
      <meshStandardMaterial color={color} />
    </Box>
  );
}

interface PieceMarkerProps {
  piece: Piece;
  isSelected?: boolean;
}

function PieceMarker({ piece, isSelected }: PieceMarkerProps) {
  const meshRef = useRef<THREE.Mesh>(null);
  const color = piece.team === 'blue' ? '#00aaff' : '#ff4444';

  useFrame((state) => {
    if (meshRef.current && isSelected) {
      meshRef.current.position.y = 0.5 + Math.sin(state.clock.elapsedTime * 3) * 0.1;
    }
  });

  const symbol = useMemo(() => {
    switch (piece.pieceType.type) {
      case 'sensor': return 'S';
      case 'scout': return 'Rc';
      case 'striker': return 'St';
      case 'support': return 'Su';
      case 'authority': return 'Au';
      case 'analyst': return 'An';
      default: return '?';
    }
  }, [piece.pieceType]);

  return (
    <group position={[piece.x, 0.5, piece.y]}>
      <mesh ref={meshRef}>
        <cylinderGeometry args={[0.3, 0.3, 0.4, 8]} />
        <meshStandardMaterial color={color} />
      </mesh>
      <Text
        position={[0, 0.5, 0]}
        fontSize={0.2}
        color="white"
        anchorX="center"
        anchorY="middle"
      >
        {symbol}
      </Text>
    </group>
  );
}

interface CapabilityMarkerProps {
  capability: ComposedCapability;
  isSelected?: boolean;
}

function CapabilityMarker({ capability, isSelected }: CapabilityMarkerProps) {
  const meshRef = useRef<THREE.Mesh>(null);

  useFrame((state) => {
    if (meshRef.current) {
      meshRef.current.rotation.y = state.clock.elapsedTime;
      if (isSelected) {
        meshRef.current.scale.setScalar(1 + Math.sin(state.clock.elapsedTime * 5) * 0.1);
      }
    }
  });

  // Determine symbol based on capabilities
  const symbol = useMemo(() => {
    if (capability.strikeBonus >= 3 && capability.authorizeBonus >= 2) return '⚔';
    if (capability.fuseBonus >= 2 && capability.detectBonus >= 2) return '◆';
    if (capability.detectBonus >= 3 && capability.trackBonus >= 2) return '◎';
    if (capability.classifyBonus >= 2 || capability.predictBonus >= 2) return '◊';
    if (capability.reconBonus >= 2) return '◇';
    if (capability.relayBonus >= 2) return '◈';
    return '●';
  }, [capability]);

  return (
    <group position={[capability.centerX, 0.8, capability.centerY]}>
      <mesh ref={meshRef}>
        <octahedronGeometry args={[0.4]} />
        <meshStandardMaterial
          color="#00ffff"
          emissive="#004444"
          transparent
          opacity={0.8}
        />
      </mesh>
      <Text
        position={[0, 0.8, 0]}
        fontSize={0.25}
        color="cyan"
        anchorX="center"
        anchorY="middle"
      >
        {capability.name}
      </Text>
      <Text
        position={[0, 0.5, 0]}
        fontSize={0.4}
        color="white"
        anchorX="center"
        anchorY="middle"
      >
        {symbol}
      </Text>
    </group>
  );
}

interface ObjectiveMarkerProps {
  objective: Objective;
  isSelected?: boolean;
}

function ObjectiveMarker({ objective, isSelected }: ObjectiveMarkerProps) {
  const meshRef = useRef<THREE.Mesh>(null);

  useFrame((state) => {
    if (meshRef.current) {
      meshRef.current.position.y = 1 + Math.sin(state.clock.elapsedTime * 2) * 0.2;
      meshRef.current.rotation.y = state.clock.elapsedTime;
    }
  });

  const color = objective.completed
    ? '#00ff00'
    : objective.assignedCapability
    ? '#ffff00'
    : '#ff00ff';

  return (
    <group position={[objective.x, 0, objective.y]}>
      <mesh ref={meshRef}>
        <coneGeometry args={[0.3, 0.6, 4]} />
        <meshStandardMaterial color={color} emissive={color} emissiveIntensity={0.3} />
      </mesh>
      <Text
        position={[0, 1.8, 0]}
        fontSize={0.2}
        color={color}
        anchorX="center"
        anchorY="middle"
      >
        {objective.name}
      </Text>
    </group>
  );
}

interface Map3DSceneProps {
  terrain: TerrainType[][];
  pieces: Piece[];
  capabilities: ComposedCapability[];
  objectives: Objective[];
  showPieces: boolean;
  selectedCapability?: number;
  selectedObjective?: number;
}

function Map3DScene({
  terrain,
  pieces,
  capabilities,
  objectives,
  showPieces,
  selectedCapability,
  selectedObjective,
}: Map3DSceneProps) {
  const height = terrain.length;
  const width = terrain[0]?.length || 0;

  // Center the map
  const offsetX = -width / 2;
  const offsetY = -height / 2;

  return (
    <>
      <ambientLight intensity={0.4} />
      <directionalLight position={[10, 10, 5]} intensity={0.8} />
      <pointLight position={[-10, 10, -10]} intensity={0.4} />

      {/* Terrain grid */}
      <group position={[offsetX, 0, offsetY]}>
        {terrain.map((row, y) =>
          row.map((cell, x) => (
            <TerrainCell key={`${x}-${y}`} terrain={cell} x={x} y={y} />
          ))
        )}

        {/* Objectives */}
        {objectives.filter(o => !o.completed).map((obj) => (
          <ObjectiveMarker
            key={obj.id}
            objective={obj}
            isSelected={selectedObjective === obj.id}
          />
        ))}

        {/* Show either individual pieces or composed capabilities based on zoom */}
        {showPieces ? (
          pieces.filter(p => p.team === 'blue').map((piece) => (
            <PieceMarker key={piece.id} piece={piece} />
          ))
        ) : (
          capabilities.map((cap) => (
            <CapabilityMarker
              key={cap.id}
              capability={cap}
              isSelected={selectedCapability === cap.id}
            />
          ))
        )}

        {/* Enemy pieces (always show as "?" if visible) */}
        {pieces.filter(p => p.team === 'red').map((piece) => (
          <group key={piece.id} position={[piece.x, 0.5, piece.y]}>
            <mesh>
              <sphereGeometry args={[0.25]} />
              <meshStandardMaterial color="#ff4444" />
            </mesh>
            <Text
              position={[0, 0.5, 0]}
              fontSize={0.3}
              color="red"
              anchorX="center"
              anchorY="middle"
            >
              ?
            </Text>
          </group>
        ))}
      </group>

      <OrbitControls
        enablePan={true}
        enableZoom={true}
        enableRotate={true}
        minDistance={5}
        maxDistance={50}
        maxPolarAngle={Math.PI / 2.2}
      />
    </>
  );
}

interface Map3DProps {
  terrain: TerrainType[][];
  pieces: Piece[];
  capabilities: ComposedCapability[];
  objectives: Objective[];
  showPieces?: boolean;
  selectedCapability?: number;
  selectedObjective?: number;
}

export default function Map3D({
  terrain,
  pieces,
  capabilities,
  objectives,
  showPieces = false,
  selectedCapability,
  selectedObjective,
}: Map3DProps) {
  return (
    <Canvas
      camera={{ position: [0, 15, 15], fov: 60 }}
      style={{ background: '#0a0a14' }}
    >
      <Map3DScene
        terrain={terrain}
        pieces={pieces}
        capabilities={capabilities}
        objectives={objectives}
        showPieces={showPieces}
        selectedCapability={selectedCapability}
        selectedObjective={selectedObjective}
      />
    </Canvas>
  );
}
