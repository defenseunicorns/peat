# Issue #80: Query Engine & Geohash Indexing - Specification

**Status**: Approved
**Priority**: HIGH (Critical Path)
**Timeline**: 3-4 weeks
**Issue**: https://github.com/kitplummer/hive/issues/80

## Executive Summary

Implement a query engine and geohash-based spatial indexing system for Automerge documents to enable location-based and field-based searches required for HIVE Protocol geographic discovery and capability-based operations.

## Context

### Current State
- ✅ **Automerge+Iroh Phase 6.2-7.1 Complete**: Document sync, partition detection, and partition lifecycle metrics working
- ✅ **Storage Layer**: AutomergeStore with RocksDB persistence
- ✅ **Beacon Data Structures**: `GeographicBeacon` struct exists in `discovery/geographic.rs`
- ✅ **Capability Query Logic**: `CapabilityQuery` implements in-memory filtering
- ❌ **No Query Engine**: Cannot query Automerge documents by field values
- ❌ **No Spatial Index**: Cannot efficiently find nearby nodes by location

### Problem Statement

HIVE Protocol nodes need to:
1. **Discover nearby nodes** by geographic location (ADR-002 requirement)
2. **Query nodes by capabilities** (e.g., "find sensors with fuel > 20%")
3. **Filter operational nodes** by status fields
4. **Paginate and sort** query results for C2 interfaces

Currently, these operations require:
- Full collection scans (O(n) for every query)
- In-memory filtering after loading all documents
- No spatial optimization for geographic queries

## Use Cases

### UC1: Geographic Beacon Discovery (Priority: CRITICAL)
**Actor**: Platform node during discovery phase
**Goal**: Find nearby platforms to form a squad

**Scenario**:
```rust
// Node at (37.7749, -122.4194) wants to find nearby beacons
let my_geohash = "9q8yyk8";  // precision 7 (~153m cell)

// Query beacons in my cell + 8 neighboring cells
let nearby_beacons = geohash_index.find_near(37.7749, -122.4194)?;

// Filter by operational status and sort by distance
let results = Query::new(beacons_collection)
    .filter_by_ids(&nearby_beacons)  // Use spatial index results
    .where_eq("operational", Value::Bool(true))
    .order_by("timestamp", SortOrder::Desc)
    .limit(10)
    .execute()?;
```

**Expected Outcome**: Returns 10 most recent operational beacons within ~500m

**Performance Requirement**: <100ms for 100 documents

---

### UC2: Capability-Based Node Discovery (Priority: HIGH)
**Actor**: Squad leader or C2 operator
**Goal**: Find nodes with specific capabilities for mission assignment

**Scenario**:
```rust
// Find sensor platforms with sufficient fuel
let results = Query::new(nodes_collection)
    .where_eq("operational", Value::Bool(true))
    .where_gt("fuel_percent", Value::Int(20))
    .where_contains("capabilities", "sensor")  // Array contains check
    .order_by("fuel_percent", SortOrder::Desc)
    .limit(5)
    .execute()?;
```

**Expected Outcome**: Returns top 5 sensor platforms sorted by fuel level

**Performance Requirement**: <100ms for 100 documents

---

### UC3: Squad Status Monitoring (Priority: MEDIUM)
**Actor**: Platoon leader
**Goal**: Monitor all squads and their readiness status

**Scenario**:
```rust
// Find squads with low readiness
let results = Query::new(squads_collection)
    .where_lt("readiness_score", Value::Float(0.7))
    .where_eq("mission_status", Value::String("active"))
    .order_by("readiness_score", SortOrder::Asc)
    .execute()?;
```

**Expected Outcome**: Returns all active squads with readiness < 70%, worst first

**Performance Requirement**: <200ms for 50 squads

---

### UC4: Pagination for C2 Interface (Priority: MEDIUM)
**Actor**: C2 operator viewing node list
**Goal**: Browse large node lists with pagination

**Scenario**:
```rust
// Page 3 of all nodes, 20 per page
let results = Query::new(nodes_collection)
    .order_by("node_id", SortOrder::Asc)
    .offset(40)  // Skip first 2 pages (40 nodes)
    .limit(20)   // Get page 3
    .execute()?;
```

**Expected Outcome**: Returns nodes 41-60 sorted by ID

**Performance Requirement**: <100ms per page

---

### UC5: Multi-Field Complex Query (Priority: MEDIUM)
**Actor**: Mission planner
**Goal**: Find optimal nodes for reconnaissance mission

**Scenario**:
```rust
// Find nearby sensor platforms with comms and sufficient fuel
let nearby_ids = geohash_index.find_near(target_lat, target_lon)?;

let results = Query::new(nodes_collection)
    .filter_by_ids(&nearby_ids)
    .where_eq("operational", Value::Bool(true))
    .where_contains("capabilities", "sensor")
    .where_contains("capabilities", "communication")
    .where_gt("fuel_percent", Value::Int(30))
    .where_gt("battery_percent", Value::Int(50))
    .order_by("distance", SortOrder::Asc)  // Requires distance calculation
    .limit(3)
    .execute()?;
```

**Expected Outcome**: Returns top 3 candidates for recon mission

**Performance Requirement**: <200ms for 100 documents

## Technical Design

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                        Query API Layer                       │
├─────────────────────────────────────────────────────────────┤
│  Query Builder                                               │
│  - .where_eq() .where_gt() .where_lt()                      │
│  - .order_by() .limit() .offset()                           │
│  - .filter_by_ids() (for index integration)                 │
└─────────────────┬───────────────────────────────────────────┘
                  │
                  ↓
┌─────────────────────────────────────────────────────────────┐
│                    Execution Engine                          │
├─────────────────────────────────────────────────────────────┤
│  1. Load documents from AutomergeStore                       │
│  2. Apply predicates (filter)                                │
│  3. Extract sort fields                                      │
│  4. Sort results                                             │
│  5. Apply offset/limit                                       │
└─────────────────┬───────────────────────────────────────────┘
                  │
                  ↓
┌─────────────────────────────────────────────────────────────┐
│                   Spatial Index Layer                        │
├─────────────────────────────────────────────────────────────┤
│  GeohashIndex                                                │
│  - In-memory HashMap<geohash, Set<doc_id>>                  │
│  - .insert(doc_id, lat, lon)                                │
│  - .find_near(lat, lon) → Vec<doc_id>                       │
│  - Queries center cell + 8 neighbors                         │
└─────────────────┬───────────────────────────────────────────┘
                  │
                  ↓
┌─────────────────────────────────────────────────────────────┐
│                AutomergeStore (Existing)                     │
│  - Document storage with RocksDB persistence                 │
│  - .get(doc_id) .all() .put(doc_id, doc)                   │
└─────────────────────────────────────────────────────────────┘
```

### Component Specifications

#### 1. Query Builder (`hive-protocol/src/storage/query.rs`)

**Public API**:
```rust
pub struct Query {
    collection: Collection,
    predicates: Vec<Box<dyn Fn(&Automerge) -> bool + Send + Sync>>,
    sort_field: Option<(String, SortOrder)>,
    limit: Option<usize>,
    offset: usize,
    doc_id_filter: Option<HashSet<String>>,  // For index integration
}

impl Query {
    pub fn new(collection: Collection) -> Self;

    // Field filters
    pub fn where_eq(self, field: &str, value: Value) -> Self;
    pub fn where_gt(self, field: &str, value: Value) -> Self;
    pub fn where_lt(self, field: &str, value: Value) -> Self;
    pub fn where_gte(self, field: &str, value: Value) -> Self;
    pub fn where_lte(self, field: &str, value: Value) -> Self;

    // Array operations
    pub fn where_contains(self, field: &str, value: Value) -> Self;

    // Index integration
    pub fn filter_by_ids(mut self, ids: &[String]) -> Self;

    // Sorting
    pub fn order_by(self, field: &str, order: SortOrder) -> Self;

    // Pagination
    pub fn limit(self, n: usize) -> Self;
    pub fn offset(self, n: usize) -> Self;

    // Execution
    pub fn execute(&self) -> Result<Vec<(String, Automerge)>>;
}

#[derive(Clone, PartialEq)]
pub enum SortOrder {
    Asc,
    Desc,
}

#[derive(Clone, PartialEq, PartialOrd)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Uint(u64),
    Float(f64),
    String(String),
    Timestamp(i64),
}
```

**Key Features**:
- Fluent builder API for composable queries
- Support for nested field paths (e.g., `"position.lat"`)
- Type-safe value comparisons
- Lazy execution (predicates stored, not evaluated until `.execute()`)

---

#### 2. Geohash Index (`hive-protocol/src/storage/geohash_index.rs`)

**Public API**:
```rust
pub struct GeohashIndex {
    index: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    precision: usize,
}

impl GeohashIndex {
    pub fn new(precision: usize) -> Self;

    /// Insert document at location
    pub fn insert(&self, doc_id: &str, lat: f64, lon: f64) -> Result<()>;

    /// Find documents near location (center + 8 neighbors)
    pub fn find_near(&self, lat: f64, lon: f64) -> Result<Vec<String>>;

    /// Remove document from index
    pub fn remove(&self, doc_id: &str, lat: f64, lon: f64) -> Result<()>;

    /// Update document location (remove old + insert new)
    pub fn update(&self, doc_id: &str, old_lat: f64, old_lon: f64,
                  new_lat: f64, new_lon: f64) -> Result<()>;

    /// Clear all entries (for testing)
    pub fn clear(&self);
}
```

**Key Features**:
- Uses `geohash` crate (0.13) for encoding
- Thread-safe with `Arc<RwLock<>>`
- Queries center cell + 8 neighbors (9 cells total)
- Precision 7 (~153m cells) matches beacon geohash precision

**Geohash Precision Table**:
| Precision | Cell Size | Use Case |
|-----------|-----------|----------|
| 5 | ~4.9km × 4.9km | City-level discovery |
| 6 | ~1.2km × 0.6km | Neighborhood-level |
| **7** | **~153m × 153m** | **Tactical cell formation** ⭐ |
| 8 | ~38m × 19m | Building-level |

---

#### 3. Field Extraction (`hive-protocol/src/storage/query.rs`)

**Internal Utilities**:
```rust
/// Extract field value from Automerge document
fn extract_field(doc: &Automerge, field: &str) -> Option<Value>;

/// Convert Automerge scalar to comparable value
fn automerge_to_value(scalar: ScalarValue) -> Value;
```

**Supported Field Paths**:
- Simple fields: `"operational"`, `"timestamp"`
- Nested fields: `"position.lat"`, `"position.lon"`
- Array fields: `"capabilities"` (for `.where_contains()`)

**Automerge Type Mapping**:
| Automerge Type | Query Value Type |
|----------------|------------------|
| `ScalarValue::Str` | `Value::String` |
| `ScalarValue::Int` | `Value::Int` |
| `ScalarValue::Uint` | `Value::Uint` |
| `ScalarValue::F64` | `Value::Float` |
| `ScalarValue::Boolean` | `Value::Bool` |
| `ScalarValue::Timestamp` | `Value::Timestamp` |
| `ScalarValue::Null` | `Value::Null` |

## Implementation Plan

### Week 1: Query Builder Foundation
**Tasks**:
- [ ] Create `hive-protocol/src/storage/query.rs` module
- [ ] Implement `Query` struct with builder pattern
- [ ] Implement `Value` enum with `PartialOrd` trait
- [ ] Implement `.where_eq()`, `.where_gt()`, `.where_lt()` predicates
- [ ] Implement `.order_by()`, `.limit()`, `.offset()` operators
- [ ] Implement `extract_field()` for nested field paths
- [ ] Implement `automerge_to_value()` for type conversion
- [ ] Write unit tests for field extraction

**Deliverables**:
- Query builder API with basic predicates
- Unit tests: 10+ tests for predicates and field extraction
- Documentation with usage examples

**Testing Strategy**:
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_where_eq_simple_field() { /* ... */ }

    #[test]
    fn test_where_eq_nested_field() { /* ... */ }

    #[test]
    fn test_where_gt_numeric_comparison() { /* ... */ }

    #[test]
    fn test_order_by_asc_desc() { /* ... */ }

    #[test]
    fn test_limit_offset_pagination() { /* ... */ }
}
```

---

### Week 2: Query Execution Engine
**Tasks**:
- [ ] Implement `.execute()` method
- [ ] Integrate with `AutomergeStore.all()` for document loading
- [ ] Apply predicates with short-circuit evaluation
- [ ] Implement sorting by extracted field values
- [ ] Apply offset/limit for pagination
- [ ] Add `.filter_by_ids()` for index integration
- [ ] Optimize predicate order (index filters first)
- [ ] Write integration tests with real Automerge documents

**Deliverables**:
- Fully functional query execution
- Integration tests: 8+ tests with AutomergeStore
- Performance benchmarks

**Testing Strategy**:
```rust
#[cfg(test)]
mod integration_tests {
    #[test]
    fn test_query_with_automerge_documents() {
        let store = AutomergeStore::new_temp()?;
        // Create test documents
        // Execute query
        // Assert results
    }

    #[test]
    fn test_query_performance_100_documents() {
        // Benchmark query execution time
        // Assert < 100ms
    }
}
```

---

### Week 3: Geohash Index
**Tasks**:
- [ ] Add `geohash = "0.13"` dependency to `hive-protocol/Cargo.toml`
- [ ] Create `hive-protocol/src/storage/geohash_index.rs` module
- [ ] Implement `GeohashIndex` with `Arc<RwLock<HashMap>>`
- [ ] Implement `.insert()`, `.remove()`, `.update()` methods
- [ ] Implement `.find_near()` with center + 8 neighbors
- [ ] Write unit tests for geohash operations
- [ ] Add concurrency tests (multiple threads inserting)
- [ ] Benchmark spatial query performance

**Deliverables**:
- GeohashIndex implementation
- Unit tests: 8+ tests for index operations
- Concurrency tests
- Performance benchmarks

**Testing Strategy**:
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_insert_and_find_near() {
        let index = GeohashIndex::new(7);
        index.insert("node1", 37.7749, -122.4194)?;
        let results = index.find_near(37.7749, -122.4194)?;
        assert!(results.contains(&"node1".to_string()));
    }

    #[test]
    fn test_find_near_includes_neighbors() {
        // Insert nodes in neighboring cells
        // Verify find_near returns nodes from 9 cells
    }

    #[test]
    fn test_concurrent_inserts() {
        // Spawn multiple threads inserting concurrently
        // Verify no data loss
    }
}
```

---

### Week 4: Integration & E2E Testing
**Tasks**:
- [ ] Integrate Query with GeohashIndex in AutomergeStore
- [ ] Add `.query()` and `.geohash_index()` methods to AutomergeStore
- [ ] Write E2E tests for UC1-UC5 (all use cases)
- [ ] Benchmark queries with 100+ documents
- [ ] Optimize slow paths (if needed)
- [ ] Document query API with examples
- [ ] Update ADR-011 with implementation status
- [ ] Create PR with full test coverage

**Deliverables**:
- Integrated query system in AutomergeStore
- E2E tests: 5 tests covering all use cases
- Performance benchmarks (must meet targets)
- Documentation updates
- PR ready for review

**E2E Testing Strategy**:
```rust
// File: hive-protocol/tests/query_e2e.rs

#[tokio::test]
async fn test_uc1_geographic_beacon_discovery() {
    // Setup: Create 20 beacons with geohash positions
    // Execute: Query nearby beacons with geohash index
    // Assert: Returns correct beacons within ~500m
    // Assert: Query completes in < 100ms
}

#[tokio::test]
async fn test_uc2_capability_based_discovery() {
    // Setup: Create nodes with various capabilities and fuel levels
    // Execute: Query for sensors with fuel > 20%
    // Assert: Returns correct nodes sorted by fuel
    // Assert: Query completes in < 100ms
}

// ... tests for UC3, UC4, UC5
```

## Definition of Done

### Functional Requirements
- [ ] **UC1-UC5 E2E tests pass** - All use cases validated
- [ ] **Query API complete** - `.where_eq()`, `.where_gt()`, `.where_lt()`, `.order_by()`, `.limit()`, `.offset()`
- [ ] **Geohash index working** - `.find_near()` returns correct cells (center + 8 neighbors)
- [ ] **Integration with AutomergeStore** - Query and index accessible from store
- [ ] **Nested field support** - Can query `"position.lat"` and other nested fields
- [ ] **Array contains support** - Can query `"capabilities"` array fields

### Performance Requirements
- [ ] **Simple queries < 100ms** - For 100 documents (UC1, UC2, UC4)
- [ ] **Complex queries < 200ms** - For 100 documents (UC3, UC5)
- [ ] **Geohash query < 50ms** - Spatial index lookup only
- [ ] **Pagination overhead < 10ms** - Offset/limit operations

### Quality Requirements
- [ ] **Unit test coverage > 80%** - For query.rs and geohash_index.rs
- [ ] **Integration tests** - 8+ tests with real AutomergeStore
- [ ] **E2E tests** - 5+ tests for all use cases
- [ ] **Concurrency tests** - GeohashIndex thread-safe
- [ ] **Documentation** - API docs with examples for all public methods
- [ ] **ADR updated** - ADR-011 reflects implementation status

### Code Quality
- [ ] **No clippy warnings** - `cargo clippy --all-targets`
- [ ] **Formatted code** - `cargo fmt --all`
- [ ] **No panics** - All errors returned as `Result<T>`
- [ ] **Thread-safe** - GeohashIndex uses `Arc<RwLock<>>`

## Dependencies

### Cargo.toml Additions
```toml
[dependencies]
geohash = "0.13"  # Geohash encoding/decoding
```

### Existing Dependencies Used
- `automerge = "0.7.1"` - Document storage
- `rocksdb = "0.22"` - Persistence layer (via AutomergeStore)
- `tokio` - Async runtime for E2E tests

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| **Performance degradation with large collections** | Medium | High | Benchmark with 1000+ docs, add field indices if needed |
| **Automerge field extraction complexity** | Low | Medium | Study Automerge API docs, write comprehensive tests |
| **Geohash precision mismatch** | Low | Medium | Use precision 7 (matches existing beacon system) |
| **Query API too complex** | Low | Low | Keep API simple, defer advanced features to Phase 2 |
| **Thread safety issues in GeohashIndex** | Low | High | Use `Arc<RwLock<>>`, add concurrency tests |

## Success Metrics

- ✅ All E2E tests pass (UC1-UC5)
- ✅ Query performance < 100ms for 100 documents
- ✅ Geohash queries < 50ms
- ✅ Unit test coverage > 80%
- ✅ Zero clippy warnings
- ✅ Documentation complete with examples
- ✅ PR approved and merged

## References

- **ADR-002**: Beacon Storage Architecture (geohash precision, TTL strategy)
- **ADR-011**: Automerge+Iroh Backend (lines 1205-1456 - pseudocode for Query and GeohashIndex)
- **Existing Code**:
  - `hive-protocol/src/discovery/geographic.rs` - GeographicBeacon struct
  - `hive-protocol/src/discovery/capability_query.rs` - In-memory query logic
  - `hive-protocol/src/storage/automerge_backend.rs` - AutomergeStore integration point
- **Geohash Crate**: https://docs.rs/geohash/0.13.0/geohash/
- **Automerge Docs**: https://docs.rs/automerge/0.7.1/automerge/

## Future Enhancements (Out of Scope for Issue #80)

- **Secondary indices** - Field-based indices for non-spatial queries
- **Watch API** - Subscribe to query results changes
- **Compound queries** - OR logic, NOT logic
- **Full-text search** - String pattern matching
- **Distance calculations** - Haversine distance in query results
- **Query optimization** - Query planner, index selection
