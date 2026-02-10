import { useMemo, useState, useCallback } from 'react';
import {
  TerminalTopology, TerminalNode, BerthId, HoldId, ZoneId,
  roleColors, roleLabels, levelColors, zoneColors,
} from '../../wire-types';

interface HierarchyTreeProps {
  topology: TerminalTopology;
  selectedZone?: ZoneId;
  selectedBerth?: BerthId;
  selectedHold?: HoldId;
  onNodeClick?: (nodeId: string) => void;
  selectedNodeId?: string;
}

// Layout constants — tighter for 200 nodes
const NODE_W = 40;
const NODE_H = 22;
const H_GAP = 4;
const V_GAP = 48;
const ZONE_GAP = 28;

interface LayoutNode {
  node: TerminalNode;
  x: number;
  y: number;
}

interface LayoutEdge {
  fromId: string;
  toId: string;
  fromX: number;
  fromY: number;
  toX: number;
  toY: number;
}

function layoutTerminal(topology: TerminalTopology) {
  const layoutNodes: LayoutNode[] = [];
  const layoutEdges: LayoutEdge[] = [];
  const nodePositions = new Map<string, { x: number; y: number }>();

  const rowY = (level: number) => 24 + (4 - level) * V_GAP;

  function placeNode(node: TerminalNode, x: number, y: number) {
    layoutNodes.push({ node, x, y });
    nodePositions.set(node.id, { x, y });
  }

  function addEdge(fromId: string, toId: string) {
    const from = nodePositions.get(fromId);
    const to = nodePositions.get(toId);
    if (from && to) {
      layoutEdges.push({
        fromId,
        toId,
        fromX: from.x + NODE_W / 2,
        fromY: from.y + NODE_H,
        toX: to.x + NODE_W / 2,
        toY: to.y,
      });
    }
  }

  // Calculate zone widths
  function holdWidth(hold: { cranes: { operators: TerminalNode[] }; stevedores: { workers: TerminalNode[] }; lashing: { lashers: TerminalNode[] } }) {
    const workerCount = hold.cranes.operators.length + hold.stevedores.workers.length + hold.lashing.lashers.length + 1; // +1 signaler
    return Math.max(workerCount * (NODE_W + H_GAP), 3 * (NODE_W + H_GAP) + NODE_W);
  }

  function berthWidth(berth: typeof topology.berths[0]) {
    return berth.holds.reduce((sum, h) => sum + holdWidth(h) + 12, 0);
  }

  // Yard zone width
  const yardW = (topology.yard.stackingCranes.length * 2 + topology.yard.blocks.length * 5 + topology.yard.blocks.length) * (NODE_W + H_GAP);

  // Gate zone width
  const gateW = (topology.gate.gates.length * 8 + topology.gate.rail.operators.length + 2) * (NODE_W + H_GAP);

  // Tractor pool width
  const tractorW = (topology.tractorPool.drivers.length + 1) * (NODE_W + H_GAP);

  const berth1W = berthWidth(topology.berths[0]);
  const berth2W = berthWidth(topology.berths[1]);

  const totalWidth = berth1W + ZONE_GAP + berth2W + ZONE_GAP + Math.max(yardW, gateW, tractorW) + 40;
  const centerX = totalWidth / 2;

  // ─── TOC at top center ───
  placeNode(topology.toc, centerX - NODE_W / 2, rowY(4));

  // ─── Berths ───
  let curX = 20;
  topology.berths.forEach((berth) => {
    const bw = berthWidth(berth);
    const mgrX = curX + bw / 2 - NODE_W / 2;
    placeNode(berth.manager, mgrX, rowY(3));
    addEdge(topology.toc.id, berth.manager.id);

    let holdX = curX;
    berth.holds.forEach((hold) => {
      const hw = holdWidth(hold);

      // H2: supervisor
      const supX = holdX + hw / 2 - NODE_W / 2;
      placeNode(hold.supervisor, supX, rowY(2));
      addEdge(berth.manager.id, hold.supervisor.id);

      // H1: leads
      const leads = [hold.cranes.lead, hold.stevedores.lead, hold.lashing.lead];
      const leadSpacing = hw / (leads.length + 1);
      leads.forEach((lead, li) => {
        const lx = holdX + leadSpacing * (li + 1) - NODE_W / 2;
        placeNode(lead, lx, rowY(1));
        addEdge(hold.supervisor.id, lead.id);
      });

      // H0: workers in sub-groups
      let wx = holdX;
      hold.cranes.operators.forEach((op) => {
        placeNode(op, wx, rowY(0));
        addEdge(hold.cranes.lead.id, op.id);
        wx += NODE_W + H_GAP;
      });
      hold.stevedores.workers.forEach((w) => {
        placeNode(w, wx, rowY(0));
        addEdge(hold.stevedores.lead.id, w.id);
        wx += NODE_W + H_GAP;
      });
      hold.lashing.lashers.forEach((l) => {
        placeNode(l, wx, rowY(0));
        addEdge(hold.lashing.lead.id, l.id);
        wx += NODE_W + H_GAP;
      });
      // Signaler
      placeNode(hold.signaler, wx, rowY(0));
      addEdge(hold.supervisor.id, hold.signaler.id);

      holdX += hw + 12;
    });

    curX += bw + ZONE_GAP;
  });

  // ─── Right-side zones: Yard, Gate, Tractor ───
  const rightBaseX = curX;

  // Yard Manager
  const yardMgrX = rightBaseX + Math.max(yardW, gateW, tractorW) / 2 - NODE_W / 2;
  placeNode(topology.yard.manager, yardMgrX, rowY(3));
  addEdge(topology.toc.id, topology.yard.manager.id);

  // SC Supervisor
  let yardX = rightBaseX;
  placeNode(topology.yard.scSupervisor, yardX + NODE_W, rowY(2));
  addEdge(topology.yard.manager.id, topology.yard.scSupervisor.id);

  topology.yard.stackingCranes.forEach((sc) => {
    placeNode(sc.lead, yardX, rowY(1));
    addEdge(topology.yard.scSupervisor.id, sc.lead.id);
    sc.operators.forEach((op) => {
      placeNode(op, yardX, rowY(0));
      addEdge(sc.lead.id, op.id);
      yardX += NODE_W + H_GAP;
    });
    yardX += H_GAP;
  });

  // Yard blocks
  topology.yard.blocks.forEach((block) => {
    placeNode(block.supervisor, yardX + (block.workers.length * (NODE_W + H_GAP)) / 2 - NODE_W / 2, rowY(2));
    addEdge(topology.yard.manager.id, block.supervisor.id);
    placeNode(block.lead, yardX + (block.workers.length * (NODE_W + H_GAP)) / 2 - NODE_W / 2, rowY(1));
    addEdge(block.supervisor.id, block.lead.id);
    block.workers.forEach((w) => {
      placeNode(w, yardX, rowY(0));
      addEdge(block.lead.id, w.id);
      yardX += NODE_W + H_GAP;
    });
    yardX += H_GAP;
  });

  // Gate Manager — below yard (offset rows)
  const gateRowOffset = V_GAP * 0.7;
  const gateMgrX = rightBaseX + Math.max(yardW, gateW, tractorW) / 2 + NODE_W;
  placeNode(topology.gate.manager, gateMgrX, rowY(3) + gateRowOffset);
  addEdge(topology.toc.id, topology.gate.manager.id);

  let gateX = rightBaseX;
  topology.gate.gates.forEach((g) => {
    placeNode(g.supervisor, gateX + 3 * (NODE_W + H_GAP), rowY(2) + gateRowOffset);
    addEdge(topology.gate.manager.id, g.supervisor.id);
    placeNode(g.lead, gateX + 3 * (NODE_W + H_GAP), rowY(1) + gateRowOffset);
    addEdge(g.supervisor.id, g.lead.id);

    const gateWorkers = [...g.scanners, ...g.rfidReaders, ...g.workers];
    gateWorkers.forEach((w) => {
      placeNode(w, gateX, rowY(0) + gateRowOffset);
      addEdge(g.lead.id, w.id);
      gateX += NODE_W + H_GAP;
    });
    gateX += H_GAP * 2;
  });

  // Rail
  placeNode(topology.gate.rail.supervisor, gateX + (NODE_W + H_GAP), rowY(2) + gateRowOffset);
  addEdge(topology.gate.manager.id, topology.gate.rail.supervisor.id);
  placeNode(topology.gate.rail.lead, gateX + (NODE_W + H_GAP), rowY(1) + gateRowOffset);
  addEdge(topology.gate.rail.supervisor.id, topology.gate.rail.lead.id);
  topology.gate.rail.operators.forEach((op) => {
    placeNode(op, gateX, rowY(0) + gateRowOffset);
    addEdge(topology.gate.rail.lead.id, op.id);
    gateX += NODE_W + H_GAP;
  });

  // Tractor Pool — directly under TOC
  const tractorBaseX = centerX - (tractorW / 2);
  placeNode(topology.tractorPool.lead, centerX - NODE_W / 2, rowY(1) - V_GAP * 0.3);
  addEdge(topology.toc.id, topology.tractorPool.lead.id);

  topology.tractorPool.drivers.forEach((d, di) => {
    placeNode(d, tractorBaseX + di * (NODE_W + H_GAP), rowY(0) - V_GAP * 0.3);
    addEdge(topology.tractorPool.lead.id, d.id);
  });

  const maxX = Math.max(...layoutNodes.map(n => n.x)) + NODE_W + 40;
  const maxY = Math.max(...layoutNodes.map(n => n.y)) + NODE_H + 40;

  return { layoutNodes, layoutEdges, totalWidth: Math.max(totalWidth, maxX), totalHeight: maxY };
}

// Zone region bounding boxes for background rendering
function computeZoneRegions(layoutNodes: LayoutNode[]): { zoneId: ZoneId; minX: number; maxX: number; minY: number; maxY: number; label: string }[] {
  const zoneMap = new Map<ZoneId, LayoutNode[]>();
  layoutNodes.forEach((ln) => {
    if (ln.node.zoneId) {
      if (!zoneMap.has(ln.node.zoneId)) zoneMap.set(ln.node.zoneId, []);
      zoneMap.get(ln.node.zoneId)!.push(ln);
    }
  });

  const labels: Record<ZoneId, string> = {
    berth1: 'Berth 1',
    berth2: 'Berth 2',
    yard: 'Yard',
    gate: 'Gate',
    tractor: 'Tractor Pool',
  };

  const regions: { zoneId: ZoneId; minX: number; maxX: number; minY: number; maxY: number; label: string }[] = [];
  zoneMap.forEach((nodes, zoneId) => {
    regions.push({
      zoneId,
      minX: Math.min(...nodes.map(n => n.x)) - 10,
      maxX: Math.max(...nodes.map(n => n.x)) + NODE_W + 10,
      minY: Math.min(...nodes.map(n => n.y)) - 10,
      maxY: Math.max(...nodes.map(n => n.y)) + NODE_H + 10,
      label: labels[zoneId],
    });
  });

  return regions;
}

export default function HierarchyTree({ topology, selectedZone, selectedBerth, selectedHold, onNodeClick, selectedNodeId }: HierarchyTreeProps) {
  const [hoveredNode, setHoveredNode] = useState<string | null>(null);
  const layout = useMemo(() => layoutTerminal(topology), [topology]);
  const zoneRegions = useMemo(() => computeZoneRegions(layout.layoutNodes), [layout.layoutNodes]);

  const isNodeVisible = useCallback((node: TerminalNode) => {
    if (selectedZone && node.zoneId && node.zoneId !== selectedZone) return false;
    if (selectedBerth && node.berthId && node.berthId !== selectedBerth) return false;
    if (selectedHold && node.holdId && node.holdId !== selectedHold) return false;
    return true;
  }, [selectedZone, selectedBerth, selectedHold]);

  const visibleNodeIds = useMemo(() => {
    const set = new Set<string>();
    layout.layoutNodes.forEach((ln) => {
      if (isNodeVisible(ln.node)) set.add(ln.node.id);
    });
    // Always show TOC + zone managers for context
    set.add(topology.toc.id);
    topology.berths.forEach(b => set.add(b.manager.id));
    set.add(topology.yard.manager.id);
    set.add(topology.gate.manager.id);
    set.add(topology.tractorPool.lead.id);
    return set;
  }, [layout.layoutNodes, isNodeVisible, topology]);

  return (
    <div style={{ width: '100%', height: '100%', overflow: 'auto', background: '#0a0a14' }}>
      {/* Header */}
      <div style={{ padding: '8px 12px', display: 'flex', gap: '8px', borderBottom: '1px solid #333', flexShrink: 0 }}>
        <span style={{ color: '#888', fontSize: '12px', lineHeight: '24px' }}>TERMINAL HIERARCHY</span>
        <span style={{ color: '#00ffff', fontSize: '12px', lineHeight: '24px', marginLeft: 'auto' }}>
          {topology.nodes.length} nodes
        </span>
      </div>

      <svg
        width={layout.totalWidth}
        height={layout.totalHeight}
        style={{ display: 'block' }}
      >
        {/* Zone region backgrounds */}
        {zoneRegions.map((region) => {
          const dimmed = selectedZone !== undefined && region.zoneId !== selectedZone;
          const color = zoneColors[region.zoneId];
          return (
            <g key={`zone-${region.zoneId}`}>
              <rect
                x={region.minX} y={region.minY}
                width={region.maxX - region.minX} height={region.maxY - region.minY}
                rx={6}
                fill={dimmed ? 'rgba(20,20,30,0.2)' : `${color}08`}
                stroke={dimmed ? '#1a1a2a' : `${color}44`}
                strokeWidth={1}
                strokeDasharray="4,4"
              />
              <text
                x={region.minX + 6} y={region.minY + 14}
                fill={dimmed ? '#333' : `${color}aa`}
                fontSize={10}
                fontFamily="monospace"
              >
                {region.label}
              </text>
            </g>
          );
        })}

        {/* Edges — dim non-visible */}
        {layout.layoutEdges.map((edge, i) => {
          const bothVisible = visibleNodeIds.has(edge.fromId) && visibleNodeIds.has(edge.toId);
          return (
            <line
              key={`edge-${i}`}
              x1={edge.fromX} y1={edge.fromY}
              x2={edge.toX} y2={edge.toY}
              stroke={bothVisible ? '#334' : '#151520'}
              strokeWidth={bothVisible ? 1 : 0.5}
            />
          );
        })}

        {/* Nodes */}
        {layout.layoutNodes.map((ln) => {
          const visible = visibleNodeIds.has(ln.node.id);
          const isHovered = hoveredNode === ln.node.id;
          const isSelected = selectedNodeId === ln.node.id;
          const color = roleColors[ln.node.role];
          const levelColor = levelColors[ln.node.level];

          return (
            <g
              key={ln.node.id}
              opacity={visible ? 1 : 0.15}
              style={{ cursor: 'pointer' }}
              onClick={() => onNodeClick?.(ln.node.id)}
              onMouseEnter={() => setHoveredNode(ln.node.id)}
              onMouseLeave={() => setHoveredNode(null)}
            >
              <rect
                x={ln.x} y={ln.y}
                width={NODE_W} height={NODE_H}
                rx={3}
                fill={isSelected ? '#2a2a4a' : isHovered ? '#1a1a3a' : '#111122'}
                stroke={isSelected ? '#00ffff' : isHovered ? '#555' : color}
                strokeWidth={isSelected ? 2 : 1}
              />
              {/* Level indicator bar */}
              <rect
                x={ln.x} y={ln.y}
                width={3} height={NODE_H}
                rx={1.5}
                fill={levelColor}
              />
              {/* Role abbreviation */}
              <text
                x={ln.x + NODE_W / 2 + 1}
                y={ln.y + 10}
                fill={color}
                fontSize={9}
                fontFamily="monospace"
                textAnchor="middle"
                fontWeight="bold"
              >
                {roleLabels[ln.node.role]}
              </text>
              {/* Status dot */}
              <circle
                cx={ln.x + NODE_W - 5}
                cy={ln.y + 5}
                r={2.5}
                fill={ln.node.status === 'active' ? '#44ff44' : ln.node.status === 'busy' ? '#ffaa00' : ln.node.status === 'idle' ? '#888' : '#ff4444'}
              />
              {/* Hold/berth label (tiny) */}
              {ln.node.berthId && (
                <text
                  x={ln.x + NODE_W / 2 + 1}
                  y={ln.y + 19}
                  fill="#555"
                  fontSize={7}
                  fontFamily="monospace"
                  textAnchor="middle"
                >
                  B{ln.node.berthId}{ln.node.holdId ? `H${ln.node.holdId}` : ''}
                </text>
              )}
              {/* Tooltip on hover */}
              {isHovered && (
                <g>
                  <rect
                    x={ln.x + NODE_W + 4} y={ln.y - 4}
                    width={ln.node.label.length * 6.5 + 12} height={18}
                    rx={3}
                    fill="#222"
                    stroke="#444"
                    strokeWidth={0.5}
                  />
                  <text
                    x={ln.x + NODE_W + 10} y={ln.y + 9}
                    fill="#ccc"
                    fontSize={9}
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
            y={24 + (4 - level) * V_GAP + NODE_H / 2 + 3}
            fill={levelColors[level as 0|1|2|3|4]}
            fontSize={9}
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
