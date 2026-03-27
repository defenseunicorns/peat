# Product Requirements Documents (PRDs)

Implementation specs for planned work. Each PRD references the originating GitHub issue and related ADRs.

## QoS, TTL, and Garbage Collection (Epic #670)

Recommended implementation order:

| PRD | Title | Issue | ADRs | Effort |
|-----|-------|-------|------|--------|
| [001](001-ttl-automerge-integration.md) | TTL and Automerge Store Integration | #667 | 016 | 0.5–1.5 days |
| [002](002-tombstone-sync-and-delete.md) | Tombstone Sync and DocumentStore::delete() | #668 | 034 | 2–3 days |
| [003](003-sync-mode-enforcement.md) | Sync Mode Enforcement | #666 | 019 | 2–3 days |
| [004](004-bandwidth-allocation.md) | Bandwidth Allocation to Sync Transport | #665 | 019 | 3–5 days |
| [005](005-storage-eviction.md) | Priority-Based Storage Eviction | #669 | 016, 019 | 1.5–2 days |
