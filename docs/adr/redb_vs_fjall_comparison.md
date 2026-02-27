# Rust Embedded Database Comparison: redb vs fjall

## Executive Summary

For PEAT Protocol's Automerge + Iroh implementation, **redb is the recommended choice**, primarily because Iroh already uses redb for its storage layer. This provides:
- Proven integration patterns with CRDT-sync use cases
- Battle-tested async/sync bridging patterns documented by Iroh team
- Ecosystem alignment with your primary networking dependency

However, fjall offers compelling advantages for write-heavy scenarios and should remain on the radar for specialized use cases.

---

## Developer & Commercial Assessment

### redb

| Aspect | Details |
|--------|---------|
| **Primary Author** | Christopher Berner (cberner) |
| **Background** | BioAI at OpenAI, previously robotics lead. UC Berkeley education. San Francisco based. |
| **Commercial Aspiration** | Hobby/Open Source - no commercial entity behind it |
| **License** | MIT OR Apache-2.0 |
| **Development Style** | Solo maintainer with community contributions |

**Notable**: Christopher appears to be a senior ML/robotics engineer at OpenAI. redb is a side project, but maintained to production quality.

### fjall

| Aspect | Details |
|--------|---------|
| **Primary Author** | Marvin (marvin-j97) |
| **Background** | Software developer & entrepreneur in Hannover, Germany. CTO/Co-founder of Zoop (comicbook platform) |
| **Commercial Aspiration** | Has GitHub Sponsors enabled; Sponsored by Orbitinghail (SQLSync) |
| **License** | MIT OR Apache-2.0 |
| **Development Style** | Very active solo developer with high commit velocity |

**Notable**: Marvin is younger and more actively focused on fjall as a primary project. The SQLSync sponsorship suggests enterprise interest.

---

## Repository Health

### redb

| Metric | Value |
|--------|-------|
| **Stars** | 4,100 |
| **Forks** | 190 |
| **Watchers** | 27 |
| **Open Issues** | 10 |
| **Open PRs** | 0 |
| **Contributors** | 34 |
| **Commits** | 1,400 |
| **Releases** | 59 |
| **Used By** | 1,300+ projects |
| **Latest Release** | v3.1.0 (September 2025) |
| **File Format** | Stable since v1.0, upgrade path provided |

**Assessment**: Mature, stable project with wide adoption. Low issue count indicates stability. Used by significant projects including Iroh.

### fjall

| Metric | Value |
|--------|-------|
| **Stars** | 1,400 |
| **Forks** | 53 |
| **Watchers** | 11 |
| **Open Issues** | 19 |
| **Open PRs** | 2 |
| **Contributors** | 13 |
| **Commits** | 1,818 |
| **Releases** | 80 |
| **Used By** | 112 projects |
| **Latest Release** | v2.11.2 (July 2025) |
| **File Format** | Stable since v2.0, v3.0 in development |

**Assessment**: Rapidly evolving project with very high commit velocity. More releases suggest faster iteration but potentially more breaking changes. Smaller but growing adoption.

---

## Performance Benchmarks

*Data from redb's official benchmarks on Ryzen 9950X3D with Samsung 9100 PRO NVMe*

| Operation | redb | fjall | RocksDB | lmdb | sled |
|-----------|------|-------|---------|------|------|
| **Bulk load** | 17,063ms | 18,619ms | 13,969ms | **9,232ms** | 24,971ms |
| **Individual writes** | **920ms** | 3,488ms | 2,432ms | 1,598ms | 2,701ms |
| **Batch writes** | 1,595ms | **353ms** | 451ms | 942ms | 853ms |
| **len()** | **0ms** | 1,181ms | 749ms | **0ms** | 1,573ms |
| **Random reads** | 1,138ms | 2,177ms | 2,911ms | **637ms** | 1,601ms |
| **Random range reads** | 1,174ms | 2,564ms | 2,734ms | **565ms** | 1,992ms |
| **16-thread reads** | 652ms | 963ms | 1,478ms | **216ms** | 690ms |
| **Removals** | 23,297ms | **6,004ms** | 6,900ms | 10,435ms | 11,088ms |
| **Uncompacted size** | 4.00 GiB | 1.00 GiB | **893 MiB** | 2.61 GiB | 2.13 GiB |
| **Compacted size** | 1.69 GiB | 1.00 GiB | **455 MiB** | 1.26 GiB | N/A |

### Key Performance Takeaways

**redb wins at:**
- Individual writes (3.8x faster than fjall)
- Random reads (1.9x faster)
- Range reads (2.2x faster)
- Constant-time len() operation
- Multi-threaded read scaling

**fjall wins at:**
- Batch writes (4.5x faster than redb)
- Removals/deletions (3.9x faster)
- Storage efficiency (built-in LZ4 compression)
- Write amplification (LSM advantage)

---

## Architecture Comparison

### redb - B+Tree Design

```
┌─────────────────────────────────────┐
│  Copy-on-Write B+Trees              │
├─────────────────────────────────────┤
│  • LMDB-inspired architecture       │
│  • MVCC with serializable isolation │
│  • Single-file database             │
│  • Memory-mapped I/O                │
│  • Zero-copy reads                  │
└─────────────────────────────────────┘
```

**Characteristics:**
- Read-optimized (B-tree random access)
- Excellent point lookups
- Predictable latency
- Higher write amplification
- Simpler crash recovery

### fjall - LSM-Tree Design

```
┌─────────────────────────────────────┐
│  Log-Structured Merge Tree (LSM)    │
├─────────────────────────────────────┤
│  • RocksDB-inspired architecture    │
│  • Column families (partitions)     │
│  • Multiple files per database      │
│  • Background compaction            │
│  • Built-in compression (LZ4)       │
└─────────────────────────────────────┘
```

**Characteristics:**
- Write-optimized (sequential writes)
- Excellent batch throughput
- Lower write amplification
- Better compression ratios
- Background maintenance overhead

---

## Feature Comparison

| Feature | redb | fjall |
|---------|------|-------|
| **100% Safe Rust** | ✅ | ✅ |
| **ACID Transactions** | ✅ | ✅ |
| **MVCC** | ✅ | ✅ |
| **Concurrent Readers** | ✅ | ✅ |
| **Single Writer** | ✅ | ✅ (+ OCC option) |
| **Compression** | ❌ | ✅ LZ4/zlib |
| **Column Families** | Tables | Partitions (keyspaces) |
| **Cross-partition Atomics** | ✅ | ✅ |
| **Savepoints/Rollback** | ✅ | ✅ |
| **Blob Separation** | ❌ | ✅ |
| **Custom Storage Backend** | ✅ | ❌ |
| **Multi-process Access** | ✅ (ReadOnly) | ❌ |
| **File Format Stability** | Stable | Stable (2.x) |
| **Key Size Limit** | 3.75 GiB | 64 KB |
| **Value Size Limit** | 3.75 GiB | 4 GB |

---

## PEAT-Specific Considerations

### Iroh Integration (Critical Factor)

**redb is used by Iroh** for:
- `iroh-docs` persistent storage
- `iroh-blobs` inline small blob storage
- Document sync state

The Iroh team has documented their async/sync bridging patterns:
```rust
// Pattern from Iroh: Handler thread with message passing
// for bridging async Tokio tasks with sync redb operations
```

This means proven patterns exist for the exact use case PEAT needs.

### CRDT Workload Analysis

For PEAT's Automerge document sync:

| Workload Pattern | Better Choice | Reasoning |
|-----------------|---------------|-----------|
| **Many small updates** | redb | Better individual write perf |
| **Large batch syncs** | fjall | 4.5x faster batch writes |
| **Point reads (get state)** | redb | 1.9x faster random reads |
| **Range scans (history)** | redb | 2.2x faster range reads |
| **Concurrent readers** | redb | Better multi-thread scaling |
| **Storage constrained** | fjall | Built-in compression |
| **High deletion rate** | fjall | 3.9x faster removals |

### Edge Device Considerations

| Factor | redb | fjall |
|--------|------|-------|
| **Memory footprint** | Lower | Higher (compaction buffers) |
| **CPU overhead** | Lower | Higher (compression, compaction) |
| **Flash wear** | Higher WAF | Lower WAF |
| **Predictable latency** | Better | Compaction spikes |
| **Build simplicity** | Simpler | More complex |

---

## Risk Assessment

### redb Risks
- **Single maintainer**: Christopher's OpenAI work could deprioritize redb
- **Slower iteration**: Stable but less frequent updates
- **No compression**: Larger storage footprint

### fjall Risks
- **Younger project**: Less battle-tested at scale
- **Faster-moving target**: More breaking changes
- **Solo developer risk**: Marvin could pivot to other projects
- **Less production validation**: 112 vs 1,300+ dependents

---

## Recommendation for PEAT

### Primary: redb

1. **Iroh alignment** - Already proven in the CRDT sync context
2. **Stability** - Mature, stable API and file format
3. **Read performance** - Better for typical C2 query patterns
4. **Predictable latency** - No compaction surprises
5. **Simpler architecture** - Easier to reason about

### When to Consider fjall

- If PEAT workload becomes heavily write-dominated
- If storage efficiency becomes critical constraint
- If batch sync operations dominate over individual updates
- If deletion-heavy patterns emerge (TTL expiration)

### Integration Path

```rust
// Recommended: Follow Iroh's pattern
// Use redb with handler thread for sync operations

use redb::{Database, TableDefinition};
use tokio::sync::mpsc;

// PEAT storage actor pattern (similar to Iroh)
struct StorageActor {
    db: Database,
    rx: mpsc::Receiver<StorageCommand>,
}
```

---

## References

- redb GitHub: https://github.com/cberner/redb
- fjall GitHub: https://github.com/fjall-rs/fjall
- Iroh async challenges: https://www.iroh.computer/blog/async-rust-challenges-in-iroh
- fjall-rs blog: https://fjall-rs.github.io/
- Benchmark source: https://github.com/cberner/redb/blob/master/crates/redb-bench/
