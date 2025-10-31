# Nomenclature Refactoring Status
**Branch:** refactor/domain-agnostic-nomenclature
**Started:** 2025-10-31
**Status:** IN PROGRESS - 67 compilation errors remaining

---

## Objective

Refactor CAP protocol from military-specific terminology to domain-agnostic names:
- Platform → Node
- Squad → Cell
- Bootstrap → Discovery
- Platoon → Zone (E5, not yet implemented)

---

## Progress Summary

### ✅ Completed

**File/Directory Renames:**
- ✅ `models/platform.rs` → `models/node.rs`
- ✅ `models/squad/` → `models/cell/`
- ✅ `bootstrap/` → `discovery/`
- ✅ `squad/` → `cell/`
- ✅ `storage/platform_store.rs` → `storage/node_store.rs`
- ✅ `storage/squad_store.rs` → `storage/cell_store.rs`

**Struct Declarations Renamed:**
- ✅ `PlatformConfig` → `NodeConfig`
- ✅ `PlatformState` → `NodeState`
- ✅ `PlatformStore` → `NodeStore`
- ✅ `SquadConfig` → `CellConfig`
- ✅ `SquadState` → `CellState`
- ✅ `SquadStore` → `CellStore`
- ✅ `SquadMessage` → `CellMessage`
- ✅ `SquadMessageBus` → `CellMessageBus`
- ✅ `SquadMessageType` → `CellMessageType`
- ✅ `SquadCoordinator` → `CellCoordinator`
- ✅ `SquadAssignment` → `CellAssignment`
- ✅ `SquadRole` → `CellRole`
- ✅ `SquadObserver` → `CellObserver`
- ✅ `PlatformObserver` → `NodeObserver`
- ✅ `BootstrapCoordinator` → `DiscoveryCoordinator`
- ✅ `BootstrapMetrics` → `DiscoveryMetrics`

**Phase Enum Updated:**
- ✅ `Phase::Bootstrap` → `Phase::Discovery`
- ✅ `Phase::Squad` → `Phase::Cell`
- ✅ Added legacy constants for backward compat

**Module Structure:**
- ✅ `lib.rs` updated with new module names
- ✅ `models/mod.rs` updated with re-exports and legacy aliases
- ✅ `storage/mod.rs` updated with re-exports and legacy aliases
- ✅ Created `cell/mod.rs`
- ✅ Updated `discovery/mod.rs`

**Legacy Compatibility:**
- ✅ Added type aliases in `models/mod.rs`:
  - `NodeConfig as PlatformConfig`
  - `NodeState as PlatformState`
  - `CellConfig as SquadConfig`
  - `CellState as SquadState`
  - `CellRole as SquadRole`
- ✅ Added type aliases in `storage/mod.rs`:
  - `NodeStore as PlatformStore`
  - `CellStore as SquadStore`
- ✅ Added Phase constants:
  - `Phase::Bootstrap = Phase::Discovery`
  - `Phase::Squad = Phase::Cell`

---

## ⚠️ Remaining Work

### Compilation Errors: 67

**Error Categories:**
- 19× `cannot find type SquadState`
- 11× `cannot find type PlatformState`
- 7× `cannot find type PlatformConfig`
- 6× `cannot find type SquadMessage`
- 6× `cannot find type SquadAssignment`
- 5× `cannot find type SquadStore`
- 5× `cannot find type SquadRole`
- Plus: missing imports for other types

**Root Cause:** Files need explicit `use` statements for types because:
1. Some files use types without importing from `crate::models`
2. Module structure changed (node:: and cell:: submodules added)
3. Legacy aliases work but code needs to import them

**Solution Strategy:**

1. **Add missing imports** - Most errors are just missing `use` statements:
   ```rust
   use crate::models::{PlatformConfig, PlatformState, SquadState, SquadRole};
   use crate::storage::SquadStore;
   use crate::cell::messaging::{SquadMessage, SquadMessageBus};
   ```

2. **OR**: Keep using legacy names via aliases (works because we added them)

3. **OR**: Update to new names:
   ```rust
   use crate::models::{NodeConfig, NodeState, CellState, CellRole};
   use crate::storage::CellStore;
   use crate::cell::messaging::{CellMessage, CellMessageBus};
   ```

### Files With Most Errors

Based on error locations:
- `discovery/capability_query.rs` - missing Platform/Squad types
- `discovery/coordinator.rs` - missing Squad/Bootstrap types
- `discovery/directed.rs` - missing SquadAssignment
- `cell/messaging.rs` - internal references
- `cell/coordinator.rs` - Squad types
- `testing/e2e_harness.rs` - Observer types

---

## Next Steps

### Step 1: Fix Compilation (2-3 hours)

**Option A: Keep legacy names** (faster, less churntask)
```bash
# Add imports for legacy names to each file
find cap-protocol/src -name "*.rs" -exec add-missing-imports.sh {} \;
```

**Option B: Full migration to new names** (cleaner, more work)
```bash
# Update all type references to new names
# Add imports for new names
```

**Recommendation:** Option A for POC phase. Full migration can happen gradually.

### Step 2: Run Tests (1 hour)

After compilation succeeds:
```bash
cargo test --workspace
```

Fix any test failures due to:
- Changed type names in assertions
- Changed collection names in Ditto
- Phase enum changes

### Step 3: Format Code (5 min)

```bash
cargo fmt --all
```

### Step 4: Create Domain Modules (1 hour)

Create `cap-protocol/src/domains/` with:
- `military.rs` - Platform, Squad, Platoon, Company
- `robotics.rs` - Robot, Cell, Zone, Factory
- `iot.rs` - Sensor, Cell, Zone, Network
- `vehicles.rs` - Vehicle, Convoy, Corridor, Fleet

Example:
```rust
// domains/military.rs
pub use crate::models::{
    NodeConfig as PlatformConfig,
    NodeState as PlatformState,
    CellConfig as SquadConfig,
    CellState as SquadState,
};
pub use crate::storage::{
    NodeStore as PlatformStore,
    CellStore as SquadStore,
};
```

### Step 5: Update Documentation (2 hours)

- Update README.md with new terminology
- Update all ADRs (001-004) with new names
- Update E5-IMPLEMENTATION-PLAN.md (Zone instead of Platoon)
- Update CAP-POC-Project-Plan.md with new phase names
- Add nomenclature mapping table to README

---

## Testing Strategy

**Unit Tests:**
- Most should pass with legacy aliases
- Some may need collection name updates ("squads" → "cells")

**Integration Tests:**
- Update test names to use new terminology
- Verify Ditto collection names work

**E2E Tests:**
- May need updates for observer types
- Collection names in Ditto queries

---

## Files Modified (Partial List)

**Renamed:**
- cap-protocol/src/models/platform.rs → node.rs
- cap-protocol/src/models/squad/ → cell/
- cap-protocol/src/bootstrap/ → discovery/
- cap-protocol/src/squad/ → cell/
- cap-protocol/src/storage/platform_store.rs → node_store.rs
- cap-protocol/src/storage/squad_store.rs → cell_store.rs

**Modified:**
- cap-protocol/src/lib.rs
- cap-protocol/src/traits.rs
- cap-protocol/src/models/mod.rs
- cap-protocol/src/models/role.rs
- cap-protocol/src/models/node.rs
- cap-protocol/src/models/cell/mod.rs
- cap-protocol/src/storage/mod.rs
- cap-protocol/src/storage/node_store.rs
- cap-protocol/src/storage/cell_store.rs
- cap-protocol/src/discovery/mod.rs
- cap-protocol/src/discovery/coordinator.rs
- cap-protocol/src/discovery/capability_query.rs
- cap-protocol/src/cell/*.rs (all files)
- cap-protocol/src/testing/e2e_harness.rs
- ...and 30+ more files

**Total Files Modified:** ~48 Rust files

---

## Automated Fix Script

To complete the refactoring quickly, run:

```bash
#!/bin/bash
# fix-imports.sh - Add missing imports to files

# Find all files with compilation errors
cargo check 2>&1 | grep "error\[E0412\]" | grep -o "src/[^:]*" | sort -u | while read file; do
    echo "Fixing imports in $file"

    # Add common imports if not already present
    if ! grep -q "use crate::models::" "$file"; then
        sed -i '1a\
use crate::models::{PlatformConfig, PlatformState, SquadConfig, SquadState, SquadRole};' "$file"
    fi

    if ! grep -q "use crate::storage::" "$file"; then
        sed -i '1a\
use crate::storage::{PlatformStore, SquadStore};' "$file"
    fi
done

cargo fmt --all
```

---

## Decision Points

**1. Full migration vs. Legacy names?**
- **Legacy names:** Faster, less risk, gradual migration possible
- **Full migration:** Cleaner, domain-agnostic from start, more work now

**Recommendation:** Use legacy names for now, migrate gradually

**2. When to create domain modules?**
- **Now:** Shows intent, documents mappings
- **Later:** After E5, when needed for multi-domain use

**Recommendation:** Create placeholder now, flesh out later

**3. Update tests now or later?**
- **Now:** Ensures refactoring is complete
- **Later:** Fix as failures occur

**Recommendation:** Run tests after compilation succeeds, fix critical failures

---

## Estimated Time to Complete

- Fix compilation errors: **2-3 hours**
- Run and fix tests: **1 hour**
- Create domain modules: **1 hour**
- Update documentation: **2 hours**
- **Total: 6-7 hours**

With focused work, could complete in one session.

---

## Branch Status

```bash
git branch: refactor/domain-agnostic-nomenclature
git status: ~48 files modified, ready to commit WIP

# To resume work:
git checkout refactor/domain-agnostic-nomenclature
cargo check --workspace  # See remaining errors
# Fix imports, test, document, then merge
```

---

## Commit Strategy

**Option A: Single commit**
- Fix all errors, test, then commit as "refactor: Domain-agnostic nomenclature"

**Option B: Incremental commits**
- Commit WIP now: "refactor: WIP domain-agnostic names (67 errors)"
- Commit fixes: "fix: Add missing imports for refactored types"
- Commit tests: "test: Update tests for new terminology"
- Commit domains: "feat: Add domain terminology modules"
- Commit docs: "docs: Update for domain-agnostic terminology"

**Recommendation:** Option B for better git history

---

**Status:** Ready for next work session to complete remaining 67 errors.
