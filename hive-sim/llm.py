"""
Port-ops simulation LLM decision logic.

Provides dry-run decision functions for each role. In dry-run mode, decisions
follow deterministic state-machine logic rather than calling an actual LLM.
Each _decide_<role>() function returns the next action for that role given
the current simulation state.
"""

from dataclasses import dataclass
from enum import Enum, auto
from typing import Optional

from lifecycle import PhysicalAction


class CraneState(Enum):
    """Observable crane states for signaler decision-making."""

    IDLE = auto()
    READY_TO_HOIST = auto()
    HOISTING = auto()
    READY_TO_LOWER = auto()
    LOWERING = auto()
    LOAD_SECURED = auto()


@dataclass
class SignalerObservation:
    """What the signaler can observe in the current tick."""

    crane_state: CraneState
    ground_clear: bool
    personnel_in_zone: int
    active_hazards: list[str]
    cycle_complete: bool = False


@dataclass
class Decision:
    """A decision result from the dry-run logic."""

    action: Optional[PhysicalAction]
    reason: str
    confidence: float = 1.0


def _decide_signaler(obs: SignalerObservation) -> Decision:
    """Dry-run decision logic for the signaler role.

    Decision flow:
      1. Observe crane state
      2. Confirm ground clear
      3. Signal ready
      4. Monitor lift
      5. Signal complete

    Args:
        obs: Current signaler observation of the environment.

    Returns:
        Decision with the next physical action (or None to wait).
    """
    # Immediate hazard override: STOP regardless of state
    if obs.active_hazards:
        return Decision(
            action=PhysicalAction.SIGNAL_STOP,
            reason=f"Hazard detected: {', '.join(obs.active_hazards)}",
        )

    # Personnel in zone: STOP until cleared
    if obs.personnel_in_zone > 0:
        return Decision(
            action=PhysicalAction.SIGNAL_STOP,
            reason=f"{obs.personnel_in_zone} personnel in safety zone",
        )

    # State-dependent decisions
    if obs.crane_state == CraneState.IDLE:
        if obs.ground_clear:
            return Decision(
                action=PhysicalAction.SIGNAL_CLEAR,
                reason="Ground clear, crane idle, ready for next operation",
            )
        return Decision(
            action=None,
            reason="Waiting for ground to clear",
        )

    if obs.crane_state == CraneState.READY_TO_HOIST:
        if obs.ground_clear:
            return Decision(
                action=PhysicalAction.SIGNAL_HOIST,
                reason="Ground clear, signaling crane to hoist",
            )
        return Decision(
            action=PhysicalAction.SIGNAL_STOP,
            reason="Ground not clear, cannot authorize hoist",
        )

    if obs.crane_state == CraneState.HOISTING:
        # Monitor during hoist - no action unless hazard (handled above)
        return Decision(
            action=None,
            reason="Monitoring active hoist",
        )

    if obs.crane_state == CraneState.READY_TO_LOWER:
        if obs.ground_clear:
            return Decision(
                action=PhysicalAction.SIGNAL_LOWER,
                reason="Ground clear, signaling crane to lower",
            )
        return Decision(
            action=PhysicalAction.SIGNAL_STOP,
            reason="Ground not clear, cannot authorize lower",
        )

    if obs.crane_state == CraneState.LOWERING:
        # Monitor during lower
        return Decision(
            action=None,
            reason="Monitoring active lower",
        )

    if obs.crane_state == CraneState.LOAD_SECURED:
        return Decision(
            action=PhysicalAction.SIGNAL_CLEAR,
            reason="Load secured, cycle complete",
        )

    return Decision(
        action=None,
        reason="Unknown crane state, holding",
        confidence=0.5,
    )
