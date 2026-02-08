import { useState } from 'react';
import {
  GapAnalysisReport,
  CapabilityGap,
  LogisticalDependency,
  LogisticalAction,
  HierarchyLevel,
  HealthStatus,
} from '../../types';

interface GapAnalysisPanelProps {
  reports: GapAnalysisReport[];
}

const healthColors: Record<HealthStatus, string> = {
  nominal: '#44ff44',
  degraded: '#ffaa00',
  critical: '#ff4444',
  failed: '#ff0000',
  offline: '#666666',
};

const healthLabels: Record<HealthStatus, string> = {
  nominal: 'NOMINAL',
  degraded: 'DEGRADED',
  critical: 'CRITICAL',
  failed: 'FAILED',
  offline: 'OFFLINE',
};

const levelLabels: Record<HierarchyLevel, string> = {
  H2: 'HOLD',
  H3: 'BERTH',
};

function ConfidenceBar({ current, required, decayRate }: {
  current: number;
  required: number;
  decayRate: number;
}) {
  const pct = Math.min(current * 100, 100);
  const reqPct = Math.min(required * 100, 100);
  const isBelow = current < required;
  const barColor = isBelow ? '#ff4444' : decayRate < -0.03 ? '#ffaa00' : '#44ff44';

  return (
    <div style={{ position: 'relative', height: '8px', background: '#222', borderRadius: '4px', overflow: 'visible' }}>
      <div style={{
        width: `${pct}%`,
        height: '100%',
        background: barColor,
        borderRadius: '4px',
        transition: 'width 0.3s',
      }} />
      {/* Threshold marker */}
      <div style={{
        position: 'absolute',
        left: `${reqPct}%`,
        top: '-2px',
        width: '2px',
        height: '12px',
        background: '#fff',
        opacity: 0.6,
      }} />
    </div>
  );
}

function ActionStatusBadge({ status }: { status: LogisticalAction['status'] }) {
  const colors = {
    pending: { bg: '#333', text: '#888' },
    in_progress: { bg: '#1a3a2a', text: '#44ff44' },
    blocked: { bg: '#3a1a1a', text: '#ff4444' },
  };
  const c = colors[status];
  return (
    <span style={{
      padding: '1px 6px',
      borderRadius: '3px',
      fontSize: '9px',
      fontWeight: 'bold',
      background: c.bg,
      color: c.text,
      textTransform: 'uppercase',
    }}>
      {status.replace('_', ' ')}
    </span>
  );
}

function GapCard({ gap }: { gap: CapabilityGap }) {
  const [expanded, setExpanded] = useState(false);
  const isBelowThreshold = gap.currentConfidence < gap.requiredConfidence;

  return (
    <div
      onClick={() => setExpanded(!expanded)}
      style={{
        background: isBelowThreshold ? '#2a1a1a' : '#1a1a2a',
        border: `1px solid ${isBelowThreshold ? '#553333' : '#333'}`,
        borderRadius: '6px',
        padding: '8px 10px',
        marginBottom: '6px',
        cursor: 'pointer',
      }}
    >
      {/* Header row */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '4px' }}>
        <span style={{
          color: isBelowThreshold ? '#ff6666' : '#ffaa44',
          fontWeight: 'bold',
          fontSize: '12px',
        }}>
          {gap.capabilityName}
        </span>
        <span style={{
          color: '#888',
          fontSize: '10px',
          textTransform: 'uppercase',
        }}>
          {gap.capabilityType}
        </span>
      </div>

      {/* Confidence bar */}
      <ConfidenceBar
        current={gap.currentConfidence}
        required={gap.requiredConfidence}
        decayRate={gap.decayRate}
      />

      {/* Stats row */}
      <div style={{ display: 'flex', gap: '12px', marginTop: '4px', fontSize: '10px' }}>
        <span style={{ color: '#aaa' }}>
          conf: <span style={{ color: isBelowThreshold ? '#ff6666' : '#fff' }}>
            {(gap.currentConfidence * 100).toFixed(0)}%
          </span>
          /{(gap.requiredConfidence * 100).toFixed(0)}%
        </span>
        <span style={{ color: '#aaa' }}>
          decay: <span style={{ color: gap.decayRate < -0.03 ? '#ffaa00' : '#888' }}>
            {(gap.decayRate * 100).toFixed(1)}%/t
          </span>
        </span>
        {gap.etaThresholdBreach !== null && (
          <span style={{ color: gap.etaThresholdBreach <= 3 ? '#ff6666' : '#aaa' }}>
            breach: {gap.etaThresholdBreach}t
          </span>
        )}
      </div>

      {/* Reason */}
      <div style={{ fontSize: '10px', color: '#888', marginTop: '3px' }}>
        {gap.reason}
      </div>

      {/* Authority/oversight info */}
      {gap.requiresOversight && (
        <div style={{ fontSize: '9px', color: '#ff44ff', marginTop: '2px' }}>
          OVERSIGHT REQUIRED {gap.maxAuthority ? `(${gap.maxAuthority})` : '(no authority)'}
        </div>
      )}

      {/* Expanded: pending actions */}
      {expanded && gap.pendingActions.length > 0 && (
        <div style={{ marginTop: '6px', borderTop: '1px solid #333', paddingTop: '6px' }}>
          <div style={{ fontSize: '10px', color: '#888', marginBottom: '4px' }}>
            PENDING ACTIONS ({gap.pendingActions.length})
          </div>
          {gap.pendingActions.map((action) => (
            <div key={action.id} style={{
              display: 'flex',
              alignItems: 'center',
              gap: '6px',
              padding: '3px 0',
              fontSize: '10px',
            }}>
              <ActionStatusBadge status={action.status} />
              <span style={{ color: '#ccc', flex: 1 }}>{action.description}</span>
              {action.etaMinutes !== null && (
                <span style={{ color: '#888' }}>ETA {action.etaMinutes}m</span>
              )}
              {action.blockedBy && (
                <span style={{ color: '#ff6666', fontSize: '9px' }}>
                  blocked: {action.blockedBy}
                </span>
              )}
            </div>
          ))}
        </div>
      )}

      {/* Click hint */}
      {!expanded && gap.pendingActions.length > 0 && (
        <div style={{ fontSize: '9px', color: '#555', marginTop: '2px' }}>
          {gap.pendingActions.length} action{gap.pendingActions.length !== 1 ? 's' : ''} pending
        </div>
      )}
    </div>
  );
}

function DependencyCard({ dep }: { dep: LogisticalDependency }) {
  const statusColors = {
    available: '#44ff44',
    unavailable: '#ff4444',
    degraded: '#ffaa00',
  };

  return (
    <div style={{
      background: '#1a1a2a',
      border: `1px solid ${dep.status === 'unavailable' ? '#553333' : '#333'}`,
      borderRadius: '6px',
      padding: '8px 10px',
      marginBottom: '6px',
    }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '2px' }}>
        <span style={{ color: '#ddd', fontWeight: 'bold', fontSize: '11px' }}>
          {dep.resourceName}
        </span>
        <span style={{
          color: statusColors[dep.status],
          fontSize: '10px',
          fontWeight: 'bold',
          textTransform: 'uppercase',
        }}>
          {dep.status}
        </span>
      </div>
      <div style={{ fontSize: '10px', color: '#888' }}>
        {dep.reason}
      </div>
      {dep.availableInMinutes !== null && (
        <div style={{ fontSize: '10px', color: '#ffaa44', marginTop: '2px' }}>
          Available in {dep.availableInMinutes} min
        </div>
      )}
      {dep.affectedCapabilities.length > 0 && (
        <div style={{ display: 'flex', gap: '4px', marginTop: '4px', flexWrap: 'wrap' }}>
          {dep.affectedCapabilities.map((cap) => (
            <span key={cap} style={{
              padding: '1px 5px',
              borderRadius: '3px',
              fontSize: '9px',
              background: '#222',
              color: '#aaa',
            }}>
              {cap}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

function ReportSection({ report }: { report: GapAnalysisReport }) {
  const gapCount = report.gaps.length;
  const criticalGaps = report.gaps.filter(g => g.currentConfidence < g.requiredConfidence).length;
  const blockedDeps = report.logisticalDependencies.filter(d => d.status === 'unavailable').length;

  return (
    <div style={{ marginBottom: '16px' }}>
      {/* Location header */}
      <div style={{
        display: 'flex',
        justifyContent: 'space-between',
        alignItems: 'center',
        padding: '6px 10px',
        background: '#111',
        borderRadius: '6px 6px 0 0',
        borderBottom: '1px solid #333',
      }}>
        <div>
          <span style={{ color: '#00ffff', fontWeight: 'bold', fontSize: '13px' }}>
            {report.locationLabel}
          </span>
          <span style={{ color: '#555', fontSize: '11px', marginLeft: '8px' }}>
            {levelLabels[report.level]} {report.locationId}
          </span>
        </div>
        <span style={{
          color: healthColors[report.worstHealth],
          fontSize: '10px',
          fontWeight: 'bold',
        }}>
          {healthLabels[report.worstHealth]}
        </span>
      </div>

      {/* Summary stats */}
      <div style={{
        display: 'flex',
        gap: '12px',
        padding: '6px 10px',
        background: '#0d0d1a',
        fontSize: '10px',
        borderBottom: '1px solid #222',
      }}>
        <span style={{ color: '#aaa' }}>
          Readiness: <span style={{
            color: report.readinessScore >= 0.7 ? '#44ff44' : report.readinessScore >= 0.5 ? '#ffaa00' : '#ff4444',
            fontWeight: 'bold',
          }}>
            {(report.readinessScore * 100).toFixed(0)}%
          </span>
        </span>
        <span style={{ color: '#aaa' }}>
          Ops: <span style={{ color: '#fff' }}>{report.operationalCount}/{report.totalCount}</span>
        </span>
        {criticalGaps > 0 && (
          <span style={{ color: '#ff6666' }}>
            {criticalGaps} gap{criticalGaps !== 1 ? 's' : ''}
          </span>
        )}
        {blockedDeps > 0 && (
          <span style={{ color: '#ff4444' }}>
            {blockedDeps} blocked
          </span>
        )}
      </div>

      {/* Gaps */}
      {gapCount > 0 && (
        <div style={{ padding: '8px 10px 4px' }}>
          <div style={{ fontSize: '10px', color: '#888', marginBottom: '6px', textTransform: 'uppercase', letterSpacing: '0.5px' }}>
            Capability Gaps ({gapCount})
          </div>
          {report.gaps.map((gap, i) => (
            <GapCard key={i} gap={gap} />
          ))}
        </div>
      )}

      {/* Logistical dependencies */}
      {report.logisticalDependencies.length > 0 && (
        <div style={{ padding: '4px 10px 8px' }}>
          <div style={{ fontSize: '10px', color: '#888', marginBottom: '6px', textTransform: 'uppercase', letterSpacing: '0.5px' }}>
            Logistical Dependencies ({report.logisticalDependencies.length})
          </div>
          {report.logisticalDependencies.map((dep, i) => (
            <DependencyCard key={i} dep={dep} />
          ))}
        </div>
      )}

      {/* No gaps message */}
      {gapCount === 0 && report.logisticalDependencies.length === 0 && (
        <div style={{ padding: '12px 10px', fontSize: '11px', color: '#44ff44', textAlign: 'center' }}>
          All capabilities nominal
        </div>
      )}
    </div>
  );
}

export function GapAnalysisPanel({ reports }: GapAnalysisPanelProps) {
  const [filterLevel, setFilterLevel] = useState<HierarchyLevel | 'all'>('all');

  const filtered = filterLevel === 'all'
    ? reports
    : reports.filter(r => r.level === filterLevel);

  const totalGaps = reports.reduce((sum, r) => sum + r.gaps.filter(g => g.currentConfidence < g.requiredConfidence).length, 0);

  return (
    <div>
      {/* Panel header */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '8px' }}>
        <h3 style={{ color: '#ff8844', margin: 0, fontSize: '14px' }}>
          GAP ANALYSIS
          {totalGaps > 0 && (
            <span style={{
              marginLeft: '8px',
              padding: '1px 6px',
              borderRadius: '8px',
              fontSize: '10px',
              background: '#3a1a1a',
              color: '#ff6666',
            }}>
              {totalGaps}
            </span>
          )}
        </h3>
        {/* Level filter */}
        <div style={{ display: 'flex', gap: '4px' }}>
          {(['all', 'H2', 'H3'] as const).map((lvl) => (
            <button
              key={lvl}
              onClick={() => setFilterLevel(lvl)}
              style={{
                padding: '2px 8px',
                fontSize: '10px',
                border: 'none',
                borderRadius: '3px',
                cursor: 'pointer',
                background: filterLevel === lvl ? '#ff8844' : '#222',
                color: filterLevel === lvl ? '#000' : '#888',
                fontWeight: filterLevel === lvl ? 'bold' : 'normal',
              }}
            >
              {lvl === 'all' ? 'ALL' : lvl}
            </button>
          ))}
        </div>
      </div>

      {/* Reports */}
      {filtered.map((report, i) => (
        <ReportSection key={i} report={report} />
      ))}

      {filtered.length === 0 && (
        <div style={{ padding: '16px', fontSize: '11px', color: '#555', textAlign: 'center' }}>
          No reports at this level
        </div>
      )}
    </div>
  );
}

export default GapAnalysisPanel;
