# hive-sim-node: HIVE Protocol Reference Implementation
#
# Multi-stage build for minimal runtime image
# This Docker image runs the HIVE Protocol simulation node with Ditto CRDT backend

# ============================================================================
# Stage 1: Builder - Build the hive-sim binary
# ============================================================================
FROM rust:1.86-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /build

# Copy entire workspace (needed for workspace dependencies)
COPY . .

# Build the hive-sim binary (release mode with optimizations)
RUN cargo build --release -p hive-sim

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

# Copy the compiled binary from builder stage
COPY --from=builder /build/target/release/hive-sim /usr/local/bin/hive-sim

# Copy entrypoint script
COPY hive-sim/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# Default environment variables (can be overridden in containerlab topology)
ENV DITTO_APP_ID="" \
    DITTO_OFFLINE_TOKEN="" \
    DITTO_SHARED_KEY="" \
    NODE_ID="node1" \
    MODE="reader" \
    LOG_LEVEL="info"

# Expose potential ports (Ditto can use dynamic ports, but good to document)
EXPOSE 12345

ENTRYPOINT ["/entrypoint.sh"]
