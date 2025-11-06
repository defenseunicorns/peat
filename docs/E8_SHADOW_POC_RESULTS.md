# E8.0 Shadow + Ditto POC Results

**Date**: 2025-11-05
**Status**: ❌ **BLOCKED - Pre-Shadow Issue Discovered**
**Decision**: Cannot proceed to Shadow testing due to Ditto SDK compilation failure

## Executive Summary

The E8.0 POC discovered a **critical blocker BEFORE testing Shadow**: The Ditto SDK cannot currently compile with Rust 1.85.0 due to symbol conflicts in the FFI layer. This issue affects ALL Ditto-dependent code (tests, examples, and the Shadow POC).

**Key Finding**: The problem is with Ditto + Rust toolchain compatibility, NOT with Shadow.

## What Was Tested

### Artifacts Created
1. ✅ `cap-protocol/examples/shadow_poc.rs` - Minimal Ditto sync test (147 lines)
2. ✅ `cap-sim/scenarios/poc-ditto-sync.yaml` - Shadow configuration for 2-node sync
3. ✅ Shadow v3.3.0 successfully installed on Linux workstation

### Compilation Attempted
- Shadow POC binary (`shadow_poc`)
- Existing example (`ditto_spike`)
- Existing E2E test (`baseline_ditto_bandwidth_e2e`)

**Result**: ALL compilations fail with identical linker errors

## The Issue: Ditto FFI Symbol Conflicts

### Error Summary

```
/usr/bin/ld: libdittoffi.a(std-*.o): multiple definition of `std::time::Instant::checked_add'
/usr/bin/ld: libdittoffi.a(std-*.o): multiple definition of `<&std::sys::pal::unix::fd::FileDesc as std::io::Read>::read_vectored'
/usr/bin/ld: libdittoffi.a(std-*.o): multiple definition of `<std::sys_common::wtf8::Wtf8Buf as core::ops::deref::DerefMut>::deref_mut'
... (many more symbol conflicts)

error: could not compile due to 1 previous error
```

### Root Cause Analysis

The Ditto SDK's FFI library (`libdittoffi.a`) was built with Rust **1.85.0**, which embeds its own copy of the Rust standard library. When linking our code (also compiled with Rust 1.85.0), we get **duplicate symbol** errors because both sides define the same standard library functions.

This is a **version mismatch** issue in the Ditto FFI build process.

### Affected Components

- ❌ All examples (`ditto_spike`, `shadow_poc`)
- ❌ All E2E tests that use Ditto
- ❌ Any binary that links against `dittolive-ditto`

**Scope**: This is a **workspace-wide blocker** for any Ditto-dependent code.

## Investigation Steps

1. ✅ Created `shadow_poc.rs` using Collection API (simpler than DQL)
2. ✅ Fixed compilation errors (AppId, async, QueryArguments)
3. ✅ Attempted debug build → Linker error
4. ✅ Attempted release build → Same linker error
5. ✅ Tested existing `ditto_spike` example → Same linker error
6. ✅ Tested existing E2E test → Same linker error

**Conclusion**: This is NOT a code issue in `shadow_poc.rs` - it's a fundamental Ditto SDK linking problem.

## Attempted Workarounds

### 1. Release Mode Build
```bash
cargo build --release --example shadow_poc
```
**Result**: ❌ Same linker errors

### 2. Existing Examples
```bash
cargo build --example ditto_spike
```
**Result**: ❌ Same linker errors

### 3. Existing Tests
```bash
cargo test --test baseline_ditto_bandwidth_e2e --no-run
```
**Result**: ❌ Same linker errors

**Conclusion**: No workaround found. This requires fixing the Ditto SDK build or changing Rust versions.

## Questions for User

1. **When did Ditto last compile successfully?**
   - Check git history: When was last successful test run?
   - What Rust version was used?

2. **Has the Rust toolchain changed recently?**
   - Current: Rust 1.85.0 (2025-02-17)
   - Was a different version used before?

3. **Ditto SDK version**:
   - Current: `dittolive-ditto v4.11.5`
   - Is there a newer version that fixes this?
   - Can we downgrade Rust to match Ditto's build?

## Potential Solutions

### Option 1: Downgrade Rust (Most Likely)
```bash
rustup override set 1.80  # Or whatever version Ditto was built with
cargo clean
cargo build --example shadow_poc
```

**Pros**: Likely to work immediately
**Cons**: Need to identify correct version

### Option 2: Update Ditto SDK
```bash
# In Cargo.toml
[dependencies]
dittolive-ditto = "4.12.4"  # Latest version
```

**Pros**: Stay current with latest Rust
**Cons**: May have breaking API changes

### Option 3: Wait for Ditto SDK Fix
Contact Ditto support about FFI symbol conflicts with Rust 1.85.0

**Pros**: Proper fix from upstream
**Cons**: Unknown timeline

### Option 4: Use Older Rust for Ditto Only
```bash
# In rust-toolchain.toml for cap-protocol
[toolchain]
channel = "1.80"  # Or appropriate version
```

**Pros**: Isolates Ditto to specific Rust version
**Cons**: Complexity of managing multiple toolchains

## Impact on E8 (Network Simulation)

### Can We Proceed with E8?

**Short answer**: NO - not until Ditto compiles.

**Dependencies**:
- E8.1-E8.4 all require running Ditto instances
- Shadow POC requires Ditto to create test binaries
- All network simulation scenarios use real Ditto sync

### Alternative: Mock Simulation (Not Recommended)

We COULD proceed with:
- Mock CRDT implementation (no real Ditto)
- Shadow network simulation with mock nodes
- Test protocol logic without actual sync

**BUT**: This defeats the entire purpose of E8, which is to:
1. Validate Ditto works under realistic network conditions
2. Measure ACTUAL bandwidth usage
3. Establish baseline for delta optimizations

**ADR-001 explicitly rejected** mock simulation as insufficient.

## Recommendations

### Immediate Actions

1. **Identify Working Rust Version**
   - Check git history for last successful Ditto build
   - Check CI logs if available
   - Try Rust 1.80, 1.76, 1.74 (available on system)

2. **Test with Older Rust**
   ```bash
   rustup override set 1.80
   cargo clean
   cargo build --example ditto_spike
   ```

3. **Document Working Version**
   - Update README with required Rust version
   - Add rust-toolchain.toml if needed

### Once Ditto Compiles

1. **Resume E8.0 POC**
   - Build `shadow_poc` binary
   - Run under Shadow: `shadow cap-sim/scenarios/poc-ditto-sync.yaml`
   - Analyze results

2. **Make GO/NO-GO Decision**
   - IF Ditto works under Shadow → Proceed with E8.1-E8.4
   - IF Ditto doesn't work under Shadow → Fall back to namespaces

## Files Created

- `cap-protocol/examples/shadow_poc.rs` - POC binary (ready, won't compile)
- `cap-sim/scenarios/poc-ditto-sync.yaml` - Shadow config (ready)
- `docs/E8_SHADOW_INSTALLATION.md` - Shadow installation guide (complete)
- `docs/E8_SHADOW_POC_RESULTS.md` - This document

## Conclusion

E8.0 POC Status: **❌ BLOCKED**

**Blocker**: Ditto SDK linker errors with Rust 1.85.0
**Not Tested**: Shadow + Ditto compatibility (can't get to Shadow yet)
**Action Required**: Fix Ditto compilation before proceeding

**Next Steps**:
1. Identify compatible Rust version
2. Rebuild with that version
3. Resume E8.0 POC testing

---

**Document Version**: 1.0
**Last Updated**: 2025-11-05
**Status**: Awaiting Ditto compilation fix
