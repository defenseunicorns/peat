/** Drives animation state machines every frame via useFrame. */

import { useFrame } from '@react-three/fiber';
import { useAnimationStore } from './animationState';

export function useAnimationTick() {
  useFrame((_, delta) => {
    useAnimationStore.getState().tick(delta);
  });
}
