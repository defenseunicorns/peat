# ADR-056: Compromised Node Detection, Isolation, and Ejection

**Status**: Proposed
**Date**: 2026-03-27
**Authors**: Kit Plummer
**Related**: ADR-006 (Security), ADR-044 (E2E Encryption & Key Management), ADR-048 (Membership Certificates), ADR-034 (Tombstone Management)
**Resolves**: ADR-006 Open Question #2 (certificate revocation without network), ADR-006 Open Question #3 (handling compromised devices in the field)

## Context

Peat Protocol operates in contested tactical environments where node compromise is not hypothetical — it is an expected operational condition. A captured or subverted node possesses valid cryptographic credentials, knows the formation key, and participates in CRDT state synchronization. The compromised node can:

- **Inject malicious CRDT state** that propagates to all peers on sync
- **Exfiltrate all synced data** to an adversary
- **Disrupt coordination** by issuing false commands or corrupted position reports
- **Persist access** because its credentials remain valid across network partitions

The core challenge is: **how does a decentralized mesh revoke a node's participation when no central authority may be reachable, nodes operate through network partitions, and the compromised node holds the same cryptographic material as legitimate peers?**

### Current State

Peat has partial coverage:

| Capability | Status | ADR |
|-----------|--------|-----|
| Short-lived certificates with grace period | Implemented | 048 |
| CRDT revocation tombstones (remove-wins) | Partially implemented | 006 |
| MLS forward secrecy on member removal | Designed, not built | 044 |
| Signed mutations on CRDT operations | Designed, not enforced | 006 |
| Compromised node detection | **Not designed** | — |
| In-field ejection without central authority | **Not designed** | — |
| Revocation propagation through partitioned mesh | **Not designed** | — |
| Transitive revocation (revoking a delegator) | **Not designed** | — |

### Requirements

1. **Immediate local isolation** — A node that detects misbehavior must be able to blacklist a peer instantly, without consensus or network communication.
2. **Cell-level ejection** — A cell must be able to eject a compromised member through agreement among remaining members, without requiring an ADMIN-tier node.
3. **Mesh-wide propagation** — Ejection must propagate to all nodes in the formation, including across network partitions, using existing CRDT sync.
4. **Forward secrecy** — A removed node must be excluded from all future encrypted communications.
5. **Partition tolerance** — All mechanisms must function during network partitions. Cross-partition convergence must occur when partitions heal.
6. **Irrevocability** — Once ejected, a node cannot be re-admitted with the same key material. Re-enrollment with fresh credentials and explicit authority approval is required.
7. **Resistance to false accusation** — A coalition of fewer than the ejection threshold should not be able to eject an honest node.
8. **Transitive revocation** — Revoking an ENROLL delegate must cascade to all nodes that delegate enrolled.

## Decision

Implement a **five-layer defense** for compromised node handling. Each layer operates independently and provides defense-in-depth. Layers 1-3 are new; Layers 4-5 formalize and extend existing capabilities.

### Layer 1: Signed CRDT Operations (Prevention)

**Mechanism**: Every CRDT mutation is wrapped in a `SignedChange` envelope containing the author's Ed25519 signature over the Automerge change hash. Peers verify signatures during sync before merging.

**Rationale**: Automerge internally tracks a hash DAG of changes but does not verify authorship. A compromised relay node can currently inject unsigned mutations that merge without attribution. Signing closes this gap.

**Design**:

```
┌─────────────────────────────────────────┐
│ SignedChange                            │
├─────────────────────────────────────────┤
│ change_hash:  [u8; 32]   (Automerge)   │
│ author_id:    EndpointId  (Ed25519 pub) │
│ signature:    [u8; 64]    (Ed25519 sig) │
│ predecessors: Vec<[u8; 32]> (causal)    │
│ timestamp:    u64          (logical)    │
│ payload:      Vec<u8>      (the change) │
└─────────────────────────────────────────┘
```

**Verification on sync receive**:
1. Verify `signature` over `(change_hash, author_id, predecessors, timestamp)`.
2. Check `author_id` is not in the local revocation set.
3. Check `author_id` is a known formation member.
4. If any check fails, reject the change and increment the peer's suspicion score (Layer 2).

**Backward compatibility**: Changes without a `SignedChange` wrapper are accepted during a migration period (configurable, default 7 days). After the migration window, unsigned changes are rejected.

**Reference**: Kleppmann, "Making CRDTs Byzantine Fault Tolerant," PaPoC 2022.

### Layer 2: Local Behavioral Blacklisting (Immediate Isolation)

**Mechanism**: Each node maintains a local suspicion score per peer. Anomalous behavior increments the score. When the score exceeds a threshold, the peer is locally blacklisted — all connections are dropped, sync is refused, and a revocation vote is proposed (Layer 3).

**Detectable anomalies** (each with configurable score increment):

| Anomaly | Score | Detection |
|---------|-------|-----------|
| Invalid signature on a CRDT change | +100 (instant blacklist) | Layer 1 verification |
| Equivocation (two different signed changes with same causal predecessors) | +100 (instant blacklist) | Compare changes during merge |
| Signature verification failure on transport message | +50 | Existing transport auth |
| Duplicate `source_node_id` from different EndpointIds | +50 | Peer table cross-reference |
| Anomalous sync volume (>10x average for the link) | +10 | Sliding window rate tracking |
| Repeated connection attempts after rejection | +5 | Connection rate limiter |

**Threshold**: Configurable, default 100 (instant blacklist on cryptographic proof of misbehavior, gradual accumulation for behavioral anomalies).

**Score decay**: Scores decay at 1 point per hour to account for transient network issues that may trigger false positives.

**Local blacklist persistence**: Stored in the local redb database. Survives node restart. Cleared only by explicit operator action or re-enrollment.

**Key property**: No consensus or network communication required. A node can protect itself immediately.

### Layer 3: Threshold-Based Revocation Voting (Cell-Level Ejection)

**Mechanism**: Any formation member can propose a revocation. A revocation takes effect when `k` out of `n` formation members (or cell members, for cell-scoped revocation) have signed the proposal. The votes and final revocation are stored as CRDT state and propagate via normal sync.

**CRDT schema**:

```
revocation_proposals/
  {target_node_id}/
    proposer:    EndpointId
    reason:      String        (human-readable + anomaly enum)
    evidence:    Vec<u8>       (optional: equivocation proof)
    timestamp:   u64
    votes/
      {voter_id}: SignedVote   (Ed25519 signature over proposal hash)
    status:      Pending | Enacted | Expired
```

**Threshold calculation**:
- **Cell-scoped**: `ceil(cell_size * 2/3)` — e.g., 2-of-3, 3-of-4, 5-of-7
- **Formation-wide**: `ceil(formation_size * 0.5) + 1` — simple majority + 1
- **Authority override**: A single ADMIN or ENROLL-tier node can enact revocation immediately (no threshold needed)

**Vote weighting**: All votes are equal weight. Tier-based weighting was considered but rejected to prevent a compromised high-tier node from blocking revocations.

**Expiration**: Proposals expire after 24 hours if threshold is not reached. This prevents stale accusations from accumulating across long partitions.

**Enactment**: When vote count reaches threshold:
1. `status` transitions to `Enacted`.
2. The target's `EndpointId` is added to the formation's revocation set (Layer 4).
3. MLS removal is initiated if MLS is active (Layer 5).
4. All nodes that receive the enacted revocation via CRDT sync immediately drop connections to the target.

**False accusation resistance**: Requiring `ceil(2/3)` of a cell means an attacker must compromise a supermajority of the cell to falsely eject an honest node. For the standard 3-7 member cell, this requires 2-5 compromised nodes — at which point the cell is already majority-compromised.

**Reference**: Chan, Perrig, Song, "Distribution and Revocation of Cryptographic Keys in Sensor Networks," IEEE TDSC 2007; Becher et al., "Distributed Node Revocation," IEEE 2007.

### Layer 4: CRDT Revocation Tombstones (Mesh-Wide Propagation)

**Mechanism**: Formalize the existing revocation map from ADR-006 as a remove-wins CRDT set. Once a node's EndpointId is in the revocation set, it cannot be removed — only re-enrollment with a new keypair is possible.

**CRDT schema** (extending existing `formation_state`):

```
revocations/
  {endpoint_id}/
    revoked_at:      u64           (timestamp)
    revoked_by:      EndpointId    (who enacted)
    reason:          RevocationReason (enum)
    evidence_hash:   Option<[u8; 32]> (hash of equivocation proof)
```

**RevocationReason enum**:
```rust
pub enum RevocationReason {
    CryptographicProof,     // Equivocation or signature forgery
    ThresholdVote,          // Cell/formation voted to eject
    AuthorityDecision,      // ADMIN/ENROLL node decision
    CertificateExpired,     // Passive revocation (ADR-048)
    TransitiveRevocation,   // Delegator was revoked (Layer 4b)
    OperatorManual,         // Manual operator action
}
```

**Propagation**: Via normal Automerge sync. Remove-wins semantics ensure convergence — if any partition revokes a node, the revocation persists after partition healing.

**Enforcement**: On every sync message receive, check `author_id` against the revocation set. On every connection attempt, check the peer's EndpointId. Revoked peers are disconnected and their pending changes are discarded.

**Transitive revocation (Layer 4b)**: When a node with `ENROLL` capability is revoked, all nodes whose enrollment certificate was signed by the revoked node are also revoked. Implementation:

```
For each node N in formation:
  If N.enrollment_cert.issuer == revoked_node.endpoint_id:
    Add N to revocation set with reason TransitiveRevocation
```

Transitively revoked nodes can re-enroll with any remaining ENROLL-capable node to obtain a fresh certificate.

**Reference**: p2panda, "Convergent Offline-First Access Control CRDT," 2025; Policy-CRDT, "Remove-Wins Strategy for Convergent Access Control," 2025.

### Layer 5: MLS Epoch Advancement (Forward Secrecy)

**Mechanism**: When a revocation is enacted, the MLS group (per ADR-044) processes a Remove proposal and advances to a new epoch. The removed member cannot derive the new epoch's key material.

**Behavior during partition**: MLS Commits require total ordering within the group. During a partition:
- The partition containing the majority of the MLS group can advance the epoch.
- The minority partition continues using the pre-revocation epoch (the compromised node can still decrypt in this partition).
- On partition healing, the minority side processes the Remove Commit and advances to the new epoch. Traffic sent during the partition in the minority side remains readable by the compromised node — this is an accepted limitation.

**Mitigation for partition limitation**: Combine with Layer 1 (signed changes). Even if the compromised node can decrypt traffic during a partition, it cannot inject unsigned/invalid CRDT state that would survive merge.

**Reference**: RFC 9420 (MLS Protocol); RFC 9750 (MLS Architecture); Ink & Switch Keyhive (BeeKEM protocol).

## Operational Procedures

### Commander-Initiated Ejection

An operator with ADMIN access can eject a node immediately:

1. Operator issues `eject <endpoint_id> --reason "captured by adversary"` via CLI or admin API.
2. The local node writes a revocation tombstone with `AuthorityDecision` reason.
3. MLS Remove proposal is issued.
4. Revocation propagates via CRDT sync to all reachable peers.

### Automated Detection and Ejection

1. Node A detects cryptographic misbehavior from Node B (Layer 2).
2. Node A locally blacklists Node B (instant, no consensus).
3. Node A writes a revocation proposal to the CRDT (Layer 3).
4. Other cell members receive the proposal via sync and verify the evidence.
5. If evidence is cryptographically valid (equivocation proof), members auto-vote.
6. When threshold is reached, revocation is enacted.

### Post-Partition Reconciliation

1. Partition A revoked Node X; Partition B did not.
2. Partitions reconnect and sync CRDT state.
3. Partition B receives the revocation tombstone (remove-wins).
4. Partition B immediately drops connections to Node X.
5. MLS epoch advances to exclude Node X from future traffic.
6. Any CRDT changes authored by Node X during the partition are retained but flagged with `revoked_author: true` for operator review.

## Consequences

### Benefits

- **No single point of failure** — Ejection works without any central authority being reachable.
- **Immediate local response** — A node can protect itself in milliseconds via behavioral blacklisting.
- **Convergent** — CRDT-based revocation guarantees all honest nodes eventually agree on who is revoked.
- **Cryptographically verifiable** — Equivocation proofs are non-repudiable evidence of compromise.
- **Defense-in-depth** — Five independent layers; compromise of one does not defeat the others.

### Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| False accusation by colluding nodes | 2/3 supermajority threshold for cell-level ejection |
| Revocation storm (cascading transitive revocations) | Rate-limit transitive revocations; require operator confirmation for >3 cascading ejections |
| Compromised node floods revocation proposals | Proposals require valid formation membership; each node can have at most one active proposal |
| Partition delays revocation propagation | Local blacklisting provides immediate protection; CRDT convergence handles the rest |
| MLS epoch can't advance during partition | Accepted limitation; signed CRDT ops (Layer 1) prevent state corruption even if confidentiality is temporarily degraded |

### Trade-offs

- **Availability vs. security**: Short certificate TTLs improve security but risk excluding legitimate nodes during extended disconnections. Operators must tune `auth_interval_hours` and `grace_period_hours` per mission profile.
- **Automation vs. control**: Fully automated ejection (auto-vote on cryptographic evidence) is faster but risks false positives from implementation bugs. Recommended: auto-vote only on cryptographic proof (equivocation, invalid signatures); require manual confirmation for behavioral anomalies.
- **Simplicity vs. completeness**: This ADR deliberately excludes Byzantine fault tolerance for CRDT merge semantics (ADR-014 scopes this out). A fully Byzantine-tolerant CRDT (e.g., Blocklace) would provide stronger guarantees but requires replacing Automerge.

## Implementation Phases

### Phase 1: Foundation (Closes ADR-006 Open Questions)

1. Formalize revocation tombstone CRDT schema
2. Enforce revocation checks on sync receive and connection
3. Implement commander-initiated ejection via admin API
4. Implement transitive revocation for ENROLL delegates

### Phase 2: Detection

5. Implement signed CRDT operations (`SignedChange` wrapper)
6. Add signature verification to sync receive path
7. Implement equivocation detection on merge
8. Implement local behavioral scoring and blacklisting

### Phase 3: Decentralized Ejection

9. Implement threshold revocation voting CRDT
10. Add auto-vote on cryptographic evidence
11. Wire revocation enactment to MLS Remove (when MLS is implemented per ADR-044)

### Phase 4: Operational Hardening

12. Add revocation audit trail and forensic export
13. Implement post-partition reconciliation procedures
14. Add monitoring/alerting for revocation events
15. Operational testing with red-team exercises

## References

### Academic

- Kleppmann, "Making CRDTs Byzantine Fault Tolerant," PaPoC 2022. [PDF](https://martin.kleppmann.com/papers/bft-crdt-papoc22.pdf)
- Shapiro et al., "The Blocklace: A Byzantine-repelling Universal CRDT," arXiv 2402.08068, 2024. [Paper](https://arxiv.org/abs/2402.08068)
- Chan, Perrig, Song, "Distribution and Revocation of Cryptographic Keys in Sensor Networks," IEEE TDSC, 2007. [PDF](https://netsec.ethz.ch/publications/papers/noderevoke-journal.pdf)
- Becher, Benenson, Dornseif, "Distributed Node Revocation based on Cooperative Security," IEEE, 2007. [IEEE](https://ieeexplore.ieee.org/document/4428759)
- Rasheed, Mahapatra, "Survey on key revocation in wireless sensor networks," JNCA, 2016. [ScienceDirect](https://www.sciencedirect.com/science/article/abs/pii/S1084804516000333)

### Standards

- RFC 9420: The Messaging Layer Security (MLS) Protocol. [RFC](https://datatracker.ietf.org/doc/html/rfc9420)
- RFC 9750: The MLS Architecture. [RFC](https://datatracker.ietf.org/doc/rfc9750/)

### Implementations

- Ink & Switch, "Keyhive: Local-first access control," 2025. [Project](https://www.inkandswitch.com/keyhive/notebook/)
- p2panda, "Convergent Offline-First Access Control CRDT," 2025. [Blog](https://p2panda.org/2025/08/27/notes-convergent-access-control-crdt.html)
- davidrusu/bft-crdts — Rust BFT CRDT implementation. [GitHub](https://github.com/davidrusu/bft-crdts)

### Internal

- ADR-006: Security, Authentication, and Authorization
- ADR-044: End-to-End Encryption and Key Management
- ADR-048: Membership Certificates and Tactical Trust
- ADR-034: Record Deletion and Tombstone Management
- SPEC-005: Security Specification
