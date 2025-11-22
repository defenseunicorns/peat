#!/bin/bash
set -e

# hive-sim-node entrypoint
# Runs CAP Protocol simulation node with Ditto in ContainerLab

echo "[${NODE_ID}] CAP Protocol Simulation Node starting"
echo "[${NODE_ID}] Mode: ${MODE}"
echo "[${NODE_ID}] Container IP: $(hostname -I)"

# Check if we're using traditional baseline (no Ditto required)
USE_TRADITIONAL=${USE_TRADITIONAL:-false}

# Check required environment variables (skip for traditional baseline)
if [ "$USE_TRADITIONAL" != "true" ]; then
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
fi

# Parse TCP configuration from environment
# TCP_LISTEN: port to listen on (for writers/servers)
# TCP_CONNECT: address:port to connect to (for readers/clients)
# BACKEND: sync backend type (default: ditto)
# NODE_TYPE: Type of node (soldier, uav, ugv) - for traffic analysis
# UPDATE_RATE_MS: Update frequency in milliseconds for writer nodes
# USE_BASELINE: If "true", use ditto_baseline (CRDT without CAP)
# USE_TRADITIONAL: If "true", use traditional_baseline (NO CRDT, periodic full messages)
# UPDATE_FREQUENCY_SECS: For traditional_baseline - period in seconds (default: 5)

USE_BASELINE=${USE_BASELINE:-false}
USE_TRADITIONAL=${USE_TRADITIONAL:-false}
BACKEND=${BACKEND:-ditto}
NODE_TYPE=${NODE_TYPE:-unknown}
UPDATE_RATE_MS=${UPDATE_RATE_MS:-5000}
UPDATE_FREQUENCY_SECS=${UPDATE_FREQUENCY_SECS:-5}
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

# Apply bandwidth constraints if specified
if [ -n "$BANDWIDTH" ]; then
    echo "[${NODE_ID}] Applying bandwidth constraint: $BANDWIDTH"
    # Convert bandwidth string to tc format (e.g., "256kbps" -> "256kbit")
    TC_RATE=$(echo "$BANDWIDTH" | sed 's/bps/bit/g')
    # Apply Token Bucket Filter (TBF) on eth0
    tc qdisc add dev eth0 root tbf rate "$TC_RATE" burst 32kbit latency 400ms 2>/dev/null || \
        echo "[${NODE_ID}] Warning: Failed to apply bandwidth constraint (may already exist or no permission)"
fi

# Export Ditto environment variables
export DITTO_APP_ID
export DITTO_OFFLINE_TOKEN
export DITTO_SHARED_KEY

# Run the appropriate simulation node
if [ "$USE_TRADITIONAL" = "true" ]; then
    echo "[${NODE_ID}] Running Traditional Baseline (NO CRDT - periodic full messages)"

    # Traditional baseline uses different arguments
    TRAD_ARGS="--node-id ${NODE_ID}"

    if [ "$MODE" = "writer" ]; then
        TRAD_ARGS="$TRAD_ARGS --mode server"
        if [ -n "$TCP_LISTEN" ]; then
            TRAD_ARGS="$TRAD_ARGS --listen 0.0.0.0:${TCP_LISTEN}"
        fi
    else
        TRAD_ARGS="$TRAD_ARGS --mode client"
        if [ -n "$TCP_CONNECT" ]; then
            TRAD_ARGS="$TRAD_ARGS --connect ${TCP_CONNECT}"
        fi
    fi

    TRAD_ARGS="$TRAD_ARGS --update-frequency ${UPDATE_FREQUENCY_SECS}"
    TRAD_ARGS="$TRAD_ARGS --node-type ${NODE_TYPE}"

    exec /usr/local/bin/traditional_baseline $TRAD_ARGS
elif [ "$MODE" = "hierarchical" ]; then
    echo "[${NODE_ID}] Running Hierarchical Simulation (protocol-based implementation)"
    # Use hive-sim binary which properly uses the protocol API for hierarchical aggregation
    # This avoids coupling to Ditto-specific SDK details
    export MODE="hierarchical"  # Ensure MODE environment variable is set
    exec /usr/local/bin/hive-sim $ARGS
else
    echo "[${NODE_ID}] Running HIVE Protocol Simulation"
    exec /usr/local/bin/hive-sim $ARGS
fi
