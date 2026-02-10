# Signaler

**Hierarchy Level:** H1 (Ground Crew)
**Role Type:** Safety-Critical Support

## Description

The signaler provides visual hand-signal communication between crane operators
and ground crew during container lift and lower operations. This is an essential
safety role: crane operations cannot proceed without active signaler confirmation.

## Responsibilities

- Provide standardized hand signals to crane operator during lift/lower cycles
- Confirm container alignment before crane engagement
- Verify ground clearance beneath suspended loads
- Maintain personnel safety zone (no personnel within swing radius)
- Halt operations immediately upon detecting any hazard

## Signal Set

| Signal         | Meaning                                      |
|----------------|----------------------------------------------|
| SIGNAL_HOIST   | Clear to raise load                          |
| SIGNAL_LOWER   | Clear to lower load                          |
| SIGNAL_STOP    | Halt all crane movement immediately          |
| SIGNAL_CLEAR   | Area clear, safe to proceed with next action |

## Operating Constraints

- Must maintain line-of-sight with crane operator at all times
- Cannot authorize simultaneous operations on adjacent bays
- Must re-confirm ground clear after any personnel movement in zone
- Signal authority is non-delegable during active lift cycle

## Decision Loop (Dry-Run)

1. Observe crane state and pending operation
2. Inspect ground zone for personnel and obstructions
3. Confirm ground clear to crane operator (SIGNAL_CLEAR)
4. Monitor lift/lower cycle, ready to issue SIGNAL_STOP
5. Signal operation complete when load is secured

## Visibility Range

The signaler operates within direct visual range of the crane cab,
typically 50-150 meters depending on crane type and port layout.
