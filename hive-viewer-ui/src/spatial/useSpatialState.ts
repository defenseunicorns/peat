/** Hook providing derived spatial state from the viewer store. */

import { useMemo } from 'react';
import { useViewerStore } from '../protocol/state';
import { deriveSpatialState } from './deriveState';
import type { SpatialDerivedState } from './types';

export function useSpatialState(): SpatialDerivedState {
  const nodes = useViewerStore((s) => s.nodes);
  const events = useViewerStore((s) => s.events);
  return useMemo(() => deriveSpatialState(nodes, events), [nodes, events]);
}
