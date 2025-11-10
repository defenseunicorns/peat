# Comparative Analysis

## Key Claims Validation

### Scale: 24node

#### @ 1gbps:

**Raw Metrics:**

- **traditional**:
  - Network: 2,329,450 bytes (2.33 MB)
  - Latency: p50=0.0ms, p90=0.0ms
  - Doc Receptions: 0

- **cap-full**:
  - Network: 8,160,000 bytes (8.16 MB)
  - Latency: p50=138.4ms, p90=4960.2ms
  - Doc Receptions: 198

- **cap-hierarchical**:
  - Network: 8,041,300 bytes (8.04 MB)
  - Latency: p50=482.2ms, p90=4802.8ms
  - Doc Receptions: 195

**Bandwidth Reduction:** -245.2% (Traditional → CAP Hierarchical)

#### @ 100mbps:

**Raw Metrics:**

- **traditional**:
  - Network: 2,360,690 bytes (2.36 MB)
  - Latency: p50=0.0ms, p90=0.0ms
  - Doc Receptions: 0

- **cap-hierarchical**:
  - Network: 8,046,000 bytes (8.05 MB)
  - Latency: p50=319.0ms, p90=4845.6ms
  - Doc Receptions: 204

- **cap-full**:
  - Network: 7,843,600 bytes (7.84 MB)
  - Latency: p50=414.5ms, p90=4980.6ms
  - Doc Receptions: 207

**Bandwidth Reduction:** -240.8% (Traditional → CAP Hierarchical)

#### @ 1mbps:

**Raw Metrics:**

- **traditional**:
  - Network: 2,418,820 bytes (2.42 MB)
  - Latency: p50=0.0ms, p90=0.0ms
  - Doc Receptions: 0

- **cap-hierarchical**:
  - Network: 9,148,000 bytes (9.15 MB)
  - Latency: p50=474.1ms, p90=4951.0ms
  - Doc Receptions: 258

- **cap-full**:
  - Network: 9,335,600 bytes (9.34 MB)
  - Latency: p50=220.3ms, p90=4974.8ms
  - Doc Receptions: 246

**Bandwidth Reduction:** -278.2% (Traditional → CAP Hierarchical)

#### @ 256kbps:

**Raw Metrics:**

- **traditional**:
  - Network: 2,458,770 bytes (2.46 MB)
  - Latency: p50=0.0ms, p90=0.0ms
  - Doc Receptions: 0

- **cap-hierarchical**:
  - Network: 9,334,000 bytes (9.33 MB)
  - Latency: p50=175.6ms, p90=5120.8ms
  - Doc Receptions: 252

- **cap-full**:
  - Network: 9,235,000 bytes (9.23 MB)
  - Latency: p50=377.3ms, p90=5017.6ms
  - Doc Receptions: 255

**Bandwidth Reduction:** -279.6% (Traditional → CAP Hierarchical)
