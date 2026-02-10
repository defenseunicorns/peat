// Phase 3 full terminal wire types (200 nodes)
// Terminal hierarchy: H4 (TOC) -> H3 (Zone Managers) -> H2 (Supervisors) -> H1 (Team Leads) -> H0 (Workers)

export type HierarchyLevel = 0 | 1 | 2 | 3 | 4;

export type TerminalRole =
  // H4 - Terminal Operations Center
  | 'toc'
  // H3 - Zone managers
  | 'berth_manager'
  | 'yard_manager'
  | 'gate_manager'
  // H2 - Supervisors
  | 'hold_supervisor'
  | 'sc_supervisor'        // Stacking crane zone supervisor
  | 'yard_block_supervisor'
  | 'gate_supervisor'
  | 'rail_supervisor'
  // H1 - Team leads
  | 'crane_lead'
  | 'stevedore_lead'
  | 'lashing_lead'
  | 'tractor_lead'
  | 'yard_lead'
  | 'stacking_crane_lead'
  | 'gate_lead'
  | 'rail_lead'
  // H0 - Workers / equipment
  | 'crane_operator'
  | 'stevedore'
  | 'lasher'
  | 'signaler'
  | 'tractor_driver'
  | 'yard_worker'
  | 'stacking_crane_op'
  | 'gate_scanner'
  | 'rfid_reader'
  | 'gate_worker'
  | 'rail_operator';

// Keep backward compat alias
export type BerthRole = TerminalRole;

export type BerthId = 1 | 2;
export type HoldId = 1 | 2 | 3;
export type ZoneId = 'berth1' | 'berth2' | 'yard' | 'gate' | 'tractor';

export interface TerminalNode {
  id: string;
  role: TerminalRole;
  level: HierarchyLevel;
  berthId?: BerthId;
  holdId?: HoldId;
  zoneId?: ZoneId;
  parentId?: string;
  label: string;
  status: 'active' | 'idle' | 'busy' | 'offline';
}

// Backward compat
export type BerthNode = TerminalNode;

export interface TerminalEdge {
  from: string;
  to: string;
}

export type BerthEdge = TerminalEdge;

export interface HoldTeam {
  holdId: HoldId;
  supervisor: TerminalNode;
  cranes: { lead: TerminalNode; operators: TerminalNode[] };
  stevedores: { lead: TerminalNode; workers: TerminalNode[] };
  lashing: { lead: TerminalNode; lashers: TerminalNode[] };
  signaler: TerminalNode;
}

export interface BerthOperation {
  berthId: BerthId;
  manager: TerminalNode;
  holds: HoldTeam[];
}

export interface YardZone {
  manager: TerminalNode;
  scSupervisor: TerminalNode;
  stackingCranes: { lead: TerminalNode; operators: TerminalNode[] }[];
  blocks: { supervisor: TerminalNode; lead: TerminalNode; workers: TerminalNode[] }[];
}

export interface GateZone {
  manager: TerminalNode;
  gates: { supervisor: TerminalNode; lead: TerminalNode; scanners: TerminalNode[]; rfidReaders: TerminalNode[]; workers: TerminalNode[] }[];
  rail: { supervisor: TerminalNode; lead: TerminalNode; operators: TerminalNode[] };
}

export interface TractorPool {
  lead: TerminalNode;
  drivers: TerminalNode[];
}

export interface YardBlock {
  id: string;
  name: string;
  capacity: number;
  filled: number;
  rows: number;
  cols: number;
}

export interface TerminalEvent {
  id: number;
  timestamp: number;
  berthId?: BerthId;
  holdId?: HoldId;
  zoneId?: ZoneId;
  source: string;
  type: 'container_move' | 'crane_cycle' | 'tractor_dispatch' | 'lashing_complete' | 'yard_store' | 'status_change' | 'gate_scan' | 'rail_load' | 'stacking_crane_cycle';
  message: string;
}

export type BerthEvent = TerminalEvent;

export interface TerminalTopology {
  toc: TerminalNode;
  berths: BerthOperation[];
  yard: YardZone;
  gate: GateZone;
  tractorPool: TractorPool;
  yardBlocks: YardBlock[];
  nodes: TerminalNode[];
  edges: TerminalEdge[];
  events: TerminalEvent[];
}

// Backward compat - old code can still import this
export type BerthTopology = TerminalTopology;

// Role display properties
export const roleColors: Record<TerminalRole, string> = {
  toc: '#ff9900',
  berth_manager: '#ff6600',
  yard_manager: '#44cc88',
  gate_manager: '#dd8844',
  hold_supervisor: '#cc44ff',
  sc_supervisor: '#44ccaa',
  yard_block_supervisor: '#66aa88',
  gate_supervisor: '#cc8844',
  rail_supervisor: '#aa7744',
  crane_lead: '#00ccff',
  stevedore_lead: '#44ff44',
  lashing_lead: '#ffcc00',
  tractor_lead: '#ff44aa',
  yard_lead: '#88aaff',
  stacking_crane_lead: '#44ddbb',
  gate_lead: '#ddaa66',
  rail_lead: '#bb8844',
  crane_operator: '#0088cc',
  stevedore: '#22aa22',
  lasher: '#cc9900',
  signaler: '#ff8888',
  tractor_driver: '#cc2266',
  yard_worker: '#6688cc',
  stacking_crane_op: '#33bb99',
  gate_scanner: '#ccaa44',
  rfid_reader: '#aacc44',
  gate_worker: '#aa8833',
  rail_operator: '#997733',
};

export const roleLabels: Record<TerminalRole, string> = {
  toc: 'TOC',
  berth_manager: 'BMg',
  yard_manager: 'YMg',
  gate_manager: 'GMg',
  hold_supervisor: 'HSv',
  sc_supervisor: 'SCS',
  yard_block_supervisor: 'YBS',
  gate_supervisor: 'GSv',
  rail_supervisor: 'RSv',
  crane_lead: 'CLd',
  stevedore_lead: 'SLd',
  lashing_lead: 'LLd',
  tractor_lead: 'TLd',
  yard_lead: 'YLd',
  stacking_crane_lead: 'SCL',
  gate_lead: 'GLd',
  rail_lead: 'RLd',
  crane_operator: 'CrO',
  stevedore: 'Stv',
  lasher: 'Lsh',
  signaler: 'Sig',
  tractor_driver: 'TrD',
  yard_worker: 'YWk',
  stacking_crane_op: 'SCO',
  gate_scanner: 'GSc',
  rfid_reader: 'RFI',
  gate_worker: 'GWk',
  rail_operator: 'RaO',
};

export const levelColors: Record<HierarchyLevel, string> = {
  0: '#4488cc',
  1: '#44cc88',
  2: '#cc44ff',
  3: '#ff6600',
  4: '#ff9900',
};

export const zoneColors: Record<ZoneId, string> = {
  berth1: '#cc44ff',
  berth2: '#9944ff',
  yard: '#44cc88',
  gate: '#dd8844',
  tractor: '#ff44aa',
};

// Generate the 200-node Phase 3 terminal topology
export function createPhase3Topology(): TerminalTopology {
  const nodes: TerminalNode[] = [];
  const edges: TerminalEdge[] = [];
  let nextId = 1;

  function makeNode(
    role: TerminalRole,
    level: HierarchyLevel,
    label: string,
    opts?: { berthId?: BerthId; holdId?: HoldId; zoneId?: ZoneId; parentId?: string },
  ): TerminalNode {
    const node: TerminalNode = {
      id: `n${nextId++}`,
      role,
      level,
      berthId: opts?.berthId,
      holdId: opts?.holdId,
      zoneId: opts?.zoneId,
      parentId: opts?.parentId,
      label,
      status: Math.random() > 0.12 ? 'active' : Math.random() > 0.5 ? 'busy' : 'idle',
    };
    nodes.push(node);
    if (opts?.parentId) {
      edges.push({ from: opts.parentId, to: node.id });
    }
    return node;
  }

  // ─── H4: Terminal Operations Center ───
  const toc = makeNode('toc', 4, 'TOC');

  // ─── Berths (×2) ───
  const berths: BerthOperation[] = ([1, 2] as BerthId[]).map((berthId) => {
    const zone: ZoneId = berthId === 1 ? 'berth1' : 'berth2';
    const manager = makeNode('berth_manager', 3, `Berth ${berthId} Mgr`, { berthId, zoneId: zone, parentId: toc.id });

    const holds: HoldTeam[] = ([1, 2, 3] as HoldId[]).map((holdId) => {
      const supervisor = makeNode('hold_supervisor', 2, `B${berthId} H${holdId} Sup`, { berthId, holdId, zoneId: zone, parentId: manager.id });

      // Crane team: lead + 3 operators
      const craneLead = makeNode('crane_lead', 1, `B${berthId}H${holdId} CrLd`, { berthId, holdId, zoneId: zone, parentId: supervisor.id });
      const craneOps = Array.from({ length: 3 }, (_, i) =>
        makeNode('crane_operator', 0, `B${berthId}H${holdId} Cr${String.fromCharCode(65 + i)}`, { berthId, holdId, zoneId: zone, parentId: craneLead.id })
      );

      // Stevedore team: lead + 8 workers
      const steveLead = makeNode('stevedore_lead', 1, `B${berthId}H${holdId} StLd`, { berthId, holdId, zoneId: zone, parentId: supervisor.id });
      const steveWorkers = Array.from({ length: 8 }, (_, i) =>
        makeNode('stevedore', 0, `B${berthId}H${holdId} Stv${i + 1}`, { berthId, holdId, zoneId: zone, parentId: steveLead.id })
      );

      // Lashing crew: lead + 4 lashers
      const lashLead = makeNode('lashing_lead', 1, `B${berthId}H${holdId} LsLd`, { berthId, holdId, zoneId: zone, parentId: supervisor.id });
      const lashers = Array.from({ length: 4 }, (_, i) =>
        makeNode('lasher', 0, `B${berthId}H${holdId} Lsh${String.fromCharCode(65 + i)}`, { berthId, holdId, zoneId: zone, parentId: lashLead.id })
      );

      // Signaler (reports to hold supervisor directly)
      const signaler = makeNode('signaler', 0, `B${berthId}H${holdId} Sig`, { berthId, holdId, zoneId: zone, parentId: supervisor.id });

      return {
        holdId,
        supervisor,
        cranes: { lead: craneLead, operators: craneOps },
        stevedores: { lead: steveLead, workers: steveWorkers },
        lashing: { lead: lashLead, lashers },
        signaler,
      };
    });

    return { berthId, manager, holds };
  });

  // ─── Yard Zone ───
  const yardMgr = makeNode('yard_manager', 3, 'Yard Manager', { zoneId: 'yard', parentId: toc.id });

  // Stacking crane zone
  const scSup = makeNode('sc_supervisor', 2, 'SC Zone Sup', { zoneId: 'yard', parentId: yardMgr.id });
  const stackingCranes = Array.from({ length: 2 }, (_, i) => {
    const lead = makeNode('stacking_crane_lead', 1, `SC Lead ${String.fromCharCode(65 + i)}`, { zoneId: 'yard', parentId: scSup.id });
    const operators = Array.from({ length: 2 }, (_, j) =>
      makeNode('stacking_crane_op', 0, `SC ${String.fromCharCode(65 + i)}${j + 1}`, { zoneId: 'yard', parentId: lead.id })
    );
    return { lead, operators };
  });

  // Yard blocks A-D
  const blockNames = ['A', 'B', 'C', 'D'];
  const yardBlockTeams = blockNames.map((name) => {
    const supervisor = makeNode('yard_block_supervisor', 2, `Yard ${name} Sup`, { zoneId: 'yard', parentId: yardMgr.id });
    const lead = makeNode('yard_lead', 1, `Yard ${name} Lead`, { zoneId: 'yard', parentId: supervisor.id });
    const workers = Array.from({ length: 5 }, (_, i) =>
      makeNode('yard_worker', 0, `Yard ${name} Wkr${i + 1}`, { zoneId: 'yard', parentId: lead.id })
    );
    return { supervisor, lead, workers };
  });

  const yard: YardZone = {
    manager: yardMgr,
    scSupervisor: scSup,
    stackingCranes,
    blocks: yardBlockTeams,
  };

  // ─── Gate Zone ───
  const gateMgr = makeNode('gate_manager', 3, 'Gate Manager', { zoneId: 'gate', parentId: toc.id });

  // Gates A and B
  const gateNames = ['A', 'B'];
  const gateTeams = gateNames.map((name) => {
    const supervisor = makeNode('gate_supervisor', 2, `Gate ${name} Sup`, { zoneId: 'gate', parentId: gateMgr.id });
    const lead = makeNode('gate_lead', 1, `Gate ${name} Lead`, { zoneId: 'gate', parentId: supervisor.id });
    const scanners = Array.from({ length: 2 }, (_, i) =>
      makeNode('gate_scanner', 0, `Gate ${name} Scan${i + 1}`, { zoneId: 'gate', parentId: lead.id })
    );
    const rfidReaders = Array.from({ length: 2 }, (_, i) =>
      makeNode('rfid_reader', 0, `Gate ${name} RFID${i + 1}`, { zoneId: 'gate', parentId: lead.id })
    );
    const workers = Array.from({ length: 4 }, (_, i) =>
      makeNode('gate_worker', 0, `Gate ${name} Wkr${i + 1}`, { zoneId: 'gate', parentId: lead.id })
    );
    return { supervisor, lead, scanners, rfidReaders, workers };
  });

  // Rail
  const railSup = makeNode('rail_supervisor', 2, 'Rail Sup', { zoneId: 'gate', parentId: gateMgr.id });
  const railLead = makeNode('rail_lead', 1, 'Rail Lead', { zoneId: 'gate', parentId: railSup.id });
  const railOps = Array.from({ length: 5 }, (_, i) =>
    makeNode('rail_operator', 0, `Rail Op ${i + 1}`, { zoneId: 'gate', parentId: railLead.id })
  );

  const gate: GateZone = {
    manager: gateMgr,
    gates: gateTeams,
    rail: { supervisor: railSup, lead: railLead, operators: railOps },
  };

  // ─── Tractor Pool (shared across berths) ───
  const tractorLead = makeNode('tractor_lead', 1, 'Tractor Pool Ld', { zoneId: 'tractor', parentId: toc.id });
  const tractorDrivers = Array.from({ length: 12 }, (_, i) =>
    makeNode('tractor_driver', 0, `Tractor ${i + 1}`, { zoneId: 'tractor', parentId: tractorLead.id })
  );

  const tractorPool: TractorPool = { lead: tractorLead, drivers: tractorDrivers };

  // ─── Yard Blocks data ───
  const yardBlocks: YardBlock[] = [
    { id: 'yard-a', name: 'Yard Block A', capacity: 200, filled: 142, rows: 5, cols: 8 },
    { id: 'yard-b', name: 'Yard Block B', capacity: 200, filled: 88, rows: 5, cols: 8 },
    { id: 'yard-c', name: 'Yard Block C', capacity: 160, filled: 110, rows: 4, cols: 8 },
    { id: 'yard-d', name: 'Yard Block D', capacity: 160, filled: 45, rows: 4, cols: 8 },
  ];

  const events = generatePhase3Events();

  return { toc, berths, yard, gate, tractorPool, yardBlocks, nodes, edges, events };
}

function generatePhase3Events(): TerminalEvent[] {
  const now = Date.now();
  const types: TerminalEvent['type'][] = [
    'container_move', 'crane_cycle', 'tractor_dispatch', 'lashing_complete',
    'yard_store', 'status_change', 'gate_scan', 'rail_load', 'stacking_crane_cycle',
  ];
  const messages: Record<TerminalEvent['type'], string[]> = {
    container_move: [
      'Container MSKU-4821 discharged from B1 Hold 2',
      'Container TCLU-1190 loaded to B2 Hold 1',
      'Container TRLU-3345 moved to yard stack C3',
    ],
    crane_cycle: [
      'B1 Crane A cycle complete (38s)',
      'B2 Crane B repositioning for bay 14',
      'B1 Crane C picking container from hold',
    ],
    tractor_dispatch: [
      'Tractor 7 dispatched B1 Hold 3 to Yard A',
      'Tractor 3 returning to B2 Hold 1',
      'Tractor 11 en route Yard C to Gate A',
    ],
    lashing_complete: [
      'B1 H2 lashing complete bay 12',
      'B2 H1 unlashing started bay 08',
      'B1 H3 lashing crew repositioning',
    ],
    yard_store: [
      'Stored at Yard A row 3 col 2 via SC-A1',
      'Retrieved from Yard C row 1 col 5 via SC-B2',
      'Yard D slot allocated row 2 col 7',
    ],
    status_change: [
      'B1 Hold 1 crane team fully active',
      'Tractor 5 idle at yard staging',
      'B2 Hold 3 stevedore team shift change',
    ],
    gate_scan: [
      'Gate A: Truck T-4481 scan complete — cleared',
      'Gate B: Container MSKU-7732 RFID verified',
      'Gate A: Hazmat flag on TCLU-9920 — hold for inspection',
    ],
    rail_load: [
      'Rail: Car 12 loaded with 2 TEU from Yard B',
      'Rail: Locomotive coupled, 18 cars ready',
      'Rail: Intermodal transfer complete — 4 containers',
    ],
    stacking_crane_cycle: [
      'SC-A1 stack cycle 42s — Yard A row 4',
      'SC-B2 retrieve cycle 38s — Yard C row 2',
      'SC-A2 repositioning to Yard B lane 3',
    ],
  };

  return Array.from({ length: 30 }, (_, i) => {
    const type = types[i % types.length];
    const msgList = messages[type];
    const berthId = (i % 5 < 2 ? 1 : i % 5 < 4 ? 2 : undefined) as BerthId | undefined;
    const holdId = (i % 4 === 3 ? undefined : ((i % 3) + 1)) as HoldId | undefined;
    const zoneId = i % 9 < 4 ? (berthId === 1 ? 'berth1' : 'berth2') as ZoneId :
                   i % 9 < 6 ? 'yard' as ZoneId :
                   i % 9 < 8 ? 'gate' as ZoneId : 'tractor' as ZoneId;
    return {
      id: i,
      timestamp: now - (30 - i) * 12000,
      berthId,
      holdId: berthId ? holdId : undefined,
      zoneId,
      source: `node-${(i % 20) + 1}`,
      type,
      message: msgList[i % msgList.length],
    };
  });
}

// Legacy compat
export function createPhase2Topology(): TerminalTopology {
  return createPhase3Topology();
}
