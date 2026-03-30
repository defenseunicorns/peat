#!/bin/bash
################################################################################
# Staged Deployment Orchestrator for Hierarchical CRDT Testing
#
# Deploys a ContainerLab topology and triggers tiered start signals to prevent
# container startup thundering herd at scale (1,000+ nodes).
#
# Start order: Battalion HQ -> Company COs -> Platoon Leaders -> Squad Leaders -> Soldiers
# Soldiers are started in batches to avoid overwhelming the system.
#
# Usage:
#   ./deploy-staged.sh <topology_yaml> [options]
#
# Options:
#   --batch-size N     Soldiers per batch (default: 100)
#   --batch-delay N    Seconds between batches (default: 2)
#   --tier-delay N     Seconds between tiers (default: 3)
#   --timeout N        Deploy timeout in minutes (default: 15)
#   --skip-deploy      Skip containerlab deploy (use if already deployed)
#   --start-only       Only send start signals (alias for --skip-deploy)
#   --dry-run          Show what would happen without executing
#
# Examples:
#   ./deploy-staged.sh topologies/lab-1000n-1gbps.yaml
#   ./deploy-staged.sh topologies/lab-1000n-1gbps.yaml --batch-size 50 --tier-delay 5
#   ./deploy-staged.sh topologies/lab-1000n-1gbps.yaml --start-only
################################################################################

set -euo pipefail

# Colors
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
log_header()  { echo -e "\n${BOLD}$1${NC}\n"; }

# Defaults
BATCH_SIZE=100
BATCH_DELAY=2
TIER_DELAY=3
TIMEOUT=15
SKIP_DEPLOY=false
DRY_RUN=false
TOPOLOGY=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --batch-size)    BATCH_SIZE="$2"; shift 2 ;;
        --batch-delay)   BATCH_DELAY="$2"; shift 2 ;;
        --tier-delay)    TIER_DELAY="$2"; shift 2 ;;
        --timeout)       TIMEOUT="$2"; shift 2 ;;
        --skip-deploy|--start-only) SKIP_DEPLOY=true; shift ;;
        --dry-run)       DRY_RUN=true; shift ;;
        -h|--help)
            head -30 "$0" | tail -27
            exit 0
            ;;
        *)
            if [[ -z "$TOPOLOGY" ]]; then
                TOPOLOGY="$1"
            else
                log_error "Unknown argument: $1"
                exit 1
            fi
            shift
            ;;
    esac
done

if [[ -z "$TOPOLOGY" ]]; then
    log_error "Usage: $0 <topology_yaml> [options]"
    exit 1
fi

if [[ ! -f "$TOPOLOGY" ]]; then
    log_error "Topology file not found: $TOPOLOGY"
    exit 1
fi

# Extract lab name from topology YAML
LAB_NAME=$(grep '^name:' "$TOPOLOGY" | awk '{print $2}' | tr -d '"' | tr -d "'")
if [[ -z "$LAB_NAME" ]]; then
    log_error "Could not extract lab name from $TOPOLOGY"
    exit 1
fi

log_header "Staged Deployment: $LAB_NAME"
echo "  Topology:    $TOPOLOGY"
echo "  Batch size:  $BATCH_SIZE"
echo "  Batch delay: ${BATCH_DELAY}s"
echo "  Tier delay:  ${TIER_DELAY}s"
echo ""

# Helper: get containers matching a pattern within this lab
get_lab_containers() {
    local pattern="${1:-}"
    if [[ -n "$pattern" ]]; then
        docker ps --format '{{.Names}}' | grep "^clab-${LAB_NAME}-" | grep "$pattern" || true
    else
        docker ps --format '{{.Names}}' | grep "^clab-${LAB_NAME}-" || true
    fi
}

# Helper: count containers
count_containers() {
    local pattern="${1:-}"
    get_lab_containers "$pattern" | wc -l
}

# Helper: send start signal to a set of containers
start_tier() {
    local tier_name="$1"
    shift
    local containers=("$@")
    local count=${#containers[@]}

    if [[ $count -eq 0 ]]; then
        log_warn "No containers found for tier: $tier_name"
        return
    fi

    log_info "Starting tier: $tier_name ($count nodes)"

    if [[ "$DRY_RUN" == "true" ]]; then
        for c in "${containers[@]}"; do
            echo "  [dry-run] docker exec $c touch /data/start"
        done
        return
    fi

    local started=0
    for c in "${containers[@]}"; do
        docker exec "$c" touch /data/start 2>/dev/null || true
        started=$((started + 1))
    done

    log_success "Started $started/$count $tier_name nodes"
}

# Helper: send start signal in batches
start_tier_batched() {
    local tier_name="$1"
    shift
    local containers=("$@")
    local count=${#containers[@]}

    if [[ $count -eq 0 ]]; then
        log_warn "No containers found for tier: $tier_name"
        return
    fi

    local num_batches=$(( (count + BATCH_SIZE - 1) / BATCH_SIZE ))
    log_info "Starting tier: $tier_name ($count nodes in $num_batches batches of $BATCH_SIZE)"

    local batch_num=0
    local idx=0
    while [[ $idx -lt $count ]]; do
        batch_num=$((batch_num + 1))
        local batch_end=$((idx + BATCH_SIZE))
        if [[ $batch_end -gt $count ]]; then
            batch_end=$count
        fi
        local batch_size=$((batch_end - idx))

        if [[ "$DRY_RUN" == "true" ]]; then
            echo "  [dry-run] Batch $batch_num: $batch_size nodes"
            idx=$batch_end
            continue
        fi

        local started=0
        for (( i=idx; i<batch_end; i++ )); do
            docker exec "${containers[$i]}" touch /data/start 2>/dev/null || true
            started=$((started + 1))
        done

        log_info "  Batch $batch_num/$num_batches: started $started nodes"
        idx=$batch_end

        if [[ $idx -lt $count ]]; then
            sleep "$BATCH_DELAY"
        fi
    done

    log_success "Started $count $tier_name nodes"
}

########################################
# Step 1: Deploy topology
########################################
if [[ "$SKIP_DEPLOY" == "false" ]]; then
    log_header "Step 1: Deploying topology"

    if [[ "$DRY_RUN" == "true" ]]; then
        echo "  [dry-run] containerlab deploy -t $TOPOLOGY --reconfigure --timeout ${TIMEOUT}m"
    else
        # Source .env for PEAT credentials
        ENV_FILE="$(dirname "$0")/../.env"
        if [[ ! -f "$ENV_FILE" ]]; then
            ENV_FILE="$(dirname "$0")/../../.env"
        fi
        if [[ -f "$ENV_FILE" ]]; then
            log_info "Loading credentials from $ENV_FILE"
            set -a && . "$ENV_FILE" && set +a
        else
            log_warn "No .env file found — PEAT_APP_ID etc must be in environment"
        fi

        BACKEND=${BACKEND:-automerge}
        export BACKEND

        log_info "Deploying with containerlab (timeout: ${TIMEOUT}m)..."
        TS_START=$(date +%s)
        containerlab deploy -t "$TOPOLOGY" --reconfigure --timeout "${TIMEOUT}m"
        TS_END=$(date +%s)
        DEPLOY_SECS=$((TS_END - TS_START))

        TOTAL=$(count_containers)
        log_success "Deployed $TOTAL containers in ${DEPLOY_SECS}s"
    fi
else
    log_info "Skipping deploy (--skip-deploy)"
fi

########################################
# Step 2: Wait for all containers Running
########################################
log_header "Step 2: Waiting for all containers to be Running"

if [[ "$DRY_RUN" == "false" ]]; then
    EXPECTED=$(docker ps -a --format '{{.Names}}' | grep "^clab-${LAB_NAME}-" | wc -l)
    WAIT_START=$(date +%s)
    MAX_WAIT=300  # 5 minutes

    while true; do
        RUNNING=$(docker ps --filter "status=running" --format '{{.Names}}' | grep "^clab-${LAB_NAME}-" | wc -l)
        ELAPSED=$(( $(date +%s) - WAIT_START ))

        if [[ $RUNNING -ge $EXPECTED ]]; then
            log_success "All $RUNNING/$EXPECTED containers running (${ELAPSED}s)"
            break
        fi

        if [[ $ELAPSED -ge $MAX_WAIT ]]; then
            log_error "Timeout: $RUNNING/$EXPECTED running after ${MAX_WAIT}s"
            # Show non-running containers
            log_warn "Non-running containers:"
            docker ps -a --filter "name=clab-${LAB_NAME}-" --format '{{.Names}} {{.Status}}' | grep -v "Up " | head -20
            exit 1
        fi

        echo -ne "  Waiting... $RUNNING/$EXPECTED running (${ELAPSED}s)\r"
        sleep 2
    done
fi

########################################
# Step 3: Tiered start signals
########################################
log_header "Step 3: Sending tiered start signals"

# Tier 1: Battalion HQ
readarray -t HQ_CONTAINERS < <(get_lab_containers "battalion-hq")
start_tier "Battalion HQ" "${HQ_CONTAINERS[@]}"
[[ "$DRY_RUN" == "false" ]] && sleep "$TIER_DELAY"

# Tier 2: Company Commanders
readarray -t CO_CONTAINERS < <(get_lab_containers "commander")
start_tier "Company Commanders" "${CO_CONTAINERS[@]}"
[[ "$DRY_RUN" == "false" ]] && sleep "$TIER_DELAY"

# Tier 3: Platoon Leaders
readarray -t PL_CONTAINERS < <(get_lab_containers "platoon-.*-leader$")
start_tier "Platoon Leaders" "${PL_CONTAINERS[@]}"
[[ "$DRY_RUN" == "false" ]] && sleep "$TIER_DELAY"

# Tier 4: Squad Leaders
readarray -t SL_CONTAINERS < <(get_lab_containers "squad-.*-leader$")
start_tier "Squad Leaders" "${SL_CONTAINERS[@]}"
[[ "$DRY_RUN" == "false" ]] && sleep "$TIER_DELAY"

# Tier 5: Soldiers (batched)
readarray -t SOLDIER_CONTAINERS < <(get_lab_containers "soldier-[0-9]")
start_tier_batched "Soldiers" "${SOLDIER_CONTAINERS[@]}"

########################################
# Summary
########################################
log_header "Deployment Complete"

if [[ "$DRY_RUN" == "false" ]]; then
    TOTAL=$(count_containers)
    echo "  Lab name:           $LAB_NAME"
    echo "  Total containers:   $TOTAL"
    echo "  Battalion HQ:       ${#HQ_CONTAINERS[@]}"
    echo "  Company Commanders: ${#CO_CONTAINERS[@]}"
    echo "  Platoon Leaders:    ${#PL_CONTAINERS[@]}"
    echo "  Squad Leaders:      ${#SL_CONTAINERS[@]}"
    echo "  Soldiers:           ${#SOLDIER_CONTAINERS[@]}"
    echo ""
    echo "  Monitor with:  make lab4-status"
    echo "  Metrics with:  make lab4-metrics"
    echo "  Destroy with:  containerlab destroy -t $TOPOLOGY --cleanup"
fi
