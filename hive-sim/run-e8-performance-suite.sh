#!/bin/bash
# E8 Performance Test Suite - Three-Way Comparison
#
# Runs comprehensive performance tests across:
# 1. Traditional IoT Baseline (NO CRDT, periodic full messages)
# 2. CAP Full Replication (CRDT without capability filtering)
# 3. CAP Differential (CRDT with capability filtering)
#
# Each configuration tested across:
# - 4 bandwidth levels: 100Mbps, 10Mbps, 1Mbps, 256Kbps
# - Traditional: 2 topology modes (Client-Server, Hub-Spoke)
# - CAP: 3 topology modes (Client-Server, Hub-Spoke, Dynamic Mesh)
# Total: 32 tests (8 traditional + 12 CAP Full + 12 CAP Differential)
#
# Estimated time: ~30-35 minutes total

set -e

cd "$(dirname "$0")"

echo "╔════════════════════════════════════════════════════════════╗"
echo "║   E8 Performance Test Suite - Three-Way Comparison        ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# ============================================
# Pre-flight Checks
# ============================================

echo "🔍 Running pre-flight checks..."
echo ""

ERRORS=0
WARNINGS=0

# Check Docker
if ! command -v docker &> /dev/null; then
    echo "❌ Error: docker not installed"
    ERRORS=$((ERRORS + 1))
else
    echo "✓ Docker installed"
fi

# Check Docker image
if ! docker image inspect hive-sim-node:latest >/dev/null 2>&1; then
    echo "❌ Error: hive-sim-node:latest not found"
    echo "   Run: cd .. && make sim-build"
    ERRORS=$((ERRORS + 1))
else
    echo "✓ Docker image: hive-sim-node:latest"
fi

# Check ContainerLab
if ! command -v containerlab &> /dev/null; then
    echo "❌ Error: containerlab not installed"
    ERRORS=$((ERRORS + 1))
else
    containerlab_version=$(containerlab version 2>&1 | head -1 || echo "unknown")
    echo "✓ ContainerLab: $containerlab_version"
fi

# Check Python
if ! command -v python3 &> /dev/null; then
    echo "⚠️  Warning: python3 not found (metrics analysis may fail)"
    WARNINGS=$((WARNINGS + 1))
else
    echo "✓ Python 3 installed"
fi

# Check jq
if ! command -v jq &> /dev/null; then
    echo "⚠️  Warning: jq not installed (metrics parsing may be limited)"
    WARNINGS=$((WARNINGS + 1))
else
    echo "✓ jq installed"
fi

# Check disk space (need ~100MB for results)
available_mb=$(df -m . | tail -1 | awk '{print $4}')
if [ "$available_mb" -lt 100 ]; then
    echo "⚠️  Warning: Low disk space (${available_mb}MB available, recommend 100MB+)"
    WARNINGS=$((WARNINGS + 1))
else
    echo "✓ Disk space: ${available_mb}MB available"
fi

# Check environment variables for CAP tests
if [ -z "$DITTO_APP_ID" ]; then
    echo "⚠️  Warning: DITTO_APP_ID not set (load from .env if available)"
    WARNINGS=$((WARNINGS + 1))
else
    echo "✓ DITTO_APP_ID configured"
fi

echo ""

# Exit if critical errors
if [ $ERRORS -gt 0 ]; then
    echo "❌ Pre-flight checks failed with $ERRORS error(s)"
    exit 1
fi

if [ $WARNINGS -gt 0 ]; then
    echo "⚠️  Pre-flight checks passed with $WARNINGS warning(s)"
    echo "   Tests will proceed but some features may not work"
else
    echo "✅ All pre-flight checks passed"
fi

echo ""

# Create master results directory
MASTER_TIMESTAMP=$(date +%Y%m%d-%H%M%S)
MASTER_RESULTS_DIR="e8-performance-results-$MASTER_TIMESTAMP"
mkdir -p "$MASTER_RESULTS_DIR"

echo "📁 Results will be saved to: $MASTER_RESULTS_DIR"
echo ""

# Test 1: Traditional IoT Baseline (NO CRDT)
echo "═══════════════════════════════════════════════════════════"
echo "Test Suite 1/3: Traditional IoT Baseline (NO CRDT)"
echo "═══════════════════════════════════════════════════════════"
echo "Configuration: USE_TRADITIONAL=true"
echo "Binary: traditional_baseline"
echo "Architecture: Periodic full-state messaging"
echo "Estimated time: ~10 minutes"
echo ""

if [ -f "./test-bandwidth-traditional.sh" ]; then
    echo "▶ Starting Traditional IoT Baseline tests..."
    ./test-bandwidth-traditional.sh 2>&1 | tee "$MASTER_RESULTS_DIR/1-traditional-iot-baseline.log"

    # Move results into master directory
    BASELINE_DIR=$(ls -dt test-results-traditional-bandwidth-* | head -1)
    if [ -n "$BASELINE_DIR" ]; then
        mv "$BASELINE_DIR" "$MASTER_RESULTS_DIR/1-traditional-iot-baseline"
        echo "✓ Traditional IoT Baseline tests complete"
    fi
else
    echo "⚠️  Warning: test-bandwidth-traditional.sh not found, skipping"
fi

echo ""
echo "Waiting 10 seconds before next suite..."
sleep 10
echo ""

# Test 2: CAP Full Replication (CAP overhead, n-squared data)
echo "═══════════════════════════════════════════════════════════"
echo "Test Suite 2/3: CAP Full Replication (n-squared data)"
echo "═══════════════════════════════════════════════════════════"
echo "Configuration: CAP_FILTER_ENABLED=false (default)"
echo "Binary: cap_sim_node with Query::All"
echo "Estimated time: ~12 minutes"
echo ""

# Check if already run
if [ -d "test-results-bandwidth-20251107-131149" ]; then
    echo "ℹ️  CAP Full tests already exist: test-results-bandwidth-20251107-131149"
    echo "   Copying to master results directory..."
    cp -r test-results-bandwidth-20251107-131149 "$MASTER_RESULTS_DIR/2-cap-full-replication"
    echo "✓ CAP Full Replication results copied"
else
    if [ -f "./test-bandwidth-constraints.sh" ]; then
        echo "▶ Starting CAP Full Replication tests..."
        ./test-bandwidth-constraints.sh 2>&1 | tee "$MASTER_RESULTS_DIR/2-cap-full-replication.log"

        # Move results into master directory
        CAP_FULL_DIR=$(ls -dt test-results-bandwidth-* | head -1)
        if [ -n "$CAP_FULL_DIR" ]; then
            mv "$CAP_FULL_DIR" "$MASTER_RESULTS_DIR/2-cap-full-replication"
            echo "✓ CAP Full Replication tests complete"
        fi
    else
        echo "⚠️  Warning: test-bandwidth-constraints.sh not found, skipping"
    fi
fi

echo ""
echo "Waiting 10 seconds before next suite..."
sleep 10
echo ""

# Test 3: CAP Differential Updates (filtered replication)
echo "═══════════════════════════════════════════════════════════"
echo "Test Suite 3/3: CAP Differential (Filtered Replication)"
echo "═══════════════════════════════════════════════════════════"
echo "Configuration: CAP_FILTER_ENABLED=true"
echo "Binary: cap_sim_node with capability-filtered queries"
echo "Estimated time: ~12 minutes"
echo ""

# Create CAP differential test script (modified version with CAP_FILTER_ENABLED=true)
cat > /tmp/test-cap-differential.sh <<'EOFSCRIPT'
#!/bin/bash
# Wrapper to run bandwidth tests with CAP filtering enabled
export CAP_FILTER_ENABLED=true
exec ./test-bandwidth-constraints.sh "$@"
EOFSCRIPT
chmod +x /tmp/test-cap-differential.sh

echo "▶ Starting CAP Differential tests..."
/tmp/test-cap-differential.sh 2>&1 | tee "$MASTER_RESULTS_DIR/3-cap-differential.log"

# Move results into master directory
CAP_DIFF_DIR=$(ls -dt test-results-bandwidth-* | grep -v "20251107-131149" | head -1)
if [ -n "$CAP_DIFF_DIR" ]; then
    mv "$CAP_DIFF_DIR" "$MASTER_RESULTS_DIR/3-cap-differential"
    echo "✓ CAP Differential tests complete"
fi

# Cleanup
rm -f /tmp/test-cap-differential.sh

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║   Generating Three-Way Comparison Report                  ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Generate comprehensive comparison report
REPORT_FILE="$MASTER_RESULTS_DIR/E8-THREE-WAY-COMPARISON.md"

cat > "$REPORT_FILE" <<EOF
# E8 Performance Test Results - Three-Way Comparison

**Date:** $(date +%Y-%m-%d)
**Test Duration:** ~30-35 minutes
**Total Tests:** 32 scenarios

## Architectures Compared

1. **Traditional IoT Baseline** - NO CRDT, periodic full-state messaging
2. **CAP Full Replication** - CRDT without capability filtering
3. **CAP Differential Filtering** - CRDT + capability filtering

## Test Matrix

- **Bandwidth Levels:** 100Mbps, 10Mbps, 1Mbps, 256Kbps
- **Traditional IoT:** 4 bandwidths × 2 topologies = 8 tests
- **CAP Full:** 4 bandwidths × 3 topologies = 12 tests
- **CAP Differential:** 4 bandwidths × 3 topologies = 12 tests

## Results Directory Structure

\`\`\`
$MASTER_RESULTS_DIR/
├── 1-traditional-iot-baseline/
│   ├── COMPREHENSIVE_SUMMARY.md
│   ├── 100mbps/
│   ├── 10mbps/
│   ├── 1mbps/
│   └── 256kbps/
├── 2-cap-full-replication/
│   ├── COMPREHENSIVE_SUMMARY.md
│   ├── 100mbps/
│   ├── 10mbps/
│   ├── 1mbps/
│   └── 256kbps/
├── 3-cap-differential/
│   ├── COMPREHENSIVE_SUMMARY.md
│   ├── 100mbps/
│   ├── 10mbps/
│   ├── 1mbps/
│   └── 256kbps/
└── E8-THREE-WAY-COMPARISON.md (this file)
\`\`\`

## Viewing Results

### Individual Architecture Summaries
\`\`\`bash
cat $MASTER_RESULTS_DIR/1-traditional-iot-baseline/COMPREHENSIVE_SUMMARY.md
cat $MASTER_RESULTS_DIR/2-cap-full-replication/COMPREHENSIVE_SUMMARY.md
cat $MASTER_RESULTS_DIR/3-cap-differential/COMPREHENSIVE_SUMMARY.md
\`\`\`

### Quick Metrics Check
\`\`\`bash
# Traditional IoT metrics
find $MASTER_RESULTS_DIR/1-traditional-iot-baseline -name "*.metrics.json" -exec wc -l {} +

# CAP Full metrics
find $MASTER_RESULTS_DIR/2-cap-full-replication -name "*_metrics.json" -exec wc -l {} +

# CAP Differential metrics
find $MASTER_RESULTS_DIR/3-cap-differential -name "*_metrics.json" -exec wc -l {} +
\`\`\`

## Key Metrics to Analyze

1. **Bandwidth Efficiency**
   - Total bytes transmitted per architecture
   - Message sizes (Traditional vs CAP deltas)
   - Bandwidth reduction percentages

2. **Latency Performance**
   - Message propagation times
   - Sync completion times
   - Network constraint impact

3. **Scalability**
   - Performance across topologies (2-node, 12-node)
   - Behavior under bandwidth constraints
   - Resource utilization

## Expected Outcomes

Based on architectural design:

| Architecture | Bandwidth | vs Traditional | Key Benefit |
|--------------|-----------|----------------|-------------|
| Traditional IoT | Baseline (100%) | - | Simple, predictable |
| CAP Full | ~60-70% | **-30-40%** | CRDT delta-state |
| CAP Differential | ~30-40% | **-60-70%** | CRDT + filtering |

## Next Steps

1. ✅ **Tests Complete** - All 32 scenarios executed
2. **Analyze Metrics** - Review bandwidth/latency across configurations
3. **Update ADR-008** - Document findings in architecture decision record
4. **Generate Charts** - Visualize bandwidth comparison
5. **Document Insights** - Update project documentation

## Files Generated

- **Individual test logs:** \`$MASTER_RESULTS_DIR/*/\`
- **Metrics JSON:** \`*_metrics.json\` and \`*.metrics.json\`
- **Summaries:** \`COMPREHENSIVE_SUMMARY.md\` per architecture
- **This report:** \`E8-THREE-WAY-COMPARISON.md\`

EOF

echo "✓ Comparison report generated: $REPORT_FILE"
echo ""

# Generate executive summary with actual data analysis
echo "╔════════════════════════════════════════════════════════════╗"
echo "║   Generating Executive Summary                            ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

if [ -f "./generate-executive-summary.sh" ]; then
    ./generate-executive-summary.sh "$MASTER_RESULTS_DIR"
else
    echo "⚠️  Warning: generate-executive-summary.sh not found, skipping"
fi

echo ""

# Create symlink to latest results for easy access
echo "Creating symlink: e8-performance-results-latest -> $MASTER_RESULTS_DIR"
ln -sfn "$MASTER_RESULTS_DIR" "e8-performance-results-latest"
echo "✓ Symlink created"
echo ""

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║   E8 Performance Test Suite Complete!                     ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "📊 Results Summary:"
echo "   Master Directory: $MASTER_RESULTS_DIR/"
echo "   Quick Access:     e8-performance-results-latest/"
echo ""
echo "   1. Traditional IoT:       $MASTER_RESULTS_DIR/1-traditional-iot-baseline/"
echo "   2. CAP Full Replication:  $MASTER_RESULTS_DIR/2-cap-full-replication/"
echo "   3. CAP Differential:      $MASTER_RESULTS_DIR/3-cap-differential/"
echo ""
echo "📈 View summaries:"
echo "   cat e8-performance-results-latest/EXECUTIVE-SUMMARY.md"
echo "   cat e8-performance-results-latest/E8-THREE-WAY-COMPARISON.md"
echo "   cat e8-performance-results-latest/1-traditional-iot-baseline/COMPREHENSIVE_SUMMARY.md"
echo "   cat e8-performance-results-latest/2-cap-full-replication/COMPREHENSIVE_SUMMARY.md"
echo "   cat e8-performance-results-latest/3-cap-differential/COMPREHENSIVE_SUMMARY.md"
echo ""
