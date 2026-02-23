#!/usr/bin/env bash
# ============================================================================
# HIVE Functional Test Suite Orchestrator
# ============================================================================
# Runs all hardware/infrastructure functional tests from a single entry point:
#
#   1. rpi-rpi BLE      — Pi-to-Pi BLE discovery + GATT sync (hive-btle)
#   2. rpi-android       — Dual-transport BLE + QUIC Pi-to-Android (hive)
#   3. k8s cluster       — K8s multi-pod mesh via k3d (hive-mesh)
#
# Usage:
#   ./scripts/functional-suite.sh              # Run all tests
#   ./scripts/functional-suite.sh --ble-only   # Run only rpi-rpi BLE
#   ./scripts/functional-suite.sh --android-only  # Run only rpi-android
#   ./scripts/functional-suite.sh --k8s-only   # Run only k8s cluster
#   ./scripts/functional-suite.sh --skip-build # Skip build steps (reuse existing binaries)
#
# Environment:
#   HIVE_BTLE_DIR     — Path to hive-btle repo  (default: ../hive-btle)
#   HIVE_MESH_DIR     — Path to hive-mesh repo  (default: ../hive-mesh)
#   RESPONDER_HOST    — BLE responder Pi         (default: kit@rpi-ci)
#   CLIENT_HOST       — BLE client Pi            (default: kit@rpi-ci2)
#   BLE_TEST_PI       — Dual-transport Pi host   (default: rpi-ci)
#   BLE_TEST_PI_IP    — Dual-transport Pi IP     (default: 192.168.228.13)
#   K3D_CLUSTER       — k3d cluster name         (default: hive-test)
# ============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HIVE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Sibling repo paths
HIVE_BTLE_DIR="${HIVE_BTLE_DIR:-$(cd "$HIVE_DIR/../hive-btle" 2>/dev/null && pwd || echo "")}"
HIVE_MESH_DIR="${HIVE_MESH_DIR:-$(cd "$HIVE_DIR/../hive-mesh" 2>/dev/null && pwd || echo "")}"

# Pi infrastructure
RESPONDER_HOST="${RESPONDER_HOST:-kit@rpi-ci}"
CLIENT_HOST="${CLIENT_HOST:-kit@rpi-ci2}"
REMOTE_BTLE_REPO="${REMOTE_BTLE_REPO:-/home/kit/hive-btle}"

# Dual-transport Pi
BLE_TEST_PI="${BLE_TEST_PI:-rpi-ci}"
BLE_TEST_PI_USER="${BLE_TEST_PI_USER:-kit}"
BLE_TEST_PI_IP="${BLE_TEST_PI_IP:-192.168.228.13}"
IROH_TEST_PORT="${IROH_TEST_PORT:-42009}"

# K8s
K3D_CLUSTER="${K3D_CLUSTER:-hive-test}"

# Parse flags
RUN_BLE=true
RUN_ANDROID=true
RUN_K8S=true
SKIP_BUILD=false

for arg in "$@"; do
    case "$arg" in
        --ble-only)    RUN_BLE=true; RUN_ANDROID=false; RUN_K8S=false ;;
        --android-only) RUN_BLE=false; RUN_ANDROID=true; RUN_K8S=false ;;
        --k8s-only)    RUN_BLE=false; RUN_ANDROID=false; RUN_K8S=true ;;
        --skip-build)  SKIP_BUILD=true ;;
        --help|-h)
            head -20 "$0" | grep '^#' | sed 's/^# \?//'
            exit 0
            ;;
        *)
            echo "Unknown option: $arg"
            exit 1
            ;;
    esac
done

# Status tracking
BLE_STATUS="skip"
ANDROID_STATUS="skip"
K8S_STATUS="skip"
FAILURES=0
START_TIME=$(date +%s)

# ── Helpers ────────────────────────────────────────────────────────────────

log_header() {
    echo ""
    echo "╔════════════════════════════════════════════════════════════╗"
    echo "║  $1"
    echo "╚════════════════════════════════════════════════════════════╝"
    echo ""
}

log_result() {
    local name="$1" status="$2"
    if [ "$status" = "pass" ]; then
        echo "  [PASS] $name"
    elif [ "$status" = "FAIL" ]; then
        echo "  [FAIL] $name"
    else
        echo "  [SKIP] $name"
    fi
}

ssh_ok() {
    ssh -o ConnectTimeout=10 -o BatchMode=yes "$1" "true" 2>/dev/null
}

# ── Test 1: rpi-rpi BLE (hive-btle) ───────────────────────────────────────

run_ble_test() {
    log_header "Test 1/3: rpi-rpi BLE (hive-btle)"

    # Check hive-btle repo exists
    if [ -z "$HIVE_BTLE_DIR" ] || [ ! -d "$HIVE_BTLE_DIR/src" ]; then
        echo "Skipped: hive-btle repo not found at $HIVE_BTLE_DIR"
        echo "  Set HIVE_BTLE_DIR to the hive-btle checkout"
        return
    fi

    # Check Pis reachable
    if ! ssh_ok "$RESPONDER_HOST"; then
        echo "Skipped: responder Pi unreachable ($RESPONDER_HOST)"
        return
    fi
    if ! ssh_ok "$CLIENT_HOST"; then
        echo "Skipped: client Pi unreachable ($CLIENT_HOST)"
        return
    fi

    # Check BT adapters
    RESP_BT=$(ssh -o BatchMode=yes "$RESPONDER_HOST" "hciconfig 2>/dev/null | grep -c '^hci' || echo 0")
    CLIENT_BT=$(ssh -o BatchMode=yes "$CLIENT_HOST" "hciconfig 2>/dev/null | grep -c '^hci' || echo 0")
    if [ "$RESP_BT" -lt 1 ] || [ "$CLIENT_BT" -lt 1 ]; then
        echo "Skipped: need BT adapters (responder=$RESP_BT, client=$CLIENT_BT)"
        return
    fi
    echo "Responder ($RESPONDER_HOST): $RESP_BT BT adapter(s)"
    echo "Client ($CLIENT_HOST): $CLIENT_BT BT adapter(s)"

    if [ "$SKIP_BUILD" = false ]; then
        # Sync source to both Pis
        echo "Syncing hive-btle source to Pis..."
        rsync -az --delete \
            --exclude 'target/' --exclude 'android/build/' \
            --exclude 'android/.gradle/' --exclude '.git/' \
            "$HIVE_BTLE_DIR/" "$RESPONDER_HOST:$REMOTE_BTLE_REPO/" &
        local RSYNC1=$!
        rsync -az --delete \
            --exclude 'target/' --exclude 'android/build/' \
            --exclude 'android/.gradle/' --exclude '.git/' \
            "$HIVE_BTLE_DIR/" "$CLIENT_HOST:$REMOTE_BTLE_REPO/" &
        local RSYNC2=$!
        wait $RSYNC1 $RSYNC2
        echo "Source synced."

        # Build on both Pis in parallel
        echo "Building on both Pis (this may take a minute)..."
        ssh -o BatchMode=yes "$RESPONDER_HOST" \
            "cd $REMOTE_BTLE_REPO && source ~/.cargo/env && cargo build --release --features linux --example ble_responder" &
        local BUILD1=$!
        ssh -o BatchMode=yes "$CLIENT_HOST" \
            "cd $REMOTE_BTLE_REPO && source ~/.cargo/env && cargo build --release --features linux --example ble_test_client" &
        local BUILD2=$!
        wait $BUILD1 $BUILD2
        echo "Builds complete."
    fi

    # Clear BlueZ device caches
    echo "Clearing BlueZ device caches..."
    ssh -o BatchMode=yes "$CLIENT_HOST" \
        "bluetoothctl devices | awk '{print \$2}' | xargs -I{} bluetoothctl remove {} 2>/dev/null || true"
    ssh -o BatchMode=yes "$RESPONDER_HOST" \
        "bluetoothctl devices | awk '{print \$2}' | xargs -I{} bluetoothctl remove {} 2>/dev/null || true"

    # Start responder
    echo "Starting responder on $RESPONDER_HOST..."
    ssh -o BatchMode=yes "$RESPONDER_HOST" "pkill -x ble_responder 2>/dev/null || true"
    sleep 1
    ssh -o BatchMode=yes "$RESPONDER_HOST" \
        "cd $REMOTE_BTLE_REPO && (nohup ./target/release/examples/ble_responder --mesh-id CITEST --callsign PI-RESP </dev/null >/tmp/responder.log 2>&1 & echo \$! >/tmp/responder.pid) && cat /tmp/responder.pid" > /tmp/responder_pid.txt
    local RESPONDER_PID
    RESPONDER_PID=$(cat /tmp/responder_pid.txt)
    echo "Responder PID: $RESPONDER_PID"
    sleep 3

    # Run client
    echo "Running client on $CLIENT_HOST..."
    local RESULT=0
    ssh -o BatchMode=yes "$CLIENT_HOST" \
        "cd $REMOTE_BTLE_REPO && timeout 90 ./target/release/examples/ble_test_client --adapter hci0 --mesh-id CITEST --timeout 60" || RESULT=$?

    # Stop responder
    ssh -o BatchMode=yes "$RESPONDER_HOST" "kill $RESPONDER_PID 2>/dev/null || true"

    if [ $RESULT -eq 0 ]; then
        BLE_STATUS="pass"
    else
        echo "Client exited with code: $RESULT"
        echo "--- Responder log ---"
        ssh -o BatchMode=yes "$RESPONDER_HOST" "tail -30 /tmp/responder.log" || true
        BLE_STATUS="FAIL"
        FAILURES=$((FAILURES + 1))
    fi
}

# ── Test 2: rpi-android Dual-Transport (hive) ─────────────────────────────

run_android_test() {
    log_header "Test 2/3: rpi-android Dual-Transport (hive)"

    # Check Android device connected
    if ! command -v adb >/dev/null 2>&1; then
        echo "Skipped: adb not found"
        return
    fi
    if ! adb devices 2>/dev/null | grep -q 'device$'; then
        echo "Skipped: no Android device connected"
        return
    fi
    local ANDROID_DEVICE
    ANDROID_DEVICE=$(adb devices | grep 'device$' | head -1 | awk '{print $1}')
    echo "Android device: $ANDROID_DEVICE"

    # Check Pi reachable
    if ! ssh_ok "$BLE_TEST_PI_USER@$BLE_TEST_PI"; then
        echo "Skipped: Pi unreachable ($BLE_TEST_PI)"
        return
    fi
    echo "Pi: $BLE_TEST_PI ($BLE_TEST_PI_IP)"

    # Use the existing Makefile targets — they handle cross-compile, deploy, etc.
    # This avoids duplicating the complex dual-transport pipeline.
    echo "Running dual-transport test via Makefile..."
    local RESULT=0
    make -C "$HIVE_DIR" dual-transport-test \
        BLE_TEST_PI="$BLE_TEST_PI" \
        BLE_TEST_PI_USER="$BLE_TEST_PI_USER" \
        BLE_TEST_PI_IP="$BLE_TEST_PI_IP" \
        IROH_TEST_PORT="$IROH_TEST_PORT" || RESULT=$?

    if [ $RESULT -eq 0 ]; then
        ANDROID_STATUS="pass"
    else
        ANDROID_STATUS="FAIL"
        FAILURES=$((FAILURES + 1))
    fi
}

# ── Test 3: k8s Cluster (hive-mesh) ───────────────────────────────────────

run_k8s_test() {
    log_header "Test 3/3: k8s Cluster (hive-mesh)"

    # Check hive-mesh repo
    if [ -z "$HIVE_MESH_DIR" ] || [ ! -d "$HIVE_MESH_DIR/src" ]; then
        echo "Skipped: hive-mesh repo not found at $HIVE_MESH_DIR"
        echo "  Set HIVE_MESH_DIR to the hive-mesh checkout"
        return
    fi

    # Check prerequisites
    if ! command -v docker >/dev/null 2>&1; then
        echo "Skipped: docker not found"
        return
    fi
    if ! command -v k3d >/dev/null 2>&1; then
        echo "Skipped: k3d not found"
        return
    fi
    if ! command -v kubectl >/dev/null 2>&1; then
        echo "Skipped: kubectl not found"
        return
    fi
    if ! command -v helm >/dev/null 2>&1; then
        echo "Skipped: helm not found"
        return
    fi

    local CREATED_CLUSTER=false

    if [ "$SKIP_BUILD" = false ]; then
        # Build Docker image
        echo "Building hive-mesh-node Docker image..."
        docker build -t hive-mesh-node:latest -f "$HIVE_MESH_DIR/deploy/Dockerfile" "$HIVE_MESH_DIR"
    fi

    # Create k3d cluster if it doesn't exist
    if ! k3d cluster list 2>/dev/null | grep -q "$K3D_CLUSTER"; then
        echo "Creating k3d cluster '$K3D_CLUSTER'..."
        k3d cluster create "$K3D_CLUSTER"
        CREATED_CLUSTER=true
    else
        echo "Using existing k3d cluster '$K3D_CLUSTER'"
    fi

    # Import image into k3d
    echo "Importing image into k3d..."
    k3d image import hive-mesh-node:latest -c "$K3D_CLUSTER"

    # Deploy with Helm
    local FORMATION_SECRET
    FORMATION_SECRET=$(openssl rand -base64 32)

    if helm status hive-mesh --kube-context "k3d-$K3D_CLUSTER" >/dev/null 2>&1; then
        echo "Upgrading existing Helm release..."
        helm upgrade hive-mesh "$HIVE_MESH_DIR/deploy/helm/hive-mesh" \
            --kube-context "k3d-$K3D_CLUSTER" \
            --set "formationSecret=$FORMATION_SECRET" \
            --set replicaCount=3 \
            --wait --timeout 120s
    else
        echo "Installing Helm release..."
        helm install hive-mesh "$HIVE_MESH_DIR/deploy/helm/hive-mesh" \
            --kube-context "k3d-$K3D_CLUSTER" \
            --set "formationSecret=$FORMATION_SECRET" \
            --set replicaCount=3 \
            --wait --timeout 120s
    fi

    # Wait for all pods Ready
    echo "Waiting for pods to be ready..."
    local READY=false
    for i in $(seq 1 60); do
        local RUNNING
        RUNNING=$(kubectl get pods --context "k3d-$K3D_CLUSTER" -l app.kubernetes.io/name=hive-mesh \
            --no-headers 2>/dev/null | grep -c "Running" || echo 0)
        if [ "$RUNNING" -ge 3 ]; then
            READY=true
            break
        fi
        sleep 2
    done

    if [ "$READY" = false ]; then
        echo "Pods did not reach Ready state within 120s"
        kubectl get pods --context "k3d-$K3D_CLUSTER" -l app.kubernetes.io/name=hive-mesh 2>/dev/null || true
        K8S_STATUS="FAIL"
        FAILURES=$((FAILURES + 1))
        k8s_cleanup "$CREATED_CLUSTER"
        return
    fi

    echo "All 3 pods running."

    # Health checks via broker API
    echo "Checking health endpoints..."
    local ALL_HEALTHY=true
    for i in 0 1 2; do
        local POD="hive-mesh-$i"
        local HEALTH
        HEALTH=$(kubectl exec --context "k3d-$K3D_CLUSTER" "$POD" -- \
            curl -sf http://localhost:8081/api/v1/health 2>/dev/null || echo "FAIL")
        if echo "$HEALTH" | grep -qi "healthy\|ok"; then
            echo "  $POD: healthy"
        else
            echo "  $POD: UNHEALTHY ($HEALTH)"
            ALL_HEALTHY=false
        fi
    done

    # Check peer discovery (give nodes time to discover each other)
    echo "Waiting for peer discovery (15s)..."
    sleep 15

    echo "Checking peer connectivity..."
    local DISCOVERY_OK=true
    for i in 0 1 2; do
        local POD="hive-mesh-$i"
        local PEERS
        PEERS=$(kubectl logs --context "k3d-$K3D_CLUSTER" "$POD" 2>/dev/null \
            | sed 's/\x1b\[[0-9;]*m//g' \
            | grep -c "Peer connected to Iroh\|Accepted incoming sync connection\|Peer removed from Iroh" || true)
        PEERS=${PEERS:-0}
        echo "  $POD: $PEERS peer connection events"
        # Each pod should have discovered at least 1 other pod
        if [ "$PEERS" -lt 1 ]; then
            DISCOVERY_OK=false
        fi
    done

    # Check for errors in logs (excluding known benign patterns)
    # pkarr publish failures are expected in k3d — pods can't reach dns.iroh.link
    echo "Checking for errors..."
    local HAS_ERRORS=false
    for i in 0 1 2; do
        local POD="hive-mesh-$i"
        local ERRORS
        ERRORS=$(kubectl logs --context "k3d-$K3D_CLUSTER" "$POD" 2>/dev/null \
            | sed 's/\x1b\[[0-9;]*m//g' \
            | grep -i "error\|panic" \
            | grep -cv "pkarr\|dns.iroh.link\|pkarr_publish" || true)
        ERRORS=${ERRORS:-0}
        if [ "$ERRORS" -gt 0 ]; then
            echo "  $POD: $ERRORS error(s) in logs"
            kubectl logs --context "k3d-$K3D_CLUSTER" "$POD" 2>/dev/null \
                | sed 's/\x1b\[[0-9;]*m//g' \
                | grep -i "error\|panic" \
                | grep -vi "pkarr\|dns.iroh.link\|pkarr_publish" \
                | tail -5
            HAS_ERRORS=true
        fi
    done

    if [ "$ALL_HEALTHY" = true ] && [ "$HAS_ERRORS" = false ]; then
        K8S_STATUS="pass"
    else
        if [ "$ALL_HEALTHY" = false ]; then
            echo "Some pods are unhealthy"
        fi
        if [ "$HAS_ERRORS" = true ]; then
            echo "Errors found in pod logs"
        fi
        K8S_STATUS="FAIL"
        FAILURES=$((FAILURES + 1))
    fi

    k8s_cleanup "$CREATED_CLUSTER"
}

k8s_cleanup() {
    local CREATED_CLUSTER="$1"
    echo "Cleaning up k8s resources..."
    helm uninstall hive-mesh --kube-context "k3d-$K3D_CLUSTER" 2>/dev/null || true
    if [ "$CREATED_CLUSTER" = true ]; then
        echo "Deleting k3d cluster '$K3D_CLUSTER'..."
        k3d cluster delete "$K3D_CLUSTER" 2>/dev/null || true
    fi
}

# ── Main ───────────────────────────────────────────────────────────────────

echo "╔════════════════════════════════════════════════════════════╗"
echo "║  HIVE Functional Test Suite                                ║"
echo "║  $(date '+%Y-%m-%d %H:%M:%S')                                       ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Tests to run:"
$RUN_BLE     && echo "  1. rpi-rpi BLE (hive-btle)" || echo "  1. rpi-rpi BLE (skipped)"
$RUN_ANDROID && echo "  2. rpi-android dual-transport (hive)" || echo "  2. rpi-android (skipped)"
$RUN_K8S     && echo "  3. k8s cluster (hive-mesh)" || echo "  3. k8s cluster (skipped)"
echo ""

$RUN_BLE     && run_ble_test
$RUN_ANDROID && run_android_test
$RUN_K8S     && run_k8s_test

# ── Summary ────────────────────────────────────────────────────────────────

END_TIME=$(date +%s)
ELAPSED=$(( END_TIME - START_TIME ))
MINUTES=$(( ELAPSED / 60 ))
SECONDS=$(( ELAPSED % 60 ))

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Functional Test Suite Results                             ║"
echo "╠════════════════════════════════════════════════════════════╣"
log_result "rpi-rpi BLE"              "$BLE_STATUS"
log_result "rpi-android dual-transport" "$ANDROID_STATUS"
log_result "k8s cluster"              "$K8S_STATUS"
echo "╠════════════════════════════════════════════════════════════╣"
echo "  Time: ${MINUTES}m ${SECONDS}s"
if [ $FAILURES -eq 0 ]; then
    PASSED=0
    [ "$BLE_STATUS" = "pass" ] && PASSED=$((PASSED + 1))
    [ "$ANDROID_STATUS" = "pass" ] && PASSED=$((PASSED + 1))
    [ "$K8S_STATUS" = "pass" ] && PASSED=$((PASSED + 1))
    if [ $PASSED -gt 0 ]; then
        echo "  All executed tests PASSED"
    else
        echo "  No tests were executed (all skipped)"
    fi
else
    echo "  $FAILURES test(s) FAILED"
fi
echo "╚════════════════════════════════════════════════════════════╝"

exit $FAILURES
