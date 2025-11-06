# cap-sim-node: CAP Protocol Network Simulation Node
#
# This Docker image runs a CAP Protocol node for ContainerLab network simulation.
# It includes Ditto SDK for CRDT sync and CAP Protocol node logic.

FROM rust:1.86-bookworm

# Install system dependencies
RUN apt-get update && apt-get install -y \
    iproute2 \
    iputils-ping \
    tcpdump \
    net-tools \
    curl \
    jq \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Copy entire workspace
COPY . .

# Build BOTH binaries for comparison testing
RUN cargo build --release --example cap_sim_node
RUN cargo build --release --example shadow_poc

# Create runtime directory for Ditto persistence
RUN mkdir -p /data/ditto

# Default environment variables (can be overridden in containerlab topology)
ENV DITTO_APP_ID="" \
    DITTO_OFFLINE_TOKEN="" \
    DITTO_SHARED_KEY="" \
    NODE_ID="node1" \
    MODE="reader" \
    LOG_LEVEL="info"

# Expose potential ports (Ditto can use dynamic ports, but good to document)
EXPOSE 12345

# Entry point script
COPY cap-sim/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
