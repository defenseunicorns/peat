# Port Operations Agent Container — Phase 0
#
# Co-locates:
#   1. HIVE sim node (Rust) — CRDT state sync [Phase 1+, stubbed for Phase 0]
#   2. MCP bridge server (Python) — translates agent <-> HIVE
#   3. Agent runtime (Python) — OODA loop with LLM
#
# Phase 0: Runs bridge + agent only (no HIVE sim node, in-memory state)
# Phase 1+: Adds hive-sim-node for real CRDT synchronization

FROM python:3.12-slim

WORKDIR /opt/port-ops

# Install Python dependencies
COPY bridge/pyproject.toml bridge/pyproject.toml
COPY agent/pyproject.toml agent/pyproject.toml

RUN pip install --no-cache-dir \
    ./bridge \
    ./agent

# Copy source code
COPY bridge/ bridge/
COPY agent/ agent/
COPY personas/ personas/

# Re-install with source
RUN pip install --no-cache-dir ./bridge ./agent

# Copy entrypoint
COPY docker/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# Default environment
ENV NODE_ID=crane-1
ENV PERSONA=gantry-crane
ENV LLM_PROVIDER=anthropic
ENV LLM_MODEL=""
ENV MAX_CYCLES=20
ENV TIME_COMPRESSION=60
ENV LOG_LEVEL=INFO

ENTRYPOINT ["/entrypoint.sh"]
