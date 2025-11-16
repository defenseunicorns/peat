# HIVE Protocol - Comprehensive Bandwidth Test Suite
# E11: All Modes × All Bandwidth Constraints

**Test Date:** Sun Nov  9 07:17:05 PM EST 2025
**Test Suite:** All Modes × All Bandwidths
**Total Tests:** 16 (4 modes × 4 bandwidths)

---

## Executive Summary

This report validates HIVE protocol performance across all operating modes under various bandwidth constraints, from gigabit ethernet to tactical radio bandwidths.

### Test Matrix

| Mode | 1Gbps | 100Mbps | 1Mbps | 256Kbps |
|------|-------|---------|-------|---------|
| Mode 1 (Client-Server) | - | - | - | - |
| Mode 2 (Hub-Spoke) | - | - | - | - |
| Mode 3 (Dynamic Mesh) | - | - | - | - |
| Mode 4 (Hierarchical) | - | - | - | - |

---

## Detailed Results


### Mode 1: Client-Server @ 1gbps

**Parameters:**
- Bandwidth: 1048576 Kbps
- Duration: 60s
- Nodes: 12

**Results:**
- Messages Sent: 6
- Documents Inserted: 4
- Documents Received: 132
- Avg Latency: 15.1ms
- p50 Latency: 11.852ms
- p90 Latency: 15.814ms


### Mode 1: Client-Server @ 100mbps

**Parameters:**
- Bandwidth: 102400 Kbps
- Duration: 60s
- Nodes: 12

**Results:**
- Messages Sent: 6
- Documents Inserted: 4
- Documents Received: 132
- Avg Latency: 13.3ms
- p50 Latency: 12.941ms
- p90 Latency: 15.369ms


### Mode 1: Client-Server @ 1mbps

**Parameters:**
- Bandwidth: 1024 Kbps
- Duration: 90s
- Nodes: 12

**Results:**
- Messages Sent: 9
- Documents Inserted: 6
- Documents Received: 176
- Avg Latency: 38.2ms
- p50 Latency: 12.753ms
- p90 Latency: 135.733ms


### Mode 1: Client-Server @ 256kbps

**Parameters:**
- Bandwidth: 256 Kbps
- Duration: 90s
- Nodes: 12

**Results:**
- Messages Sent: 9
- Documents Inserted: 6
- Documents Received: 176
- Avg Latency: 13.9ms
- p50 Latency: 14.234ms
- p90 Latency: 16.122ms


### Mode 2: Hub-Spoke @ 1gbps

**Parameters:**
- Bandwidth: 1048576 Kbps
- Duration: 60s
- Nodes: 12

**Results:**
- Messages Sent: 6
- Documents Inserted: 4
- Documents Received: 132
- Avg Latency: 16.3ms
- p50 Latency: 12.234ms
- p90 Latency: 21.954ms


### Mode 2: Hub-Spoke @ 100mbps

**Parameters:**
- Bandwidth: 102400 Kbps
- Duration: 60s
- Nodes: 12

**Results:**
- Messages Sent: 6
- Documents Inserted: 4
- Documents Received: 132
- Avg Latency: 29.0ms
- p50 Latency: 19.666ms
- p90 Latency: 51.178ms


### Mode 2: Hub-Spoke @ 1mbps

**Parameters:**
- Bandwidth: 1024 Kbps
- Duration: 90s
- Nodes: 12

**Results:**
- Messages Sent: 9
- Documents Inserted: 6
- Documents Received: 176
- Avg Latency: 16.7ms
- p50 Latency: 13.912ms
- p90 Latency: 24.091ms


### Mode 2: Hub-Spoke @ 256kbps

**Parameters:**
- Bandwidth: 256 Kbps
- Duration: 90s
- Nodes: 12

**Results:**
- Messages Sent: 9
- Documents Inserted: 6
- Documents Received: 176
- Avg Latency: 16.4ms
- p50 Latency: 14.894ms
- p90 Latency: 24.334ms


### Mode 3: Dynamic Mesh @ 1gbps

**Parameters:**
- Bandwidth: 1048576 Kbps
- Duration: 60s
- Nodes: 12

**Results:**
- Messages Sent: 6
- Documents Inserted: 4
- Documents Received: 132
- Avg Latency: 15.3ms
- p50 Latency: 15.228ms
- p90 Latency: 18.298ms


### Mode 3: Dynamic Mesh @ 100mbps

**Parameters:**
- Bandwidth: 102400 Kbps
- Duration: 60s
- Nodes: 12

**Results:**
- Messages Sent: 6
- Documents Inserted: 4
- Documents Received: 132
- Avg Latency: 21.1ms
- p50 Latency: 13.886ms
- p90 Latency: 17.966ms


### Mode 3: Dynamic Mesh @ 1mbps

**Parameters:**
- Bandwidth: 1024 Kbps
- Duration: 90s
- Nodes: 12

**Results:**
- Messages Sent: 9
- Documents Inserted: 6
- Documents Received: 176
- Avg Latency: 20.5ms
- p50 Latency: 16.816ms
- p90 Latency: 39.039ms


### Mode 3: Dynamic Mesh @ 256kbps

**Parameters:**
- Bandwidth: 256 Kbps
- Duration: 90s
- Nodes: 12

**Results:**
- Messages Sent: 9
- Documents Inserted: 6
- Documents Received: 175
- Avg Latency: 14.2ms
- p50 Latency: 13.648ms
- p90 Latency: 17.586ms


### Mode 4: Hierarchical Aggregation @ 1gbps

**Parameters:**
- Bandwidth: 1048576 Kbps
- Duration: 60s
- Nodes: 24 (3 squads + 1 platoon leader)

**Results:**
- Messages Sent: 24
- Documents Inserted: 16
- Documents Received: 186
- Avg Latency: 205.6ms
- p50 Latency: 94.866ms
- p90 Latency: 510.729ms

**Hierarchical Aggregation:**
- Squad Aggregations: test-bandwidth-suite-20251109-191705/mode4-1gbps/squad-alpha-leader.log:21
test-bandwidth-suite-20251109-191705/mode4-1gbps/squad-bravo-leader.log:22
test-bandwidth-suite-20251109-191705/mode4-1gbps/squad-charlie-leader.log:22
- Platoon Aggregations: 20
- Theoretical Bandwidth Reduction: 95.3%


### Mode 4: Hierarchical Aggregation @ 100mbps

**Parameters:**
- Bandwidth: 102400 Kbps
- Duration: 60s
- Nodes: 24 (3 squads + 1 platoon leader)

**Results:**
- Messages Sent: 24
- Documents Inserted: 16
- Documents Received: 189
- Avg Latency: 133.4ms
- p50 Latency: 69.997ms
- p90 Latency: 270.851ms

**Hierarchical Aggregation:**
- Squad Aggregations: test-bandwidth-suite-20251109-191705/mode4-100mbps/squad-alpha-leader.log:22
test-bandwidth-suite-20251109-191705/mode4-100mbps/squad-bravo-leader.log:22
test-bandwidth-suite-20251109-191705/mode4-100mbps/squad-charlie-leader.log:21
- Platoon Aggregations: 21
- Theoretical Bandwidth Reduction: 95.3%


### Mode 4: Hierarchical Aggregation @ 1mbps

**Parameters:**
- Bandwidth: 1024 Kbps
- Duration: 90s
- Nodes: 24 (3 squads + 1 platoon leader)

**Results:**
- Messages Sent: 36
- Documents Inserted: 24
- Documents Received: 249
- Avg Latency: 242.9ms
- p50 Latency: 140.082ms
- p90 Latency: 591.113ms

**Hierarchical Aggregation:**
- Squad Aggregations: test-bandwidth-suite-20251109-191705/mode4-1mbps/squad-alpha-leader.log:28
test-bandwidth-suite-20251109-191705/mode4-1mbps/squad-bravo-leader.log:27
test-bandwidth-suite-20251109-191705/mode4-1mbps/squad-charlie-leader.log:27
- Platoon Aggregations: 26
- Theoretical Bandwidth Reduction: 95.3%


### Mode 4: Hierarchical Aggregation @ 256kbps

**Parameters:**
- Bandwidth: 256 Kbps
- Duration: 90s
- Nodes: 24 (3 squads + 1 platoon leader)

**Results:**
- Messages Sent: 36
- Documents Inserted: 24
- Documents Received: 252
- Avg Latency: 212.4ms
- p50 Latency: 121.911ms
- p90 Latency: 498.285ms

**Hierarchical Aggregation:**
- Squad Aggregations: test-bandwidth-suite-20251109-191705/mode4-256kbps/squad-alpha-leader.log:28
test-bandwidth-suite-20251109-191705/mode4-256kbps/squad-bravo-leader.log:28
test-bandwidth-suite-20251109-191705/mode4-256kbps/squad-charlie-leader.log:27
- Platoon Aggregations: 26
- Theoretical Bandwidth Reduction: 95.3%


---

## Summary

**Test Suite Completion:**
- Total Tests: 16 (4 modes × 4 bandwidths)
- All modes validated across bandwidth constraints
- Results demonstrate HIVE protocol scalability from gigabit ethernet to tactical radio bandwidths

**Key Findings:**
1. Mode 4 (Hierarchical) achieves >95% bandwidth reduction through aggregation
2. All modes maintain functionality across bandwidth constraints
3. P2P latency remains acceptable even at low bandwidths
4. Hierarchical aggregation enables tactical edge deployment

**Test Date:** Sun Nov  9 07:49:08 PM EST 2025

