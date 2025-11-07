#!/bin/bash
set -e

# cap-sim-node entrypoint
# Runs CAP Protocol simulation node with Ditto in ContainerLab

echo "[${NODE_ID}] CAP Protocol Simulation Node starting"
echo "[${NODE_ID}] Mode: ${MODE}"
echo "[${NODE_ID}] Container IP: $(hostname -I)"

# Check required environment variables
if [ -z "$DITTO_APP_ID" ]; then
    echo "[${NODE_ID}] ERROR: DITTO_APP_ID not set"
    exit 1
fi

if [ -z "$DITTO_OFFLINE_TOKEN" ]; then
    echo "[${NODE_ID}] ERROR: DITTO_OFFLINE_TOKEN not set"
    exit 1
fi

if [ -z "$DITTO_SHARED_KEY" ]; then
    echo "[${NODE_ID}] ERROR: DITTO_SHARED_KEY not set"
    exit 1
fi

# Parse TCP configuration from environment
# TCP_LISTEN: port to listen on (for writers/servers)
# TCP_CONNECT: address:port to connect to (for readers/clients)
# BACKEND: sync backend type (default: ditto)
# NODE_TYPE: Type of node (soldier, uav, ugv) - for traffic analysis
# UPDATE_RATE_MS: Update frequency in milliseconds for writer nodes
# USE_BASELINE: If "true", use shadow_poc (baseline Ditto) instead of cap_sim_node

USE_BASELINE=${USE_BASELINE:-false}
BACKEND=${BACKEND:-ditto}
NODE_TYPE=${NODE_TYPE:-unknown}
UPDATE_RATE_MS=${UPDATE_RATE_MS:-5000}
CAP_FILTER_ENABLED=${CAP_FILTER_ENABLED:-false}

ARGS="--node-id ${NODE_ID} --mode ${MODE} --backend ${BACKEND} --node-type ${NODE_TYPE} --update-rate-ms ${UPDATE_RATE_MS}"

# Add CAP filter flag if enabled
if [ "$CAP_FILTER_ENABLED" = "true" ]; then
    ARGS="$ARGS --cap-filter"
fi

# Export CAP_FILTER_ENABLED for the binary to read
export CAP_FILTER_ENABLED

if [ -n "$TCP_LISTEN" ]; then
    ARGS="$ARGS --tcp-listen ${TCP_LISTEN}"
    echo "[${NODE_ID}] TCP: Will listen on port ${TCP_LISTEN}"
fi

if [ -n "$TCP_CONNECT" ]; then
    ARGS="$ARGS --tcp-connect ${TCP_CONNECT}"
    echo "[${NODE_ID}] TCP: Will connect to ${TCP_CONNECT}"
fi

# Export Ditto environment variables
export DITTO_APP_ID
export DITTO_OFFLINE_TOKEN
export DITTO_SHARED_KEY

# Run the simulation node (trait-based with DQL v2 or baseline)
if [ "$USE_BASELINE" = "true" ]; then
    echo "[${NODE_ID}] Running BASELINE (ditto_baseline - raw Ditto without CAP)"
    # ditto_baseline doesn't support all the args, only basic ones
    BASELINE_ARGS="--node-id ${NODE_ID} --mode ${MODE}"
    if [ -n "$TCP_LISTEN" ]; then
        BASELINE_ARGS="$BASELINE_ARGS --tcp-listen ${TCP_LISTEN}"
    fi
    if [ -n "$TCP_CONNECT" ]; then
        BASELINE_ARGS="$BASELINE_ARGS --tcp-connect ${TCP_CONNECT}"
    fi
    exec /app/target/release/examples/ditto_baseline $BASELINE_ARGS
else
    echo "[${NODE_ID}] Running CAP Protocol (cap_sim_node with backend: ${BACKEND})"
    exec /app/target/release/examples/cap_sim_node $ARGS
fi
