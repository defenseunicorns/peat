#!/bin/bash
# Port Operations Agent Entrypoint
#
# Phase 0: Single agent (MCP bridge in-process)
# Phase 1a: Multi-agent orchestrator (shared HiveStateStore)
# Phase 1a+clab: Multi-agent with TCP relay output

set -e

echo "=== Port Operations Agent ==="
echo "  Node ID:     ${NODE_ID}"
echo "  Mode:        ${MODE:-single}"
echo "  LLM:         ${LLM_PROVIDER} ${LLM_MODEL}"
echo "  Max Cycles:  ${MAX_CYCLES}"
echo "  Compression: ${TIME_COMPRESSION}x"
echo "=============================="

if [ "${MODE}" = "multi" ]; then
    # Phase 1a/1b: Multi-agent orchestrator
    ARGS="--mode multi --agents ${AGENTS:-2c5w4t1s1a2x}"
    ARGS="${ARGS} --provider ${LLM_PROVIDER:-dry-run}"
    ARGS="${ARGS} --max-cycles ${MAX_CYCLES:-50}"
    ARGS="${ARGS} --time-compression ${TIME_COMPRESSION:-600}"
    ARGS="${ARGS} --log-level ${LOG_LEVEL:-INFO}"

    if [ -n "${LLM_MODEL}" ]; then
        ARGS="${ARGS} --model ${LLM_MODEL}"
    fi

    if [ -n "${RELAY_HOST}" ]; then
        # Pipe stdout to relay via TCP (ContainerLab mode)
        echo "Connecting to relay at ${RELAY_HOST}:${RELAY_PORT:-9100}..."
        exec python -m port_agent.main ${ARGS} 2>/dev/null \
            | while IFS= read -r line; do
                echo "$line" > /dev/tcp/${RELAY_HOST}/${RELAY_PORT:-9100} 2>/dev/null || true
                echo "$line"
              done
    else
        exec python -m port_agent.main ${ARGS}
    fi
else
    # Phase 0: Single agent
    ARGS="--node-id ${NODE_ID} --persona ${PERSONA:-gantry-crane} --provider ${LLM_PROVIDER:-anthropic}"
    ARGS="${ARGS} --max-cycles ${MAX_CYCLES:-20} --time-compression ${TIME_COMPRESSION:-60}"
    ARGS="${ARGS} --log-level ${LOG_LEVEL:-INFO}"

    if [ -n "${LLM_MODEL}" ]; then
        ARGS="${ARGS} --model ${LLM_MODEL}"
    fi

    if [ -n "${OLLAMA_URL}" ]; then
        ARGS="${ARGS} --ollama-url ${OLLAMA_URL}"
    fi

    if [ -n "${METRICS_FILE}" ]; then
        ARGS="${ARGS} --metrics-file ${METRICS_FILE}"
    fi

    exec python -m port_agent.main ${ARGS}
fi
