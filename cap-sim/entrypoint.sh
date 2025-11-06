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

ARGS="--node-id ${NODE_ID} --mode ${MODE}"

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

# Run the simulation node
# For now, using shadow_poc as the binary
# TODO: Create proper cap-sim-node binary
exec /app/target/release/examples/shadow_poc $ARGS
