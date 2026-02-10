// Phase 2 berth operation wire types
// Port hierarchy: H4 (Scheduler) -> H3 (Berth Manager) -> H2 (Hold Supervisor) -> H1 (Team Lead) -> H0 (Worker)

export type HierarchyLevel = 0 | 1 | 2 | 3 | 4;

export type BerthRole =
  | 'scheduler'        // H4
  | 'berth_manager'    // H3
  | 'hold_supervisor'  // H2
  | 'crane_lead'       // H1
  | 'stevedore_lead'   // H1
  | 'lashing_lead'     // H1
  | 'tractor_lead'     // H1
  | 'yard_lead'        // H1
  | 'crane_operator'   // H0
  | 'stevedore'        // H0
  | 'lasher'           // H0
  | 'signaler'         // H0
  | 'tractor_driver'   // H0
  | 'yard_worker';     // H0

export type HoldId = 1 | 2 | 3;

export interface BerthNode {
  id: string;
  role: BerthRole;
  level: HierarchyLevel;
  holdId?: HoldId;       // undefined for shared resources (tractors, yard blocks)
  parentId?: string;     // undefined for scheduler (root)
  label: string;
  status: 'active' | 'idle' | 'busy' | 'offline';
}

export interface BerthEdge {
  from: string;
  to: string;
}

export interface HoldTeam {
  holdId: HoldId;
  supervisor: BerthNode;
  cranes: { lead: BerthNode; operators: BerthNode[] };
  stevedores: { lead: BerthNode; workers: BerthNode[] };
  lashing: { lead: BerthNode; lashers: BerthNode[] };
  signaler: BerthNode;
}

export interface SharedPool {
  tractors: { lead: BerthNode; drivers: BerthNode[] };
  yardBlockA: { lead: BerthNode; workers: BerthNode[] };
  yardBlockB: { lead: BerthNode; workers: BerthNode[] };
}

export interface YardBlock {
  id: string;
  name: string;
  capacity: number;
  filled: number;
  rows: number;
  cols: number;
}

export interface BerthEvent {
  id: number;
  timestamp: number;
  holdId?: HoldId;
  source: string;
  type: 'container_move' | 'crane_cycle' | 'tractor_dispatch' | 'lashing_complete' | 'yard_store' | 'status_change';
  message: string;
}

export interface BerthTopology {
  scheduler: BerthNode;
  berthManager: BerthNode;
  holds: HoldTeam[];
  shared: SharedPool;
  yardBlocks: YardBlock[];
  nodes: BerthNode[];
  edges: BerthEdge[];
  events: BerthEvent[];
}

// Role display properties
export const roleColors: Record<BerthRole, string> = {
  scheduler: '#ff9900',
  berth_manager: '#ff6600',
  hold_supervisor: '#cc44ff',
  crane_lead: '#00ccff',
  stevedore_lead: '#44ff44',
  lashing_lead: '#ffcc00',
  tractor_lead: '#ff44aa',
  yard_lead: '#88aaff',
  crane_operator: '#0088cc',
  stevedore: '#22aa22',
  lasher: '#cc9900',
  signaler: '#ff8888',
  tractor_driver: '#cc2266',
  yard_worker: '#6688cc',
};

export const roleLabels: Record<BerthRole, string> = {
  scheduler: 'SCH',
  berth_manager: 'BMG',
  hold_supervisor: 'HSV',
  crane_lead: 'CLd',
  stevedore_lead: 'SLd',
  lashing_lead: 'LLd',
  tractor_lead: 'TLd',
  yard_lead: 'YLd',
  crane_operator: 'CrO',
  stevedore: 'Stv',
  lasher: 'Lsh',
  signaler: 'Sig',
  tractor_driver: 'TrD',
  yard_worker: 'YWk',
};

export const levelColors: Record<HierarchyLevel, string> = {
  0: '#4488cc',
  1: '#44cc88',
  2: '#cc44ff',
  3: '#ff6600',
  4: '#ff9900',
};

// Generate the 58-node Phase 2 topology
export function createPhase2Topology(): BerthTopology {
  const nodes: BerthNode[] = [];
  const edges: BerthEdge[] = [];
  let nextId = 1;

  function makeNode(role: BerthRole, level: HierarchyLevel, label: string, holdId?: HoldId, parentId?: string): BerthNode {
    const node: BerthNode = {
      id: `n${nextId++}`,
      role,
      level,
      holdId,
      parentId,
      label,
      status: Math.random() > 0.15 ? 'active' : Math.random() > 0.5 ? 'busy' : 'idle',
    };
    nodes.push(node);
    if (parentId) {
      edges.push({ from: parentId, to: node.id });
    }
    return node;
  }

  // H4: Scheduler
  const scheduler = makeNode('scheduler', 4, 'Scheduler');

  // H3: Berth Manager
  const berthManager = makeNode('berth_manager', 3, 'Berth Mgr', undefined, scheduler.id);

  // H2 + H1 + H0: Per-hold teams
  const holds: HoldTeam[] = ([1, 2, 3] as HoldId[]).map((holdId) => {
    const supervisor = makeNode('hold_supervisor', 2, `Hold ${holdId} Sup`, holdId, berthManager.id);

    // Crane team
    const craneLead = makeNode('crane_lead', 1, `H${holdId} Crane Ld`, holdId, supervisor.id);
    const craneOps = [
      makeNode('crane_operator', 0, `H${holdId} Crane A`, holdId, craneLead.id),
      makeNode('crane_operator', 0, `H${holdId} Crane B`, holdId, craneLead.id),
    ];

    // Stevedore team
    const steveLead = makeNode('stevedore_lead', 1, `H${holdId} Steve Ld`, holdId, supervisor.id);
    const steveWorkers = Array.from({ length: 5 }, (_, i) =>
      makeNode('stevedore', 0, `H${holdId} Steve ${i + 1}`, holdId, steveLead.id)
    );

    // Lashing crew
    const lashLead = makeNode('lashing_lead', 1, `H${holdId} Lash Ld`, holdId, supervisor.id);
    const lashers = [
      makeNode('lasher', 0, `H${holdId} Lasher A`, holdId, lashLead.id),
      makeNode('lasher', 0, `H${holdId} Lasher B`, holdId, lashLead.id),
    ];

    // Signaler
    const signaler = makeNode('signaler', 0, `H${holdId} Signaler`, holdId, supervisor.id);

    return {
      holdId,
      supervisor,
      cranes: { lead: craneLead, operators: craneOps },
      stevedores: { lead: steveLead, workers: steveWorkers },
      lashing: { lead: lashLead, lashers },
      signaler,
    };
  });

  // Shared: Tractor pool
  const tractorLead = makeNode('tractor_lead', 1, 'Tractor Pool Ld', undefined, berthManager.id);
  const tractorDrivers = Array.from({ length: 5 }, (_, i) =>
    makeNode('tractor_driver', 0, `Tractor ${i + 1}`, undefined, tractorLead.id)
  );

  // Shared: Yard Block A
  const yardLeadA = makeNode('yard_lead', 1, 'Yard A Lead', undefined, berthManager.id);
  const yardWorkersA = Array.from({ length: 3 }, (_, i) =>
    makeNode('yard_worker', 0, `Yard A Wkr ${i + 1}`, undefined, yardLeadA.id)
  );

  // Shared: Yard Block B
  const yardLeadB = makeNode('yard_lead', 1, 'Yard B Lead', undefined, berthManager.id);
  const yardWorkersB = Array.from({ length: 3 }, (_, i) =>
    makeNode('yard_worker', 0, `Yard B Wkr ${i + 1}`, undefined, yardLeadB.id)
  );

  const shared: SharedPool = {
    tractors: { lead: tractorLead, drivers: tractorDrivers },
    yardBlockA: { lead: yardLeadA, workers: yardWorkersA },
    yardBlockB: { lead: yardLeadB, workers: yardWorkersB },
  };

  const yardBlocks: YardBlock[] = [
    { id: 'yard-a', name: 'Yard Block A', capacity: 120, filled: 78, rows: 4, cols: 6 },
    { id: 'yard-b', name: 'Yard Block B', capacity: 120, filled: 45, rows: 4, cols: 6 },
  ];

  const events = generateDemoEvents();

  return { scheduler, berthManager, holds, shared, yardBlocks, nodes, edges, events };
}

function generateDemoEvents(): BerthEvent[] {
  const now = Date.now();
  const types: BerthEvent['type'][] = ['container_move', 'crane_cycle', 'tractor_dispatch', 'lashing_complete', 'yard_store', 'status_change'];
  const messages: Record<BerthEvent['type'], string[]> = {
    container_move: ['Container MSKU-4821 moved to stack B3', 'Container TCLU-1190 discharged from hold', 'Container TRLU-3345 loaded to vessel'],
    crane_cycle: ['Crane A cycle complete (42s)', 'Crane B repositioning', 'Crane A picking container'],
    tractor_dispatch: ['Tractor 3 dispatched to Yard A', 'Tractor 1 returning to Hold 2', 'Tractor 5 en route to Yard B'],
    lashing_complete: ['Lashing complete bay 12', 'Unlashing started bay 08', 'Lashing crew repositioning'],
    yard_store: ['Stored at Yard A row 3 col 2', 'Retrieved from Yard B row 1 col 5', 'Yard A slot allocated'],
    status_change: ['Hold 1 crane team active', 'Tractor 2 idle', 'Hold 3 stevedore team on break'],
  };

  return Array.from({ length: 20 }, (_, i) => {
    const type = types[i % types.length];
    const msgList = messages[type];
    return {
      id: i,
      timestamp: now - (20 - i) * 15000,
      holdId: (i % 4 === 3 ? undefined : ((i % 3) + 1)) as HoldId | undefined,
      source: `node-${(i % 12) + 1}`,
      type,
      message: msgList[i % msgList.length],
    };
  });
}
