# hive-sim-node: HIVE Protocol Reference Implementation
#
# Multi-stage build for minimal runtime image
# This Docker image runs the HIVE Protocol simulation node with Ditto CRDT backend

# ============================================================================
# Stage 1: Builder - Build the hive-sim binary
# ============================================================================
FROM rust:1.89-bookworm AS builder

# Install build dependencies
# clang and libclang-dev required for automerge-backend's bindgen dependency
# mold is the fast linker specified in .cargo/config.toml
RUN apt-get update && apt-get install -y \
    protobuf-compiler \
    clang \
    libclang-dev \
    cmake \
    mold \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /build

# Copy entire workspace (needed for workspace dependencies)
COPY . .

# Build the hive-sim binary with both backends enabled, traditional baseline, and producer-only baseline
# Always build with automerge-backend feature for maximum flexibility
# Note: We only build hive-sim (protocol-based) and baseline examples
# The hierarchical_sim_node examples are Ditto-specific and should not be used
RUN cargo build --release -p hive-sim --features automerge-backend && \
    cargo build --release --example traditional_baseline -p hive-protocol && \
    cargo build --release --example producer_only_baseline -p hive-protocol && \
    cargo build --release --example p2p_mesh_baseline -p hive-protocol

# ============================================================================
# Stage 2: Runtime - Minimal runtime image
# ============================================================================
FROM debian:bookworm-slim

# Install only runtime dependencies
RUN apt-get update && apt-get install -y \
    iproute2 \
    iputils-ping \
    tcpdump \
    net-tools \
    curl \
    jq \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create runtime directory for Ditto persistence
RUN mkdir -p /data/ditto

# Copy the compiled binaries from builder stage
COPY --from=builder /build/target/release/hive-sim /usr/local/bin/hive-sim
COPY --from=builder /build/target/release/examples/traditional_baseline /usr/local/bin/traditional_baseline
COPY --from=builder /build/target/release/examples/producer_only_baseline /usr/local/bin/producer_only_baseline
COPY --from=builder /build/target/release/examples/p2p_mesh_baseline /usr/local/bin/p2p_mesh_baseline

# Copy entrypoint script (relative to build context which is parent dir)
COPY hive-sim/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# Default environment variables (can be overridden in containerlab topology)
# Note: Secrets (HIVE_SHARED_KEY, HIVE_SECRET_KEY) should be passed at runtime
# via docker run -e or containerlab env, not baked into image
ENV NODE_ID="node1" \
    MODE="reader" \
    LOG_LEVEL="info"

# Expose ports for P2P communication
EXPOSE 12345

ENTRYPOINT ["/entrypoint.sh"]
