#!/bin/bash
# E8 Performance Test Suite - Three-Way Comparison
#
# Runs comprehensive performance tests across:
# 1. Ditto Baseline (pure CRDT, no CAP)
# 2. CAP Full Replication (CAP overhead, n-squared data)
# 3. CAP Differential (CAP with filtered replication)
#
# Each configuration tested across:
# - 4 bandwidth levels: 100Mbps, 10Mbps, 1Mbps, 256Kbps
# - 3 topology modes: Client-Server, Hub-Spoke, Dynamic Mesh
# Total: 36 tests (3 configs × 12 combinations)
#
# Estimated time: ~35-40 minutes total

set -e

cd "$(dirname "$0")"

echo "╔════════════════════════════════════════════════════════════╗"
echo "║   E8 Performance Test Suite - Three-Way Comparison        ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Check Docker image is built
if ! docker image inspect cap-sim-node:latest >/dev/null 2>&1; then
    echo "❌ Error: cap-sim-node:latest not found"
    echo "   Run: cd .. && make sim-build"
    exit 1
fi

echo "✓ Docker image found: cap-sim-node:latest"
echo ""

# Create master results directory
MASTER_TIMESTAMP=$(date +%Y%m%d-%H%M%S)
MASTER_RESULTS_DIR="e8-performance-results-$MASTER_TIMESTAMP"
mkdir -p "$MASTER_RESULTS_DIR"

echo "📁 Results will be saved to: $MASTER_RESULTS_DIR"
echo ""

# Test 1: Ditto Baseline (pure CRDT)
echo "═══════════════════════════════════════════════════════════"
echo "Test Suite 1/3: Ditto Baseline (Pure CRDT, No CAP)"
echo "═══════════════════════════════════════════════════════════"
echo "Configuration: USE_BASELINE=true"
echo "Binary: ditto_baseline"
echo "Estimated time: ~12 minutes"
echo ""

if [ -f "./test-bandwidth-baseline.sh" ]; then
    echo "▶ Starting Ditto Baseline tests..."
    ./test-bandwidth-baseline.sh 2>&1 | tee "$MASTER_RESULTS_DIR/1-ditto-baseline.log"

    # Move results into master directory
    BASELINE_DIR=$(ls -dt test-results-baseline-* | head -1)
    if [ -n "$BASELINE_DIR" ]; then
        mv "$BASELINE_DIR" "$MASTER_RESULTS_DIR/1-ditto-baseline"
        echo "✓ Ditto Baseline tests complete"
    fi
else
    echo "⚠️  Warning: test-bandwidth-baseline.sh not found, skipping"
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
echo "║   E8 Performance Test Suite Complete!                     ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "📊 Results Summary:"
echo "   Master Directory: $MASTER_RESULTS_DIR/"
echo ""
echo "   1. Ditto Baseline:        $MASTER_RESULTS_DIR/1-ditto-baseline/"
echo "   2. CAP Full Replication:  $MASTER_RESULTS_DIR/2-cap-full-replication/"
echo "   3. CAP Differential:      $MASTER_RESULTS_DIR/3-cap-differential/"
echo ""
echo "📈 View individual summaries:"
echo "   cat $MASTER_RESULTS_DIR/1-ditto-baseline/COMPREHENSIVE_SUMMARY.md"
echo "   cat $MASTER_RESULTS_DIR/2-cap-full-replication/COMPREHENSIVE_SUMMARY.md"
echo "   cat $MASTER_RESULTS_DIR/3-cap-differential/COMPREHENSIVE_SUMMARY.md"
echo ""
echo "📝 Next Steps:"
echo "   1. Run: make e8-compare-results DIR=$MASTER_RESULTS_DIR"
echo "   2. Review: E8-THREE-WAY-COMPARISON.md"
echo "   3. Update: docs/adrs/ADR-008-network-simulation-approach.md"
echo ""
