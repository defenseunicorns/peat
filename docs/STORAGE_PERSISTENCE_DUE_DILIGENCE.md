# Storage and Persistence Layer Due Diligence

**Purpose**: Evaluate embedded database options for CAP Protocol's Automerge + Iroh storage layer
**Context**: ADR-011 E11.2 - Production storage implementation
**Date**: 2025-11-12
**Status**: Research Phase

---

## Executive Summary

CAP Protocol needs a production-grade embedded storage layer to persist Automerge CRDT documents on tactical edge devices. This document evaluates options for replacing Ditto's proprietary storage with an open-source alternative.

**Key Requirements**:
- Embedded (no separate server process)
- Crash-safe persistence (tactical power loss common)
- Efficient key-value storage for Automerge documents
- Support for range queries (geohash proximity, collection scans)
- Low memory footprint (runs on constrained devices)
- Battle-tested in production
- Rust-native bindings

**TL;DR Recommendation**: **RocksDB** for production, **Redb** as promising Rust-native alternative

---

## Requirements Analysis

### Functional Requirements

| Requirement | Priority | Description |
|-------------|----------|-------------|
| **Key-Value Store** | Critical | Primary access pattern: get/put/delete by document ID |
| **Range Queries** | Critical | Scan collections, geohash prefix queries |
| **ACID Transactions** | High | Atomic updates across multiple documents |
| **Embedded** | Critical | No separate server, library only |
| **Crash Recovery** | Critical | Survive power loss without corruption |
| **Compaction** | High | Reclaim space from deleted/old documents |
| **Snapshots** | Medium | Backup without stopping writes |
| **Multi-Collection** | High | Logical separation (cells, nodes, capabilities) |

### Non-Functional Requirements

| Requirement | Priority | Target | Description |
|-------------|----------|--------|-------------|
| **Write Throughput** | High | >10K writes/sec | Frequent CRDT updates |
| **Read Latency** | Critical | <1ms p50, <10ms p99 | Real-time operations coordination |
| **Memory Footprint** | High | <100MB | Tactical edge devices (Raspberry Pi 4) |
| **Disk Usage** | Medium | <1GB | SD card storage |
| **Startup Time** | Medium | <1 second | Fast node initialization |
| **License** | Critical | Apache-2.0 or MIT | GOTS compatibility |
| **Maturity** | High | Production-proven | Safety-critical deployment |
| **Rust Support** | High | Native or solid bindings | Type safety, no FFI overhead |

---

## Storage Options Evaluated

### Option 1: RocksDB ⭐ **RECOMMENDED for Production**

**Description**: LSM-tree based key-value store from Facebook/Meta, used in production by hundreds of companies.

**Rust Bindings**: [`rust-rocksdb`](https://github.com/rust-rocksdb/rust-rocksdb) - Well-maintained, 1.5K+ stars

#### Pros
- ✅ **Battle-tested**: Billions of devices (WhatsApp, LinkedIn, Facebook)
- ✅ **Performance**: Optimized for write-heavy workloads (10K+ writes/sec)
- ✅ **Feature-rich**: Column families (collections), snapshots, compaction
- ✅ **Well-documented**: Extensive tuning guides
- ✅ **Active development**: Meta actively maintains C++ and Rust bindings
- ✅ **Tactical-proven**: Used in edge/mobile applications
- ✅ **Compression**: LZ4/Snappy/Zstd reduce disk usage
- ✅ **Crash-safe**: WAL (write-ahead log) ensures durability

#### Cons
- ⚠️ **C++ dependency**: FFI overhead, harder debugging
- ⚠️ **Memory usage**: Can use significant RAM for write buffers (tunable)
- ⚠️ **Complexity**: Many tuning knobs (can be overwhelming)
- ⚠️ **Build time**: Large C++ codebase slows compilation

#### Performance Characteristics
```
Writes:     100K+ ops/sec (sequential), 10K+ ops/sec (random)
Reads:      <1ms p50, <5ms p99 (with proper tuning)
Memory:     50-200MB baseline + write buffers
Disk:       Efficient with compression (60-80% reduction)
Startup:    Fast (<100ms for small DBs, ~1s for large)
```

#### Example Usage
```rust
use rocksdb::{DB, Options, ColumnFamilyDescriptor};

// Create DB with column families (collections)
let mut opts = Options::default();
opts.create_if_missing(true);
opts.create_missing_column_families(true);

let cfs = vec![
    ColumnFamilyDescriptor::new("cells", Options::default()),
    ColumnFamilyDescriptor::new("nodes", Options::default()),
    ColumnFamilyDescriptor::new("capabilities", Options::default()),
];

let db = DB::open_cf_descriptors(&opts, "/var/cap/data", cfs)?;

// Get collection handle
let cells_cf = db.cf_handle("cells").unwrap();

// Upsert document (Automerge bytes)
let doc_bytes = automerge.save();
db.put_cf(cells_cf, b"cell-1", &doc_bytes)?;

// Retrieve document
let stored = db.get_cf(cells_cf, b"cell-1")?.unwrap();
let doc = Automerge::load(&stored)?;

// Range query (all cells with prefix)
let iter = db.prefix_iterator_cf(cells_cf, b"cell-");
for (key, value) in iter {
    println!("Found: {:?}", key);
}
```

#### Deployment Considerations
- **Tuning**: Use default settings initially, optimize after profiling
- **Backup**: Use `checkpoint()` for consistent snapshots
- **Monitoring**: Track `get_statistics()` for performance metrics
- **Disk space**: Enable compression, periodic compaction

**Verdict**: ✅ **Production-ready** - Best choice for E11.2 MVP

---

### Option 2: Redb 🚀 **PROMISING Rust-Native Alternative**

**Description**: Pure Rust embedded database inspired by LMDB, designed for simplicity and safety.

**Repository**: [`redb`](https://github.com/cberner/redb) - 3K+ stars, active development

#### Pros
- ✅ **Pure Rust**: No FFI, excellent error messages, compile-time safety
- ✅ **Simple API**: Minimal tuning, easy to use correctly
- ✅ **ACID transactions**: Full transactional semantics
- ✅ **Crash-safe**: Copy-on-write B-tree (inspired by LMDB)
- ✅ **Zero-copy reads**: Memory-mapped files (fast reads)
- ✅ **Type-safe**: Rust type system enforces correctness
- ✅ **Small footprint**: <1MB library, minimal dependencies
- ✅ **Fast compile**: Pure Rust compiles quickly

#### Cons
- ⚠️ **Less mature**: Newer project (2021), fewer production deployments
- ⚠️ **Write performance**: MVCC overhead (slower than RocksDB for write-heavy)
- ⚠️ **Memory-mapped**: May not work well on all filesystems (SD cards?)
- ⚠️ **Limited tooling**: No CLI tools, fewer tuning options
- ⚠️ **Smaller community**: Less Stack Overflow/GitHub issue history

#### Performance Characteristics
```
Writes:     10K-50K ops/sec (depending on transaction size)
Reads:      <100μs p50, <1ms p99 (zero-copy)
Memory:     Very low (~10-50MB baseline)
Disk:       Efficient but no compression
Startup:    Instant (memory-mapped)
```

#### Example Usage
```rust
use redb::{Database, TableDefinition, ReadableTable};

const CELLS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("cells");

// Open database
let db = Database::create("/var/cap/data.redb")?;

// Write transaction
let write_txn = db.begin_write()?;
{
    let mut table = write_txn.open_table(CELLS_TABLE)?;
    let doc_bytes = automerge.save();
    table.insert("cell-1", doc_bytes.as_slice())?;
}
write_txn.commit()?;

// Read transaction
let read_txn = db.begin_read()?;
let table = read_txn.open_table(CELLS_TABLE)?;
if let Some(stored) = table.get("cell-1")? {
    let doc = Automerge::load(stored.value())?;
}

// Range query
let range = table.range("cell-".."cell.".into())?;
for result in range {
    let (key, value) = result?;
    println!("Found: {}", key.value());
}
```

#### Deployment Considerations
- **Transactions**: Keep write transactions short (lock-free reads, but writes block)
- **Backup**: Copy-on-write makes snapshots trivial
- **Memory**: Memory-mapped, ensure sufficient virtual address space
- **Compaction**: Automatic, triggered on commit

**Verdict**: ✅ **Excellent for MVP/POC** - Consider for E11.2 if pure Rust preferred

---

### Option 3: Sled

**Description**: Pure Rust embedded database with focus on correctness and performance.

**Repository**: [`sled`](https://github.com/spacejam/sled) - 8K+ stars

#### Pros
- ✅ **Pure Rust**: No C++ dependencies
- ✅ **Fast**: Optimized for SSD performance
- ✅ **ACID**: Full transactional semantics
- ✅ **Popular**: Many production users

#### Cons
- ❌ **Unmaintained**: No commits since 2022, maintainer moved to other projects
- ❌ **Known bugs**: Outstanding issues with data corruption reports
- ❌ **API instability**: Never reached 1.0
- ❌ **No future**: Project effectively abandoned

**Verdict**: ❌ **Do not use** - Too risky for safety-critical system

---

### Option 4: SQLite (with rusqlite)

**Description**: Most widely deployed SQL database, with Rust bindings.

**Rust Bindings**: [`rusqlite`](https://github.com/rusqlite/rusqlite) - 3K+ stars

#### Pros
- ✅ **Ultra-mature**: Decades of production use
- ✅ **Extensively tested**: 100% branch coverage, billion+ devices
- ✅ **SQL queries**: Flexible query capabilities
- ✅ **Well-documented**: Excellent documentation
- ✅ **Tooling**: sqlite3 CLI for debugging

#### Cons
- ❌ **SQL overhead**: CAP doesn't need relational model
- ❌ **Impedance mismatch**: Blob storage in SQL feels wrong
- ❌ **Write performance**: Row-based storage not optimal for CRDT blobs
- ❌ **C dependency**: FFI overhead
- ❌ **Complexity**: Must design schema, migrations, indexes

**Verdict**: ⚠️ **Overkill** - SQL capabilities unnecessary for key-value workload

---

### Option 5: LMDB (with lmdb-rs)

**Description**: Lightning Memory-Mapped Database, ultra-fast embedded DB.

**Rust Bindings**: [`lmdb-rs`](https://github.com/vhbit/lmdb-rs) or [`heed`](https://github.com/meilisearch/heed)

#### Pros
- ✅ **Extremely fast reads**: Memory-mapped, zero-copy
- ✅ **ACID**: Full transactional semantics
- ✅ **Battle-tested**: Used in OpenLDAP, Postfix, etc.
- ✅ **Simple**: Minimal API surface

#### Cons
- ⚠️ **Fixed database size**: Must pre-allocate (annoying on SD cards)
- ⚠️ **C dependency**: FFI overhead
- ⚠️ **Write contention**: Single writer (not ideal for concurrent workloads)
- ⚠️ **Less popular in Rust**: Fewer examples, smaller community

**Verdict**: ⚠️ **Alternative** - Good if read performance is critical, but fixed size is awkward

---

### Option 6: IndexedDB (via web)

**Description**: Browser-based storage API (via WASM).

#### Pros
- ✅ **No dependencies**: Built into browsers
- ✅ **Async**: Non-blocking I/O

#### Cons
- ❌ **Browser-only**: Doesn't work for native tactical devices
- ❌ **Storage limits**: Browser quotas (5-50MB)
- ❌ **Not suitable**: Wrong deployment target

**Verdict**: ❌ **Not applicable** - CAP targets native edge devices, not browsers

---

## Comparison Matrix

| Feature | RocksDB | Redb | Sled | SQLite | LMDB |
|---------|---------|------|------|--------|------|
| **License** | Apache-2.0 | Apache-2.0/MIT | Apache-2.0 | Public Domain | OpenLDAP |
| **Language** | C++ (Rust bindings) | Pure Rust | Pure Rust | C (Rust bindings) | C (Rust bindings) |
| **Maturity** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐ (unmaintained) | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ |
| **Write Performance** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐ |
| **Read Performance** | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ |
| **Memory Footprint** | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ |
| **Ease of Use** | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐ |
| **ACID Transactions** | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Column Families** | ✅ | ✅ (tables) | ✅ (trees) | ✅ (tables) | ✅ (databases) |
| **Range Queries** | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Compression** | ✅ | ❌ | ❌ | ❌ | ❌ |
| **Production Usage** | Billions | Thousands | Thousands | Billions | Millions |
| **Active Development** | ✅ | ✅ | ❌ | ✅ | ✅ (minimal) |

---

## Recommended Decision Tree

```
Start
  ↓
Need battle-tested production reliability?
  ├─ YES → RocksDB (Meta, billions of devices)
  └─ NO → Continue
       ↓
       Prefer pure Rust (no C++ dependencies)?
         ├─ YES → Redb (newer but solid)
         └─ NO → RocksDB (FFI acceptable)
              ↓
              Need SQL query flexibility?
                ├─ YES → SQLite (but probably overkill)
                └─ NO → RocksDB
                     ↓
                     Optimize for read latency over write throughput?
                       ├─ YES → LMDB or Redb
                       └─ NO → RocksDB
```

---

## Recommendations

### For E11.2 MVP (Weeks 3-4): **RocksDB**

**Rationale**:
1. **Production-proven**: Used in similar embedded/edge scenarios (WhatsApp, mobile apps)
2. **Performance**: Exceeds requirements (10K+ writes/sec, <1ms reads)
3. **Features**: Column families = collections, snapshots, compression
4. **Risk**: Low - battle-tested in billion+ device deployments
5. **Timeline**: Well-documented, existing examples, fast implementation

**Implementation Plan**:
```rust
// cap-protocol/src/storage/rocksdb_backend.rs

use rocksdb::{DB, Options, ColumnFamilyDescriptor};

pub struct RocksDbStore {
    db: Arc<DB>,
}

impl RocksDbStore {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // Tactical device tuning
        opts.set_write_buffer_size(16 * 1024 * 1024); // 16MB write buffer
        opts.set_max_write_buffer_number(3);
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);

        let cfs = vec![
            ColumnFamilyDescriptor::new("cells", Options::default()),
            ColumnFamilyDescriptor::new("nodes", Options::default()),
            ColumnFamilyDescriptor::new("capabilities", Options::default()),
            ColumnFamilyDescriptor::new("squad_summaries", Options::default()),
            ColumnFamilyDescriptor::new("platoon_summaries", Options::default()),
        ];

        let db = DB::open_cf_descriptors(&opts, path, cfs)?;

        Ok(Self { db: Arc::new(db) })
    }

    pub fn collection(&self, name: &str) -> Collection {
        Collection {
            db: self.db.clone(),
            cf_name: name.to_string(),
        }
    }
}

pub struct Collection {
    db: Arc<DB>,
    cf_name: String,
}

impl Collection {
    pub fn upsert(&self, doc_id: &str, doc: &Automerge) -> Result<()> {
        let cf = self.db.cf_handle(&self.cf_name)
            .ok_or_else(|| anyhow!("Collection not found"))?;
        let bytes = doc.save();
        self.db.put_cf(cf, doc_id.as_bytes(), &bytes)?;
        Ok(())
    }

    pub fn get(&self, doc_id: &str) -> Result<Option<Automerge>> {
        let cf = self.db.cf_handle(&self.cf_name)
            .ok_or_else(|| anyhow!("Collection not found"))?;
        if let Some(bytes) = self.db.get_cf(cf, doc_id.as_bytes())? {
            Ok(Some(Automerge::load(&bytes)?))
        } else {
            Ok(None)
        }
    }

    pub fn delete(&self, doc_id: &str) -> Result<()> {
        let cf = self.db.cf_handle(&self.cf_name)
            .ok_or_else(|| anyhow!("Collection not found"))?;
        self.db.delete_cf(cf, doc_id.as_bytes())?;
        Ok(())
    }

    pub fn scan(&self) -> impl Iterator<Item = Result<(String, Automerge)>> {
        let cf = self.db.cf_handle(&self.cf_name).unwrap();
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        iter.map(|result| {
            let (key, value) = result?;
            let doc_id = String::from_utf8(key.to_vec())?;
            let doc = Automerge::load(&value)?;
            Ok((doc_id, doc))
        })
    }

    // Geohash proximity query
    pub fn query_geohash_prefix(&self, prefix: &str) -> Vec<(String, Automerge)> {
        let cf = self.db.cf_handle(&self.cf_name).unwrap();
        let iter = self.db.prefix_iterator_cf(cf, prefix.as_bytes());

        iter.filter_map(|(key, value)| {
            let doc_id = String::from_utf8(key.to_vec()).ok()?;
            let doc = Automerge::load(&value).ok()?;
            Some((doc_id, doc))
        }).collect()
    }
}
```

**Timeline**: 3-4 days implementation + 2-3 days testing

---

### For E11.3+ (Future): **Consider Redb**

If pure Rust becomes a priority (easier debugging, no FFI, faster compilation), **Redb** is a strong candidate once it has more production usage.

**When to switch**:
- After E11.2 benchmarking shows RocksDB's write performance is more than needed
- If C++ dependency causes deployment issues
- If Redb accumulates more production deployments (monitor for 6-12 months)

---

## Integration with Automerge + Iroh

### Storage Layer Responsibilities

```
┌─────────────────────────────────────────────┐
│         cap-protocol (Business Logic)        │
├─────────────────────────────────────────────┤
│           Automerge (CRDT Engine)            │
├─────────────────────────────────────────────┤
│      Storage Trait (Backend Abstraction)     │
├─────────────────────────────────────────────┤
│  RocksDbStore  │  RedbStore  │  DittoStore  │ (Implementations)
├─────────────────────────────────────────────┤
│         Filesystem / Operating System        │
└─────────────────────────────────────────────┘
```

### Trait-Based Abstraction

```rust
// cap-protocol/src/storage/mod.rs

pub trait StorageBackend: Send + Sync {
    fn collection(&self, name: &str) -> Box<dyn Collection>;
    fn list_collections(&self) -> Vec<String>;
    fn flush(&self) -> Result<()>;
    fn close(self) -> Result<()>;
}

pub trait Collection: Send + Sync {
    fn upsert(&self, doc_id: &str, doc: &Automerge) -> Result<()>;
    fn get(&self, doc_id: &str) -> Result<Option<Automerge>>;
    fn delete(&self, doc_id: &str) -> Result<()>;
    fn scan(&self) -> Box<dyn Iterator<Item = Result<(String, Automerge)>>>;
    fn query_geohash_prefix(&self, prefix: &str) -> Vec<(String, Automerge)>;
}

// Runtime selection via environment variable
pub fn create_storage_backend(config: &Config) -> Result<Box<dyn StorageBackend>> {
    match config.storage_backend.as_str() {
        "rocksdb" => Ok(Box::new(RocksDbStore::new(&config.data_path)?)),
        "redb" => Ok(Box::new(RedbStore::new(&config.data_path)?)),
        "ditto" => Ok(Box::new(DittoStore::new(config)?)),
        other => Err(anyhow!("Unknown storage backend: {}", other)),
    }
}
```

**Configuration**:
```bash
# Environment variables
export CAP_STORAGE_BACKEND=rocksdb  # or redb, ditto
export CAP_DATA_PATH=/var/cap/data

# Or in config file
[storage]
backend = "rocksdb"
data_path = "/var/cap/data"
```

---

## Testing Strategy

### Unit Tests
- Basic CRUD operations
- Range queries
- Concurrent access
- Error handling

### Integration Tests
- Automerge document lifecycle
- Collection management
- Geohash queries
- Crash recovery (kill -9 during write)

### Performance Benchmarks
- Write throughput (ops/sec)
- Read latency (p50, p90, p99)
- Memory usage under load
- Disk space growth

### Tactical Device Testing
- Raspberry Pi 4 (4GB RAM)
- SD card I/O characteristics
- Power loss recovery
- Multi-hour soak tests

---

## Migration Path from Ditto

### Phase 1: Trait Abstraction (E11.2 Week 1)
1. Define `StorageBackend` and `Collection` traits
2. Implement traits for Ditto (existing code)
3. Update all callsites to use trait
4. Tests continue to pass with Ditto backend

### Phase 2: RocksDB Implementation (E11.2 Week 2)
1. Implement `RocksDbStore` + `RocksDbCollection`
2. Add unit tests for RocksDB backend
3. Add integration tests with Automerge
4. Benchmark against Ditto

### Phase 3: Production Readiness (E11.2 Week 3-4)
1. Add TTL support (document expiration)
2. Add backup/restore functionality
3. Performance tuning for tactical devices
4. Documentation and deployment guides

### Phase 4: Gradual Rollout (E11.3+)
1. Default remains Ditto
2. Enable RocksDB via feature flag
3. A/B testing in non-critical scenarios
4. Monitor for 2-4 weeks
5. Switch default to RocksDB
6. Eventually remove Ditto dependency

---

## Risk Assessment

### RocksDB Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| C++ build complexity | Medium | Low | Use pre-built binaries, Docker |
| Memory usage higher than expected | Medium | Medium | Tune write buffers, test on Pi 4 |
| Crash on SD card corruption | Low | High | Use WAL, test power loss scenarios |
| Performance worse than Ditto | Low | Medium | Benchmark early, tune settings |

### Redb Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Less mature, unknown bugs | Medium | High | Extensive testing, staged rollout |
| Memory-mapped doesn't work on SD | Low | High | Test on Pi 4 with SD card |
| Write performance insufficient | Low | Medium | Benchmark early, switch to RocksDB |

---

## Success Criteria

### Functional
- ✅ All existing tests pass with new storage backend
- ✅ Document CRUD operations work correctly
- ✅ Geohash proximity queries return correct results
- ✅ Crash recovery works (survives kill -9 during write)
- ✅ Collections are isolated (no cross-collection leakage)

### Performance
- ✅ Write throughput: >10K ops/sec
- ✅ Read latency: p50 <1ms, p99 <10ms
- ✅ Memory usage: <100MB baseline
- ✅ Startup time: <2 seconds
- ✅ Disk usage: <500MB for 1000 documents with compression

### Operational
- ✅ Deployment documentation complete
- ✅ Backup/restore procedures documented
- ✅ Monitoring metrics defined
- ✅ Performance tuning guide available

---

## Next Steps

1. **Immediate (this branch)**:
   - Review this due diligence doc
   - Get team consensus on RocksDB choice
   - Finalize trait API design

2. **E11.2 Week 1**:
   - Create trait abstraction
   - Refactor existing code to use trait
   - All tests passing with Ditto backend

3. **E11.2 Week 2**:
   - Implement RocksDB backend
   - Unit tests for RocksDB
   - Integration tests with Automerge

4. **E11.2 Week 3-4**:
   - Production features (TTL, backup)
   - Performance tuning
   - Tactical device testing
   - Documentation

---

## References

1. [RocksDB GitHub](https://github.com/facebook/rocksdb)
2. [rust-rocksdb](https://github.com/rust-rocksdb/rust-rocksdb)
3. [Redb GitHub](https://github.com/cberner/redb)
4. [Sled GitHub](https://github.com/spacejam/sled)
5. [LMDB](https://www.symas.com/lmdb)
6. [Heed (LMDB Rust bindings)](https://github.com/meilisearch/heed)
7. [Embedded Database Comparison](https://github.com/erikgrinaker/toydb/blob/master/docs/architecture.md)
8. ADR-011: Ditto vs Automerge+Iroh
9. E11.1: Automerge Storage POC

---

**Last Updated**: 2025-11-12
**Next Review**: After team consensus + E11.2 Week 1 implementation
**Owner**: Core development team
