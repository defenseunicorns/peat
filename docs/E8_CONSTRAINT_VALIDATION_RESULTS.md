# E8.1 Priority 1: Network Constraint Validation Results

**Date**: 2025-11-05
**Validation Test**: Extreme Network Constraints with TCP Transport
**Decision**: ❌ **NO-GO for Shadow Approach**

## Executive Summary

Shadow network simulator **CANNOT** be used for E8 network simulation because it lacks full TCP socket option support required by the Ditto SDK. TCP connections fail to establish due to incompatible socket implementations, preventing any network constraint testing.

**Recommendation**: **Pivot to ADR-008 Option 1 (Linux Network Namespaces)** for E8 implementation.

---

## What We Tested

### Validation Scenario
Created `cap-sim/scenarios/validation-extreme-constraints.yaml` with:
- **Network Constraints**: 56 Kbps bandwidth, 1000ms latency, 5% packet loss
- **Transport**: Explicit TCP (node1 listener on port 12345, node2 client connecting to 11.0.0.1:12345)
- **Goal**: Validate that Shadow's network constraints actually affect Ditto traffic

### Test Configuration
```yaml
network:
  graph:
    type: gml
    inline: |
      graph [
        node [
          id 0
          host_bandwidth_down "56 Kbit"
          host_bandwidth_up "56 Kbit"
        ]
        edge [
          source 0
          target 0
          latency "1000 ms"
          packet_loss 0.05
        ]
      ]

hosts:
  node1:
    processes:
      - path: target/debug/examples/shadow_poc
        args: --node-id node1 --mode writer --tcp-listen 12345

  node2:
    processes:
      - path: target/debug/examples/shadow_poc
        args: --node-id node2 --mode reader --tcp-connect 11.0.0.1:12345
```

---

## Results: TCP Socket Failures

### node1 (TCP Listener) - FAILED
```
2000-01-01T00:00:01.000000Z  INFO dittoffi: Starting TCP server bind="0.0.0.0:12345"
2000-01-01T00:00:01.000000Z ERROR dittoffi: Failed to start TCP server error=I/O error: Protocol not available (os error 92)

Caused by:
    Protocol not available (os error 92)
```

**Error Repeated**: Every second from t=1s to t=15s (15 attempts)

### node2 (TCP Client) - FAILED
```
2000-01-01T00:00:01.000000Z  INFO dittoffi: Starting static TCP client connection to "11.0.0.1:12345"
2000-01-01T00:00:01.000000Z  INFO dittoffi: Starting TCP server bind="[::]:0"
2000-01-01T00:00:01.000000Z ERROR dittoffi: Failed to start TCP server error=I/O error: Address family not supported by protocol (os error 97)

Caused by:
    Address family not supported by protocol (os error 97)
```

**Error Repeated**: Every second from t=1s to t=26s (26 attempts)

### Test Outcome
- **TCP connections**: ❌ Never established
- **Peer discovery**: ❌ Never occurred
- **Document sync**: ❌ Never attempted (no connection)
- **Network constraints**: ❌ **UNTESTABLE** - no traffic to constrain

---

## Root Cause Analysis

### Error 92: ENOPROTOOPT - "Protocol not available"
This error occurs when:
1. A socket option is not supported by the underlying system
2. The protocol level specified in `setsockopt()` is incorrect
3. The network stack doesn't implement the requested feature

### Shadow's Limited TCP Support
According to Shadow's documentation (found via web research):
> "Shadow implements over 150 functions from the system call API, but does not yet fully support all API features. Although applications that make basic use of the supported system calls should work out of the box, **those that use more complex features or functions may not yet function correctly when running in Shadow**."

### Ditto SDK Incompatibility
The Ditto SDK's TCP implementation uses socket options or system call features that Shadow's simulated syscall layer does not support:

1. **node1 error (92)**: Ditto tries to bind TCP listener using socket options Shadow doesn't implement
2. **node2 error (97)**: Ditto tries to bind IPv6 listener (`[::]`), which Shadow doesn't support
3. **Fundamental incompatibility**: Not just missing features, but core TCP binding operations fail

---

## Comparison to Initial POC

### E8.0 POC (mDNS Transport)
- ✅ Ditto initialized successfully
- ✅ Ditto ran under Shadow's syscall interception
- ❌ Peer discovery failed (mDNS requires multicast, which Shadow doesn't support)
- ⚠️  **Assumed TCP would work** - this was incorrect

### E8.1 Validation (TCP Transport)
- ✅ Ditto initialized successfully
- ❌ TCP listener binding fails (ENOPROTOOPT)
- ❌ TCP connections never established
- ❌ **No transport mechanism available** for Ditto in Shadow

---

## Critical Questions Answered

### User Question: "Were you able to exercise Shadow (limit bandwidth, cause network interruptions, etc.) in the POC?"

**Answer**: **NO**

1. **E8.0 POC**: Did NOT test constraints - only validated Ditto runs under Shadow
2. **E8.1 Validation**: **CANNOT test constraints** - TCP connections fail before any network traffic occurs
3. **Fundamental blocker**: Shadow cannot run Ditto with any working transport mechanism

### Can This Be Fixed?

**Short answer**: Unlikely without significant Shadow development effort.

**Long answer**:
- Shadow would need to implement the specific socket options Ditto requires
- This is not a configuration issue - it's missing functionality in Shadow's syscall layer
- Extending Shadow's socket API support is complex and time-consuming
- Ditto SDK is closed-source, so we can't modify its socket usage

---

## NO-GO Decision

### Shadow Approach: ❌ **NOT VIABLE**

**Reasons**:
1. ❌ TCP connections fail with ENOPROTOOPT errors
2. ❌ No working transport mechanism for Ditto (mDNS doesn't work, TCP doesn't work)
3. ❌ Cannot test network constraints without established connections
4. ❌ Fixing Shadow would require significant development effort outside project scope
5. ❌ Risk too high - even if TCP worked, other Ditto features might fail

### Impact on E8 Plan
- E8.0 (Shadow POC): ~~Completed~~ **INVALIDATED** - TCP validation failed
- E8.1 (Shadow Harness): **BLOCKED** - cannot proceed with Shadow
- E8.2-E8.4: **BLOCKED** - all depend on working Shadow simulation

---

## Recommended Path Forward

### Pivot to ADR-008 Option 1: Linux Network Namespaces

#### Why Linux Namespaces?
From ADR-008:
> "Pros: Runs real Linux network stack (not simulated), Full TCP/IP support"

This approach:
1. ✅ Uses real Linux kernel networking (no syscall simulation)
2. ✅ Full TCP/IP protocol support (all socket options work)
3. ✅ Real network interfaces with real constraints (tc, netem)
4. ✅ Ditto SDK will work without modifications
5. ✅ Industry-standard approach (used by Docker, Kubernetes)

#### Implementation Path
1. **E8.1 Reboot**: Implement namespace-based harness
   - Create network namespaces with `ip netns`
   - Create virtual ethernet pairs (`veth`) connecting namespaces
   - Apply traffic control with `tc qdisc` (bandwidth, latency, loss)

2. **E8.2 Constraints**: Use `tc` (traffic control) for network shaping
   - Bandwidth limiting: `tc qdisc add dev veth0 root tbf`
   - Latency injection: `tc qdisc add dev veth0 root netem delay`
   - Packet loss: `tc qdisc add dev veth0 root netem loss`

3. **E8.3 Partitions**: Use `iptables` for network isolation
   - Selective packet dropping between namespace groups
   - Heal partitions by removing iptables rules

4. **E8.4 Analysis**: Same telemetry approach
   - Ditto sync events, document convergence times
   - Compare constrained vs unconstrained scenarios

#### Trade-offs
**Cons vs Shadow**:
- Slower execution (real-time, not simulated time)
- More system resources (real processes, real network stack)
- Less deterministic (real Linux scheduler, real timing)

**Why This Is Acceptable**:
- **Correctness > Speed**: Real network behavior > Fast incorrect simulation
- **E8 Scale**: 112 nodes (Army company) fits in Linux namespaces (tested up to 10K containers)
- **Timeline**: Week 1 still feasible - namespace setup is simpler than debugging Shadow
- **Risk**: Low - proven technology, known to work with real applications

---

## Timeline Impact

### Original E8 Timeline
- Week 1: E8.0 POC (Shadow) + E8.1 Harness (Shadow)
- Week 2: E8.2 Constraints + E8.3 Partitions
- Week 3: E8.4 Analysis + Documentation

### Revised E8 Timeline (Namespaces)
- **Week 1 (Days 1-2)**: E8.1 Namespace Harness (replaces Shadow POC)
  - Create namespace topology (2-node, then 112-node)
  - Validate Ditto sync between namespaces
  - Implement metrics collection

- **Week 1 (Days 3-5)**: E8.2 Basic Constraints
  - Implement `tc` bandwidth/latency/loss
  - Validate constraints affect Ditto traffic
  - Baseline measurements

- **Week 2**: E8.3 Partition Scenarios
  - Implement `iptables`-based partitions
  - Test 3-partition scenario (company → platoons)
  - Measure convergence times

- **Week 3**: E8.4 Analysis + Documentation
  - Compare baseline vs constrained vs partitioned
  - Generate plots and analysis
  - Document E8 results and recommendations

**Net Timeline Impact**: +2 days (recovering from Shadow detour)
**Overall Status**: Still achievable within original 3-week estimate

---

## Lessons Learned

### What Went Wrong
1. ❌ **Premature GO decision** after E8.0 POC
   - Validated Ditto runs under Shadow
   - Did NOT validate network connectivity works
   - Assumed TCP would "just work"

2. ❌ **Insufficient skepticism** of simulation approach
   - Shadow's "runs real applications" marketing was compelling
   - Didn't adequately investigate syscall support limitations
   - Should have started with simpler TCP connectivity test

3. ✅ **User question caught the mistake**
   - "Were you able to exercise Shadow?" revealed the gap
   - Constraint validation exposed the incompatibility
   - Better to find blocker in Week 1 than Week 2

### What Went Right
1. ✅ **Structured validation approach**
   - Created extreme constraint scenario to make failures obvious
   - Documented expected vs actual behavior
   - Clear success criteria

2. ✅ **Quick pivot path**
   - ADR-008 already identified fallback option
   - Linux namespaces are well-understood technology
   - Can recover timeline with namespace approach

3. ✅ **Learning captured**
   - Rust 1.86+ required for Ditto
   - Shadow's syscall limitations documented
   - TCP transport configuration for Ditto understood

---

## Next Steps

1. ✅ **Document NO-GO decision** (this document)
2. ⬜ **Update E8.1 GitHub issue** with pivot to namespaces
3. ⬜ **Create E8.1 namespace harness plan**
4. ⬜ **Begin namespace implementation**
   - Start with 2-node topology
   - Validate Ditto sync between namespaces
   - Then scale to 112-node company structure

---

## Supporting Evidence

### Validation Test Files
- **Scenario**: `cap-sim/scenarios/validation-extreme-constraints.yaml`
- **Shadow Logs**: `shadow.data/hosts/node1/shadow_poc.1000.stderr`
- **Shadow Logs**: `shadow.data/hosts/node2/shadow_poc.1000.stderr`
- **Test Run Log**: `validation_run.log`

### Related Documentation
- **ADR-008**: Network Simulation Layer Architecture Decision (identifies namespace alternative)
- **E8 Implementation Plan**: Original 3-week Shadow-based plan
- **E8.0 POC Results**: Initial Shadow validation (incomplete)

---

## Conclusion

Shadow network simulator is **NOT VIABLE** for E8 network simulation due to fundamental TCP socket incompatibility with Ditto SDK. The constraint validation test revealed that Shadow's syscall simulation layer does not support the socket options required by Ditto's TCP implementation, preventing any network connections from being established.

**Recommendation**: **Pivot to Linux Network Namespaces** (ADR-008 Option 1) as the E8 network simulation approach. This provides full TCP/IP protocol support, real Linux networking, and proven scalability for the 112-node Army company use case.

**Timeline**: Recoverable - namespace approach is simpler than debugging Shadow, estimated +2 days to original timeline.

**Next Action**: Update E8.1 issue and begin namespace harness implementation.
