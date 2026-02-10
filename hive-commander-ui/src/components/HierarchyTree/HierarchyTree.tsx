import { useMemo, useState } from 'react';
import { BerthTopology, BerthNode, HoldId, roleColors, roleLabels, levelColors } from '../../wire-types';

interface HierarchyTreeProps {
  topology: BerthTopology;
  selectedHold?: HoldId;
  onNodeClick?: (nodeId: string) => void;
  selectedNodeId?: string;
}

// Layout constants
const NODE_W = 48;
const NODE_H = 28;
const H_GAP = 8;
const V_GAP = 52;
const HOLD_GAP = 32;
const SHARED_GAP = 24;

interface LayoutNode {
  node: BerthNode;
  x: number;
  y: number;
}

interface LayoutEdge {
  fromX: number;
  fromY: number;
  toX: number;
  toY: number;
}

function layoutTopology(topology: BerthTopology) {
  const layoutNodes: LayoutNode[] = [];
  const layoutEdges: LayoutEdge[] = [];
  const nodePositions = new Map<string, { x: number; y: number }>();

  // Row Y positions (top to bottom: H4, H3, H2, H1, H0)
  const rowY = (level: number) => 20 + (4 - level) * V_GAP;

  // H4: Scheduler centered at top
  const totalWidth = 3 * 240 + 2 * HOLD_GAP + SHARED_GAP + 200;
  const centerX = totalWidth / 2;

  function placeNode(node: BerthNode, x: number, y: number) {
    layoutNodes.push({ node, x, y });
    nodePositions.set(node.id, { x, y });
  }

  function addEdge(fromId: string, toId: string) {
    const from = nodePositions.get(fromId);
    const to = nodePositions.get(toId);
    if (from && to) {
      layoutEdges.push({
        fromX: from.x + NODE_W / 2,
        fromY: from.y + NODE_H,
        toX: to.x + NODE_W / 2,
        toY: to.y,
      });
    }
  }

  // Place H4 Scheduler
  placeNode(topology.scheduler, centerX - NODE_W / 2, rowY(4));

  // Place H3 Berth Manager
  placeNode(topology.berthManager, centerX - NODE_W / 2, rowY(3));
  addEdge(topology.scheduler.id, topology.berthManager.id);

  // Place 3 holds side by side
  const holdWidth = 240;
  const holdsStartX = (totalWidth - (3 * holdWidth + 2 * HOLD_GAP + SHARED_GAP + 200)) / 2;

  topology.holds.forEach((hold, i) => {
    const holdBaseX = holdsStartX + i * (holdWidth + HOLD_GAP);

    // H2: Hold supervisor
    const supX = holdBaseX + holdWidth / 2 - NODE_W / 2;
    placeNode(hold.supervisor, supX, rowY(2));
    addEdge(topology.berthManager.id, hold.supervisor.id);

    // H1: Team leads
    const leads = [hold.cranes.lead, hold.stevedores.lead, hold.lashing.lead];
    const leadSpacing = holdWidth / (leads.length + 1);
    leads.forEach((lead, li) => {
      const lx = holdBaseX + leadSpacing * (li + 1) - NODE_W / 2;
      placeNode(lead, lx, rowY(1));
      addEdge(hold.supervisor.id, lead.id);
    });

    // H0: Workers
    const craneBaseX = holdBaseX;
    hold.cranes.operators.forEach((op, oi) => {
      const ox = craneBaseX + oi * (NODE_W + H_GAP);
      placeNode(op, ox, rowY(0));
      addEdge(hold.cranes.lead.id, op.id);
    });

    const steveBaseX = holdBaseX + 80;
    hold.stevedores.workers.forEach((w, wi) => {
      const wx = steveBaseX + wi * (NODE_W + H_GAP / 2);
      placeNode(w, wx, rowY(0));
      addEdge(hold.stevedores.lead.id, w.id);
    });

    const lashBaseX = holdBaseX + holdWidth - 2 * (NODE_W + H_GAP);
    hold.lashing.lashers.forEach((l, li) => {
      const lx = lashBaseX + li * (NODE_W + H_GAP);
      placeNode(l, lx, rowY(0));
      addEdge(hold.lashing.lead.id, l.id);
    });

    // Signaler reports directly to hold supervisor
    placeNode(hold.signaler, holdBaseX + holdWidth - NODE_W, rowY(0));
    addEdge(hold.supervisor.id, hold.signaler.id);
  });

  // Shared resources to the right
  const sharedBaseX = holdsStartX + 3 * (holdWidth + HOLD_GAP) + SHARED_GAP;

  // Tractor pool
  const tractorLeadX = sharedBaseX + 30;
  placeNode(topology.shared.tractors.lead, tractorLeadX, rowY(1));
  addEdge(topology.berthManager.id, topology.shared.tractors.lead.id);

  topology.shared.tractors.drivers.forEach((d, di) => {
    placeNode(d, sharedBaseX + di * (NODE_W + H_GAP / 2), rowY(0));
    addEdge(topology.shared.tractors.lead.id, d.id);
  });

  // Yard blocks
  const yardALeadX = sharedBaseX + 30;
  const yardBLeadX = sharedBaseX + 120;
  placeNode(topology.shared.yardBlockA.lead, yardALeadX, rowY(1) + V_GAP * 0.6);
  addEdge(topology.berthManager.id, topology.shared.yardBlockA.lead.id);

  topology.shared.yardBlockA.workers.forEach((w, wi) => {
    placeNode(w, sharedBaseX + wi * (NODE_W + H_GAP / 2), rowY(0) + V_GAP * 0.6);
    addEdge(topology.shared.yardBlockA.lead.id, w.id);
  });

  placeNode(topology.shared.yardBlockB.lead, yardBLeadX, rowY(1) + V_GAP * 1.2);
  addEdge(topology.berthManager.id, topology.shared.yardBlockB.lead.id);

  topology.shared.yardBlockB.workers.forEach((w, wi) => {
    placeNode(w, sharedBaseX + wi * (NODE_W + H_GAP / 2), rowY(0) + V_GAP * 1.2);
    addEdge(topology.shared.yardBlockB.lead.id, w.id);
  });

  return { layoutNodes, layoutEdges, totalWidth, totalHeight: rowY(0) + V_GAP * 1.2 + NODE_H + 40 };
}

export default function HierarchyTree({ topology, selectedHold, onNodeClick, selectedNodeId }: HierarchyTreeProps) {
  const [hoveredNode, setHoveredNode] = useState<string | null>(null);
  const layout = useMemo(() => layoutTopology(topology), [topology]);

  const filteredNodes = useMemo(() => {
    if (!selectedHold) return layout.layoutNodes;
    return layout.layoutNodes.filter(ln =>
      ln.node.holdId === selectedHold || ln.node.holdId === undefined
    );
  }, [layout.layoutNodes, selectedHold]);

  const visibleNodeIds = useMemo(() => new Set(filteredNodes.map(n => n.node.id)), [filteredNodes]);

  const filteredEdges = useMemo(() => {
    if (!selectedHold) return layout.layoutEdges;
    // Show edges where both endpoints are visible
    return layout.layoutEdges.filter((_e, i) => {
      const edgeData = topology.edges[i];
      if (!edgeData) return true;
      return visibleNodeIds.has(edgeData.from) && visibleNodeIds.has(edgeData.to);
    });
  }, [layout.layoutEdges, selectedHold, topology.edges, visibleNodeIds]);

  return (
    <div style={{ width: '100%', height: '100%', overflow: 'auto', background: '#0a0a14' }}>
      {/* Hold filter buttons */}
      <div style={{ padding: '8px 12px', display: 'flex', gap: '8px', borderBottom: '1px solid #333' }}>
        <span style={{ color: '#888', fontSize: '12px', lineHeight: '24px' }}>HIERARCHY</span>
        <span style={{ color: '#00ffff', fontSize: '12px', lineHeight: '24px', marginLeft: 'auto' }}>
          {topology.nodes.length} nodes
        </span>
      </div>

      <svg
        width={layout.totalWidth + 40}
        height={layout.totalHeight}
        style={{ display: 'block' }}
      >
        {/* Hold region backgrounds */}
        {topology.holds.map((hold, i) => {
          const holdNodes = layout.layoutNodes.filter(ln => ln.node.holdId === hold.holdId);
          if (holdNodes.length === 0) return null;
          const minX = Math.min(...holdNodes.map(n => n.x)) - 8;
          const maxX = Math.max(...holdNodes.map(n => n.x)) + NODE_W + 8;
          const minY = Math.min(...holdNodes.map(n => n.y)) - 8;
          const maxY = Math.max(...holdNodes.map(n => n.y)) + NODE_H + 8;
          const dimmed = selectedHold !== undefined && selectedHold !== hold.holdId;
          return (
            <g key={`hold-bg-${i}`}>
              <rect
                x={minX} y={minY}
                width={maxX - minX} height={maxY - minY}
                rx={6}
                fill={dimmed ? 'rgba(30,30,50,0.3)' : 'rgba(40,30,80,0.2)'}
                stroke={dimmed ? '#222' : '#443366'}
                strokeWidth={1}
                strokeDasharray="4,4"
              />
              <text
                x={minX + 4} y={minY + 14}
                fill={dimmed ? '#444' : '#886aaa'}
                fontSize={11}
                fontFamily="monospace"
              >
                Hold {hold.holdId}
              </text>
            </g>
          );
        })}

        {/* Shared region background */}
        {(() => {
          const sharedNodes = layout.layoutNodes.filter(ln => ln.node.holdId === undefined && ln.node.level <= 1);
          if (sharedNodes.length === 0) return null;
          const minX = Math.min(...sharedNodes.map(n => n.x)) - 8;
          const maxX = Math.max(...sharedNodes.map(n => n.x)) + NODE_W + 8;
          const minY = Math.min(...sharedNodes.map(n => n.y)) - 8;
          const maxY = Math.max(...sharedNodes.map(n => n.y)) + NODE_H + 8;
          return (
            <g>
              <rect
                x={minX} y={minY}
                width={maxX - minX} height={maxY - minY}
                rx={6}
                fill="rgba(30,50,40,0.2)"
                stroke="#336644"
                strokeWidth={1}
                strokeDasharray="4,4"
              />
              <text
                x={minX + 4} y={minY + 14}
                fill="#66aa88"
                fontSize={11}
                fontFamily="monospace"
              >
                Shared Pool
              </text>
            </g>
          );
        })()}

        {/* Edges */}
        {filteredEdges.map((edge, i) => (
          <line
            key={`edge-${i}`}
            x1={edge.fromX} y1={edge.fromY}
            x2={edge.toX} y2={edge.toY}
            stroke="#334"
            strokeWidth={1}
          />
        ))}

        {/* All edges (dimmed for context) */}
        {layout.layoutEdges.map((edge, i) => {
          if (filteredEdges.includes(edge)) return null;
          return (
            <line
              key={`edge-dim-${i}`}
              x1={edge.fromX} y1={edge.fromY}
              x2={edge.toX} y2={edge.toY}
              stroke="#1a1a2a"
              strokeWidth={0.5}
            />
          );
        })}

        {/* Nodes */}
        {layout.layoutNodes.map((ln) => {
          const dimmed = selectedHold !== undefined && ln.node.holdId !== undefined && ln.node.holdId !== selectedHold;
          const isHovered = hoveredNode === ln.node.id;
          const isSelected = selectedNodeId === ln.node.id;
          const color = roleColors[ln.node.role];
          const levelColor = levelColors[ln.node.level];

          return (
            <g
              key={ln.node.id}
              opacity={dimmed ? 0.25 : 1}
              style={{ cursor: 'pointer' }}
              onClick={() => onNodeClick?.(ln.node.id)}
              onMouseEnter={() => setHoveredNode(ln.node.id)}
              onMouseLeave={() => setHoveredNode(null)}
            >
              <rect
                x={ln.x} y={ln.y}
                width={NODE_W} height={NODE_H}
                rx={4}
                fill={isSelected ? '#2a2a4a' : isHovered ? '#1a1a3a' : '#111122'}
                stroke={isSelected ? '#00ffff' : isHovered ? '#555' : color}
                strokeWidth={isSelected ? 2 : 1}
              />
              {/* Level indicator bar */}
              <rect
                x={ln.x} y={ln.y}
                width={4} height={NODE_H}
                rx={2}
                fill={levelColor}
              />
              {/* Role abbreviation */}
              <text
                x={ln.x + NODE_W / 2 + 2}
                y={ln.y + 12}
                fill={color}
                fontSize={10}
                fontFamily="monospace"
                textAnchor="middle"
                fontWeight="bold"
              >
                {roleLabels[ln.node.role]}
              </text>
              {/* Status dot */}
              <circle
                cx={ln.x + NODE_W - 6}
                cy={ln.y + 6}
                r={3}
                fill={ln.node.status === 'active' ? '#44ff44' : ln.node.status === 'busy' ? '#ffaa00' : ln.node.status === 'idle' ? '#888' : '#ff4444'}
              />
              {/* Hold label (small) */}
              {ln.node.holdId && (
                <text
                  x={ln.x + NODE_W / 2 + 2}
                  y={ln.y + 23}
                  fill="#666"
                  fontSize={8}
                  fontFamily="monospace"
                  textAnchor="middle"
                >
                  H{ln.node.holdId}
                </text>
              )}
              {/* Tooltip on hover */}
              {isHovered && (
                <g>
                  <rect
                    x={ln.x + NODE_W + 4} y={ln.y - 4}
                    width={ln.node.label.length * 7 + 12} height={20}
                    rx={3}
                    fill="#222"
                    stroke="#444"
                    strokeWidth={0.5}
                  />
                  <text
                    x={ln.x + NODE_W + 10} y={ln.y + 10}
                    fill="#ccc"
                    fontSize={10}
                    fontFamily="monospace"
                  >
                    {ln.node.label}
                  </text>
                </g>
              )}
            </g>
          );
        })}

        {/* Level labels on left */}
        {[4, 3, 2, 1, 0].map(level => (
          <text
            key={`level-${level}`}
            x={4}
            y={20 + (4 - level) * V_GAP + NODE_H / 2 + 4}
            fill={levelColors[level as 0|1|2|3|4]}
            fontSize={10}
            fontFamily="monospace"
            opacity={0.6}
          >
            H{level}
          </text>
        ))}
      </svg>
    </div>
  );
}
