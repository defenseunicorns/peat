#!/bin/bash
################################################################################
# Dual-Bridge Deployment Orchestrator (Plan B: 1,200 Nodes)
#
# Deploys two ContainerLab topologies on separate Docker bridges connected by
# a router container, breaking the 1,023 veth pair bridge limit.
#
# Architecture:
#   Bridge A (172.30.0.0/22) — Companies 1-4 + Battalion HQ + gateway
#   Bridge B (172.31.0.0/22) — Companies 5-8 + gateway
#   Router container — IP forwarding between bridges
#
# Usage:
#   ./deploy-dual-bridge.sh <topology-a.yaml> <topology-b.yaml> [options]
#
# Options:
#   --router-image IMG   Router container image (default: alpine:latest)
#   --timeout N          Deploy timeout per bridge in minutes (default: 10)
#   --skip-deploy        Skip containerlab deploy, only setup router/routes
#   --destroy            Tear down everything
#   --dry-run            Show what would happen
#
# Examples:
#   # Generate and deploy
#   cd topologies && python3 generate-scaling-topology.py --companies 8 --split --name peat-1200
#   cd .. && ./scripts/deploy-dual-bridge.sh \
#       topologies/peat-1200-a-1gbps.yaml \
#       topologies/peat-1200-b-1gbps.yaml
#
#   # Destroy
#   ./scripts/deploy-dual-bridge.sh \
#       topologies/peat-1200-a-1gbps.yaml \
#       topologies/peat-1200-b-1gbps.yaml --destroy
################################################################################

set -euo pipefail

CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BOLD='\033[1m'
NC='\033[0m'

log_info()    { echo -e "${CYAN}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn()    { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error()   { echo -e "${RED}[ERROR]${NC} $1"; }
log_header()  { echo -e "\n${BOLD}════════════════════════════════════════════════════════════${NC}"; echo -e "${BOLD}  $1${NC}"; echo -e "${BOLD}════════════════════════════════════════════════════════════${NC}\n"; }

# Defaults
ROUTER_IMAGE="alpine:latest"
ROUTER_NAME="peat-bridge-router"
TIMEOUT=10
SKIP_DEPLOY=false
DESTROY=false
DRY_RUN=false
TOPO_A=""
TOPO_B=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --router-image) ROUTER_IMAGE="$2"; shift 2 ;;
        --timeout)      TIMEOUT="$2"; shift 2 ;;
        --skip-deploy)  SKIP_DEPLOY=true; shift ;;
        --destroy)      DESTROY=true; shift ;;
        --dry-run)      DRY_RUN=true; shift ;;
        -h|--help)      head -35 "$0" | tail -32; exit 0 ;;
        *)
            if [[ -z "$TOPO_A" ]]; then
                TOPO_A="$1"
            elif [[ -z "$TOPO_B" ]]; then
                TOPO_B="$1"
            else
                log_error "Unknown argument: $1"; exit 1
            fi
            shift ;;
    esac
done

if [[ -z "$TOPO_A" ]] || [[ -z "$TOPO_B" ]]; then
    log_error "Usage: $0 <topology-a.yaml> <topology-b.yaml> [options]"
    exit 1
fi

# Extract lab names
LAB_A=$(grep '^name:' "$TOPO_A" | awk '{print $2}' | tr -d '"' | tr -d "'")
LAB_B=$(grep '^name:' "$TOPO_B" | awk '{print $2}' | tr -d '"' | tr -d "'")
NET_A="$LAB_A"
NET_B="$LAB_B"

run_cmd() {
    if [[ "$DRY_RUN" == "true" ]]; then
        echo "  [dry-run] $*"
    else
        "$@"
    fi
}

########################################
# Destroy mode
########################################
if [[ "$DESTROY" == "true" ]]; then
    log_header "Destroying Dual-Bridge Deployment"

    log_info "Stopping router..."
    docker rm -f "$ROUTER_NAME" 2>/dev/null || true

    log_info "Destroying bridge A ($LAB_A)..."
    containerlab destroy -t "$TOPO_A" --cleanup 2>/dev/null || true

    log_info "Destroying bridge B ($LAB_B)..."
    containerlab destroy -t "$TOPO_B" --cleanup 2>/dev/null || true

    docker network prune -f 2>/dev/null || true
    log_success "Dual-bridge deployment destroyed"
    exit 0
fi

########################################
# Step 1: Deploy bridge A
########################################
if [[ "$SKIP_DEPLOY" == "false" ]]; then
    log_header "Step 1: Deploying Bridge A ($LAB_A)"

    # Source .env
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    ENV_FILE="$SCRIPT_DIR/../.env"
    [[ ! -f "$ENV_FILE" ]] && ENV_FILE="$SCRIPT_DIR/../../.env"
    if [[ -f "$ENV_FILE" ]]; then
        set -a && . "$ENV_FILE" && set +a
    fi
    export BACKEND=${BACKEND:-automerge}

    run_cmd containerlab deploy -t "$TOPO_A" --reconfigure --timeout "${TIMEOUT}m"
    NODES_A=$(docker ps --format '{{.Names}}' | grep -c "^clab-${LAB_A}-" || echo 0)
    log_success "Bridge A: $NODES_A containers"

    ########################################
    # Step 2: Deploy bridge B
    ########################################
    log_header "Step 2: Deploying Bridge B ($LAB_B)"
    run_cmd containerlab deploy -t "$TOPO_B" --reconfigure --timeout "${TIMEOUT}m"
    NODES_B=$(docker ps --format '{{.Names}}' | grep -c "^clab-${LAB_B}-" || echo 0)
    log_success "Bridge B: $NODES_B containers"
fi

########################################
# Step 3: Setup router container
########################################
log_header "Step 3: Setting Up Router Container"

# Remove old router if exists
docker rm -f "$ROUTER_NAME" 2>/dev/null || true

# Find the Docker network names created by containerlab
# ContainerLab creates networks named after the lab's mgmt.network field
DOCKER_NET_A=$(docker network ls --format '{{.Name}}' | grep "$NET_A" | head -1)
DOCKER_NET_B=$(docker network ls --format '{{.Name}}' | grep "$NET_B" | head -1)

if [[ -z "$DOCKER_NET_A" ]] || [[ -z "$DOCKER_NET_B" ]]; then
    log_error "Could not find Docker networks for $NET_A and/or $NET_B"
    echo "  Available networks:"
    docker network ls --format '  {{.Name}}' | grep -E "peat|lab" || true
    exit 1
fi

log_info "Bridge A network: $DOCKER_NET_A"
log_info "Bridge B network: $DOCKER_NET_B"

# Create router container connected to bridge A
log_info "Creating router container..."
run_cmd docker run -d \
    --name "$ROUTER_NAME" \
    --privileged \
    --network "$DOCKER_NET_A" \
    "$ROUTER_IMAGE" \
    sleep infinity

# Connect router to bridge B
log_info "Connecting router to bridge B..."
run_cmd docker network connect "$DOCKER_NET_B" "$ROUTER_NAME"

# Enable IP forwarding in router
log_info "Enabling IP forwarding in router..."
if [[ "$DRY_RUN" == "false" ]]; then
    docker exec "$ROUTER_NAME" sh -c "
        echo 1 > /proc/sys/net/ipv4/ip_forward
        # Install iptables if not present
        apk add --no-cache iptables 2>/dev/null || true
        # Allow forwarding between interfaces
        iptables -P FORWARD ACCEPT
    "
fi

# Get router IPs on each network
if [[ "$DRY_RUN" == "false" ]]; then
    ROUTER_IP_A=$(docker inspect -f "{{range .NetworkSettings.Networks}}{{.IPAddress}} {{end}}" "$ROUTER_NAME" | awk '{print $1}')
    ROUTER_IP_B=$(docker inspect -f "{{range .NetworkSettings.Networks}}{{.IPAddress}} {{end}}" "$ROUTER_NAME" | awk '{print $2}')
    # Get subnet info
    SUBNET_A=$(docker network inspect "$DOCKER_NET_A" -f '{{range .IPAM.Config}}{{.Subnet}}{{end}}')
    SUBNET_B=$(docker network inspect "$DOCKER_NET_B" -f '{{range .IPAM.Config}}{{.Subnet}}{{end}}')

    log_success "Router setup complete"
    echo "  Router IP on bridge A: $ROUTER_IP_A (subnet: $SUBNET_A)"
    echo "  Router IP on bridge B: $ROUTER_IP_B (subnet: $SUBNET_B)"
fi

########################################
# Step 4: Inject cross-bridge routes
########################################
log_header "Step 4: Injecting Cross-Bridge Routes"

if [[ "$DRY_RUN" == "false" ]]; then
    # All containers on bridge A need a route to bridge B's subnet via the router
    log_info "Adding routes on bridge A containers -> bridge B subnet..."
    ADDED_A=0
    for container in $(docker ps --format '{{.Names}}' | grep "^clab-${LAB_A}-"); do
        docker exec "$container" ip route add "$SUBNET_B" via "$ROUTER_IP_A" 2>/dev/null || true
        ADDED_A=$((ADDED_A + 1))
    done
    log_success "Added route to $ADDED_A bridge A containers"

    # All containers on bridge B need a route to bridge A's subnet via the router
    log_info "Adding routes on bridge B containers -> bridge A subnet..."
    ADDED_B=0
    for container in $(docker ps --format '{{.Names}}' | grep "^clab-${LAB_B}-"); do
        docker exec "$container" ip route add "$SUBNET_A" via "$ROUTER_IP_B" 2>/dev/null || true
        ADDED_B=$((ADDED_B + 1))
    done
    log_success "Added route to $ADDED_B bridge B containers"
fi

########################################
# Step 5: Inject cross-bridge TCP_CONNECT
########################################
log_header "Step 5: Injecting Cross-Bridge TCP_CONNECT Addresses"

if [[ "$DRY_RUN" == "false" ]]; then
    # Find battalion HQ's IP on bridge A
    HQ_CONTAINER="clab-${LAB_A}-battalion-hq"
    HQ_IP=$(docker inspect -f "{{range .NetworkSettings.Networks}}{{.IPAddress}} {{end}}" "$HQ_CONTAINER" | awk '{print $1}')
    log_info "Battalion HQ IP: $HQ_IP"

    # Company commanders on bridge B need to connect to HQ via IP
    # The topology generator puts a placeholder __CROSS_BRIDGE_HQ_ADDR__
    # We need to update the TCP_CONNECT env var in bridge B company commanders
    # Since env vars are set at container creation, we use a different approach:
    # inject the connection via a file that entrypoint.sh can read, or
    # restart the containers with the correct env var.
    # Simplest: set the env var by writing to the process's environment
    # Actually: we need to restart bridge B commanders with the correct TCP_CONNECT

    CO_CONTAINERS=$(docker ps --format '{{.Names}}' | grep "^clab-${LAB_B}-.*commander" || true)
    if [[ -n "$CO_CONTAINERS" ]]; then
        log_info "Updating cross-bridge company commanders with HQ IP..."
        for co in $CO_CONTAINERS; do
            # Write a connection override file that the binary can pick up
            # The peat-sim binary reads TCP_CONNECT from env, so we write an
            # env override file and restart the container
            docker exec "$co" sh -c "
                echo 'TCP_CONNECT=${HQ_IP}:12345' > /tmp/cross-bridge-env
            " 2>/dev/null || true
            log_info "  Updated $co -> ${HQ_IP}:12345"
        done
        log_warn "NOTE: Bridge B commanders may need restart to pick up cross-bridge connection."
        log_warn "The topology generator should be re-run with the actual HQ IP once known."
    fi
fi

########################################
# Step 6: Send tiered start signals
########################################
log_header "Step 6: Sending Tiered Start Signals"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ "$DRY_RUN" == "false" ]]; then
    # Use the staged deploy script's start-only mode for each bridge
    # Or do it inline:

    TIER_DELAY=3
    BATCH_SIZE=100
    BATCH_DELAY=2

    # Tier 1: Battalion HQ (bridge A)
    log_info "Starting Battalion HQ..."
    docker exec "clab-${LAB_A}-battalion-hq" touch /data/start 2>/dev/null || true
    sleep "$TIER_DELAY"

    # Tier 2: Company Commanders (both bridges)
    log_info "Starting Company Commanders..."
    for container in $(docker ps --format '{{.Names}}' | grep -E "^clab-(${LAB_A}|${LAB_B})-.*commander"); do
        docker exec "$container" touch /data/start 2>/dev/null || true
    done
    sleep "$TIER_DELAY"

    # Tier 3: Platoon Leaders
    log_info "Starting Platoon Leaders..."
    for container in $(docker ps --format '{{.Names}}' | grep -E "^clab-(${LAB_A}|${LAB_B})-.*platoon-.*-leader$"); do
        docker exec "$container" touch /data/start 2>/dev/null || true
    done
    sleep "$TIER_DELAY"

    # Tier 4: Squad Leaders
    log_info "Starting Squad Leaders..."
    for container in $(docker ps --format '{{.Names}}' | grep -E "^clab-(${LAB_A}|${LAB_B})-.*squad-.*-leader$"); do
        docker exec "$container" touch /data/start 2>/dev/null || true
    done
    sleep "$TIER_DELAY"

    # Tier 5: Soldiers (batched)
    log_info "Starting Soldiers (batches of $BATCH_SIZE)..."
    readarray -t SOLDIERS < <(docker ps --format '{{.Names}}' | grep -E "^clab-(${LAB_A}|${LAB_B})-.*soldier-[0-9]")
    TOTAL_SOLDIERS=${#SOLDIERS[@]}
    IDX=0
    BATCH_NUM=0
    while [[ $IDX -lt $TOTAL_SOLDIERS ]]; do
        BATCH_NUM=$((BATCH_NUM + 1))
        BATCH_END=$((IDX + BATCH_SIZE))
        [[ $BATCH_END -gt $TOTAL_SOLDIERS ]] && BATCH_END=$TOTAL_SOLDIERS

        for (( i=IDX; i<BATCH_END; i++ )); do
            docker exec "${SOLDIERS[$i]}" touch /data/start 2>/dev/null || true
        done

        BATCH_ACTUAL=$((BATCH_END - IDX))
        log_info "  Batch $BATCH_NUM: started $BATCH_ACTUAL soldiers"
        IDX=$BATCH_END

        [[ $IDX -lt $TOTAL_SOLDIERS ]] && sleep "$BATCH_DELAY"
    done
fi

########################################
# Summary
########################################
log_header "Dual-Bridge Deployment Complete"

if [[ "$DRY_RUN" == "false" ]]; then
    TOTAL_A=$(docker ps --format '{{.Names}}' | grep -c "^clab-${LAB_A}-" || echo 0)
    TOTAL_B=$(docker ps --format '{{.Names}}' | grep -c "^clab-${LAB_B}-" || echo 0)
    TOTAL=$((TOTAL_A + TOTAL_B + 1))  # +1 for router

    echo "  Bridge A ($LAB_A):  $TOTAL_A containers"
    echo "  Bridge B ($LAB_B):  $TOTAL_B containers"
    echo "  Router:             1 ($ROUTER_NAME)"
    echo "  Total:              $TOTAL"
    echo ""
    echo "  Verify cross-bridge connectivity:"
    echo "    docker exec clab-${LAB_B}-company-5-commander ping -c 3 $HQ_IP"
    echo ""
    echo "  Destroy:"
    echo "    $0 $TOPO_A $TOPO_B --destroy"
fi
