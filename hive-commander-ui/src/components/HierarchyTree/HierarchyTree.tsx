import { PortOpsEntity, PortOpsRole, HierarchyLevel, inferRole } from '../../wire-types';

/** Color scheme for each port-ops role. */
const ROLE_COLORS: Record<PortOpsRole, string> = {
  signaler: '#ffaa00',       // Amber — safety/visibility
  crane_operator: '#44aaff', // Blue — equipment
  stevedore: '#88cc44',      // Green — ground crew
  supervisor: '#ff44ff',     // Magenta — authority
};

/** Display labels for role actions shown in the tree. */
const ROLE_ACTION_LABELS: Record<PortOpsRole, Record<string, string>> = {
  signaler: {
    idle: 'Standing by',
    active: 'Observing',
    signaling: 'Signaling',
    error: 'FAULT',
  },
  crane_operator: {
    idle: 'Crane idle',
    active: 'Operating',
    signaling: 'Awaiting signal',
    error: 'FAULT',
  },
  stevedore: {
    idle: 'Standing by',
    active: 'Working',
    signaling: 'Coordinating',
    error: 'FAULT',
  },
  supervisor: {
    idle: 'Monitoring',
    active: 'Directing',
    signaling: 'Communicating',
    error: 'FAULT',
  },
};

/** Hierarchy level labels. */
const LEVEL_LABELS: Record<HierarchyLevel, string> = {
  1: 'H1 Ground',
  2: 'H2 Equipment',
  3: 'H3 Supervisor',
};

interface HierarchyTreeProps {
  entities: PortOpsEntity[];
  selectedEntityId?: string;
  onSelectEntity?: (entityId: string) => void;
}

export function HierarchyTree({ entities, selectedEntityId, onSelectEntity }: HierarchyTreeProps) {
  // Group entities by hierarchy level
  const byLevel = new Map<HierarchyLevel, PortOpsEntity[]>();
  for (const entity of entities) {
    const level = entity.hierarchyLevel as HierarchyLevel;
    if (!byLevel.has(level)) byLevel.set(level, []);
    byLevel.get(level)!.push(entity);
  }

  // Sort levels descending (supervisors on top)
  const levels = Array.from(byLevel.keys()).sort((a, b) => b - a);

  return (
    <div style={{ fontFamily: 'monospace', fontSize: '13px', color: '#ccc' }}>
      {levels.map((level) => (
        <div key={level} style={{ marginBottom: '12px' }}>
          <div style={{
            color: '#888',
            fontSize: '11px',
            textTransform: 'uppercase',
            letterSpacing: '1px',
            marginBottom: '4px',
            borderBottom: '1px solid #333',
            paddingBottom: '2px',
          }}>
            {LEVEL_LABELS[level]}
          </div>
          {byLevel.get(level)!.map((entity) => {
            const role = inferRole(entity.entityId) ?? entity.role;
            const color = ROLE_COLORS[role] ?? '#888';
            const actionLabel = ROLE_ACTION_LABELS[role]?.[entity.state] ?? entity.state;
            const isSelected = entity.entityId === selectedEntityId;

            return (
              <div
                key={entity.entityId}
                onClick={() => onSelectEntity?.(entity.entityId)}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '8px',
                  padding: '4px 8px',
                  cursor: 'pointer',
                  background: isSelected ? '#1a2a3a' : 'transparent',
                  borderLeft: `3px solid ${color}`,
                  marginBottom: '2px',
                }}
              >
                <span style={{ color, fontWeight: 'bold', minWidth: '120px' }}>
                  {entity.entityId}
                </span>
                <span style={{ color: '#aaa', fontSize: '11px' }}>
                  {actionLabel}
                </span>
                {entity.lastAction && (
                  <span style={{ color: '#666', fontSize: '10px', marginLeft: 'auto' }}>
                    {entity.lastAction}
                  </span>
                )}
              </div>
            );
          })}
        </div>
      ))}
      {entities.length === 0 && (
        <div style={{ color: '#666', fontStyle: 'italic', padding: '8px' }}>
          No entities active
        </div>
      )}
    </div>
  );
}

export default HierarchyTree;
