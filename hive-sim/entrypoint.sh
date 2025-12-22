#!/bin/bash
set -e

# hive-sim-node entrypoint
# Runs CAP Protocol simulation node with Ditto in ContainerLab

echo "[${NODE_ID}] CAP Protocol Simulation Node starting"
echo "[${NODE_ID}] Mode: ${MODE}"
echo "[${NODE_ID}] Container IP: $(hostname -I)"

# Staged deployment support: wait for start signal before proceeding
# This allows all containers to be deployed before starting the HIVE service
WAIT_FOR_START=${WAIT_FOR_START:-false}
if [ "$WAIT_FOR_START" = "true" ]; then
    echo "[${NODE_ID}] STAGED MODE: Waiting for start signal..."
    while [ ! -f /data/start ]; do
        sleep 0.5
    done
    echo "[${NODE_ID}] Start signal received, proceeding..."
fi

# Check if we're using traditional baseline (no Ditto required)
USE_TRADITIONAL=${USE_TRADITIONAL:-false}
USE_PRODUCER_ONLY=${USE_PRODUCER_ONLY:-false}
USE_P2P_MESH=${USE_P2P_MESH:-false}

# Check if MODE is p2p_mesh, set flag
if [ "$MODE" = "p2p_mesh" ]; then
    USE_P2P_MESH=true
fi

# Check required environment variables (skip for traditional/producer-only/p2p-mesh baseline or automerge backend)
# BACKEND defaults to "ditto" but can be overridden to "automerge" which doesn't need credentials
BACKEND=${BACKEND:-ditto}

# For ditto backend, verify HIVE credentials are set
# The Rust code in credentials.rs handles fallbacks internally
if [ "$USE_TRADITIONAL" != "true" ] && [ "$USE_PRODUCER_ONLY" != "true" ] && [ "$USE_P2P_MESH" != "true" ] && [ "$BACKEND" != "automerge" ]; then
    if [ -z "$HIVE_APP_ID" ]; then
        echo "[${NODE_ID}] ERROR: HIVE_APP_ID not set"
        exit 1
    fi
    if [ -z "$HIVE_OFFLINE_TOKEN" ]; then
        echo "[${NODE_ID}] ERROR: HIVE_OFFLINE_TOKEN not set"
        exit 1
    fi
    # Accept either HIVE_SECRET_KEY or HIVE_SHARED_KEY (credentials.rs handles both)
    if [ -z "$HIVE_SECRET_KEY" ] && [ -z "$HIVE_SHARED_KEY" ]; then
        echo "[${NODE_ID}] ERROR: HIVE_SECRET_KEY or HIVE_SHARED_KEY not set"
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

# Run the appropriate simulation node
if [ "$USE_P2P_MESH" = "true" ] || [ "$MODE" = "p2p_mesh" ]; then
    echo "[${NODE_ID}] Running P2P Mesh Baseline (Lab 3 - full mesh connectivity)"

    # P2P mesh baseline arguments
    MESH_ARGS="--node-id ${NODE_ID}"
    MESH_ARGS="$MESH_ARGS --listen-port ${LISTEN_PORT:-12345}"

    if [ -n "$PEERS" ]; then
        MESH_ARGS="$MESH_ARGS --peers ${PEERS}"
    fi

    MESH_ARGS="$MESH_ARGS --update-frequency ${UPDATE_FREQUENCY_SECS}"

    exec /usr/local/bin/p2p_mesh_baseline $MESH_ARGS
elif [ "$USE_PRODUCER_ONLY" = "true" ]; then
    echo "[${NODE_ID}] Running Producer-Only Baseline (Lab 1 - upload only, no broadcast)"

    # Producer-only baseline uses same argument format as traditional
    PROD_ARGS="--node-id ${NODE_ID}"

    if [ "$MODE" = "writer" ]; then
        PROD_ARGS="$PROD_ARGS --mode server"
        if [ -n "$TCP_LISTEN" ]; then
            PROD_ARGS="$PROD_ARGS --listen 0.0.0.0:${TCP_LISTEN}"
        fi
    else
        PROD_ARGS="$PROD_ARGS --mode client"
        if [ -n "$TCP_CONNECT" ]; then
            PROD_ARGS="$PROD_ARGS --connect ${TCP_CONNECT}"
        fi
    fi

    PROD_ARGS="$PROD_ARGS --update-frequency ${UPDATE_FREQUENCY_SECS}"

    exec /usr/local/bin/producer_only_baseline $PROD_ARGS
elif [ "$USE_TRADITIONAL" = "true" ]; then
    echo "[${NODE_ID}] Running Traditional Baseline (Lab 2 - full replication with broadcast)"

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
