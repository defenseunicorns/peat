#!/bin/bash
# Port Operations Agent Entrypoint
#
# Phase 0: Runs MCP bridge (in-process) + agent
# Phase 1+: Will also start hive-sim-node for CRDT sync

set -e

echo "=== Port Operations Agent ==="
echo "  Node ID:     ${NODE_ID}"
echo "  Persona:     ${PERSONA}"
echo "  LLM:         ${LLM_PROVIDER} ${LLM_MODEL}"
echo "  Max Cycles:  ${MAX_CYCLES}"
echo "  Compression: ${TIME_COMPRESSION}x"
echo "=============================="

# Build CLI args
ARGS="--node-id ${NODE_ID} --persona ${PERSONA} --provider ${LLM_PROVIDER}"
ARGS="${ARGS} --max-cycles ${MAX_CYCLES} --time-compression ${TIME_COMPRESSION}"
ARGS="${ARGS} --log-level ${LOG_LEVEL}"

if [ -n "${LLM_MODEL}" ]; then
    ARGS="${ARGS} --model ${LLM_MODEL}"
fi

if [ -n "${OLLAMA_URL}" ]; then
    ARGS="${ARGS} --ollama-url ${OLLAMA_URL}"
fi

if [ -n "${METRICS_FILE}" ]; then
    ARGS="${ARGS} --metrics-file ${METRICS_FILE}"
fi

# Run the agent
exec python -m port_agent.main ${ARGS}
