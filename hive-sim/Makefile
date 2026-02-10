# Hive-Sim Makefile
# Simple interface for running network simulations and tests

.PHONY: help build clean deploy destroy lab4-24 lab4-48 lab4-96 lab4-384 lab4-start lab4-status lab4-metrics lab4-results lab4-watch lab4-experiment lab4-single lab4-analyze lab4-save-logs lab4-destroy
.PHONY: clab-deploy-phase2 clab-destroy-phase2 clab-status-phase2
.PHONY: lab5-packet-loss lab5a-sweep lab5a-collect-latency lab5a-report lab5-intermittent lab5b-sweep lab5b-report lab5-churn lab5c-sweep lab5c-report lab5-knockout lab5-leader-kill lab5-partition lab5-blackout lab5-all lab5-status lab5-inject lab5-recover lab5-recovery-time lab5-verify-consistency lab5-report

# Default target
.DEFAULT_GOAL := help

##@ General

help: ## Display this help message
	@echo "Hive-Sim - Network Simulation Testing"
	@echo ""
	@awk 'BEGIN {FS = ":.*##"; printf "Usage:\n  make \033[36m<target>\033[0m\n"} /^[a-zA-Z0-9_-]+:.*?##/ { printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

build: ## Build Docker image for nodes
	@echo "🔨 Building hive-sim-node Docker image..."
	docker build -f Dockerfile -t hive-sim-node:latest ..

clean: ## Destroy all deployments and clean up (use sudo for root processes)
	@echo "🧹 Cleaning up all deployments..."
	-docker ps -a --filter "name=clab-" -q | xargs -r docker rm -f 2>/dev/null
	-docker network prune -f 2>/dev/null
	-rm -rf .generated/ clab-*/ /tmp/hive-sim* 2>/dev/null
	@ORPHANS=$$(pgrep -c -f "hive-sim" 2>/dev/null || echo 0); \
	if [ "$$ORPHANS" -gt 0 ]; then \
		echo "⚠️  Found $$ORPHANS orphaned hive-sim processes (root-owned)"; \
		echo "   Run: sudo pkill -9 -f hive-sim"; \
	fi
	@echo "✅ Cleanup complete"

##@ Traditional Baseline Testing (Single-Battalion)

baseline-24: ## Run 24-node traditional baseline test
	@echo "🚀 Running 24-node traditional baseline..."
	./test-scaling-validation.sh 24

baseline-48: ## Run 48-node traditional baseline test
	@echo "🚀 Running 48-node traditional baseline..."
	./test-scaling-validation.sh 48

baseline-96: ## Run 96-node traditional baseline test
	@echo "🚀 Running 96-node traditional baseline..."
	./test-scaling-validation.sh 96

baseline-192: ## Run 192-node traditional baseline test
	@echo "🚀 Running 192-node traditional baseline..."
	./test-scaling-validation.sh 192

baseline-384: ## Run 384-node traditional baseline test
	@echo "🚀 Running 384-node traditional baseline..."
	./test-scaling-validation.sh 384

baseline-500: ## Run 500-node traditional baseline test
	@echo "🚀 Running 500-node traditional baseline..."
	./test-scaling-validation.sh 500

##@ Traditional Baseline Suite

baseline-suite-small: baseline-24 baseline-48 ## Run small baseline suite (24, 48 nodes)
	@echo "✅ Small baseline suite complete"

baseline-suite-standard: baseline-24 baseline-48 baseline-96 ## Run standard baseline suite (24, 48, 96 nodes)
	@echo "✅ Standard baseline suite complete"

baseline-suite-full: baseline-24 baseline-48 baseline-96 baseline-192 baseline-384 ## Run full baseline suite (24-384 nodes)
	@echo "✅ Full baseline suite complete"

##@ Multi-Battalion Testing (Large Scale)

multi-1500: ## Deploy 1500-node multi-battalion topology
	@echo "🚀 Deploying 1500-node multi-battalion topology..."
	./deploy-multibridge-1500node.sh

multi-2000: ## Deploy 2000-node multi-battalion topology
	@echo "🚀 Deploying 2000-node multi-battalion topology..."
	./deploy-multibridge-2000node.sh

##@ Hierarchical Gateway Testing (Edge Deployments)

hierarchical-generate: ## Generate hierarchical topology (3 squads × 12 nodes)
	@echo "📝 Generating hierarchical topology..."
	python3 generate-hierarchical-topology.py --name platoon-3squad --squads 3 --nodes-per-squad 12

hierarchical-deploy: hierarchical-generate ## Deploy hierarchical topology
	@echo "🚀 Deploying hierarchical topology..."
	containerlab deploy -t topologies/platoon-3squad.yaml
	./attach-gateways-platoon-3squad.sh

hierarchical-constrain-tactical: ## Apply tactical radio constraints
	@echo "🔧 Applying tactical radio constraints..."
	./apply-hierarchical-constraints.sh --topology platoon-3squad --profile tactical-radio-vhf --squads 3 --nodes-per-squad 12

hierarchical-constrain-iot: ## Apply IoT LoRa constraints
	@echo "🔧 Applying IoT LoRa constraints..."
	./apply-hierarchical-constraints.sh --topology platoon-3squad --profile iot-lora --squads 3 --nodes-per-squad 12

hierarchical-list-profiles: ## List available bandwidth profiles
	./apply-hierarchical-constraints.sh --list-profiles

##@ Quick Validation

validate: ## Quick validation (24 nodes, 10s test) - use this first!
	@chmod +x simple-validate.sh
	@./simple-validate.sh

##@ Development

rebuild: clean build ## Clean and rebuild Docker image
	@echo "✅ Rebuild complete"

test-deploy-24: build ## Quick deploy test (24 nodes, no metrics)
	@echo "🧪 Quick deployment test (24 nodes)..."
	containerlab deploy -t topologies/traditional-battalion-24node.yaml
	@echo "⏳ Waiting 10s..."
	@sleep 10
	@echo "📊 Container count: $$(docker ps | grep traditional-battalion-24node | wc -l)"
	@echo "🧹 Cleaning up..."
	containerlab destroy -t topologies/traditional-battalion-24node.yaml --cleanup

##@ Utilities

list-deployments: ## List active ContainerLab deployments
	@echo "📋 Active deployments:"
	@containerlab inspect --all 2>/dev/null || echo "No active deployments"

show-networks: ## Show Docker networks
	@echo "🌐 Docker networks:"
	@docker network ls | grep -E "battalion|platoon|squad" || echo "No simulation networks found"

show-results: ## Show recent test results
	@echo "📊 Recent test results:"
	@ls -dt scaling-results-* 2>/dev/null | head -5 | while read dir; do \
		echo ""; \
		echo "=== $$dir ==="; \
		cat "$$dir/test-summary.txt" 2>/dev/null | grep -A 20 "Test Configuration:" | head -15; \
	done

logs-hq: ## Show logs from battalion HQ (most recent deployment)
	@echo "📝 Battalion HQ logs:"
	@CONTAINER=$$(docker ps --format '{{.Names}}' | grep 'battalion.*hq' | head -1); \
	if [ -n "$$CONTAINER" ]; then \
		docker logs $$CONTAINER 2>&1 | tail -50; \
	else \
		echo "No HQ container found"; \
	fi

##@ Documentation

docs: ## Show key documentation files
	@echo "📚 Documentation:"
	@echo "  - HIERARCHICAL-LAB-TESTING-GUIDE.md - Lab testing with hierarchical topologies"
	@echo "  - MULTI-BRIDGE-ARCHITECTURE.md - Multi-bridge scaling architecture"
	@echo "  - VALIDATION-TEST-PLAN.md - Validation test plan"
	@echo "  - bandwidth-profiles.yaml - Bandwidth constraint profiles"

##@ Examples

.PHONY: example-baseline example-hierarchical example-tactical

example-baseline: ## Example: Run baseline test on 48 nodes
	@echo "Example: Traditional Baseline Test"
	@echo "=================================="
	@echo ""
	@echo "This will:"
	@echo "  1. Build Docker image"
	@echo "  2. Deploy 48-node traditional baseline topology"
	@echo "  3. Run 60-second test with metrics collection"
	@echo "  4. Clean up deployment"
	@echo ""
	@echo "Run: make baseline-48"

example-hierarchical: ## Example: Run hierarchical gateway test
	@echo "Example: Hierarchical Gateway Test"
	@echo "==================================="
	@echo ""
	@echo "This will:"
	@echo "  1. Build Docker image"
	@echo "  2. Generate 3-squad hierarchical topology (40 nodes)"
	@echo "  3. Deploy topology with squad networks + backhaul"
	@echo "  4. Attach gateways to backhaul network"
	@echo ""
	@echo "Run: make hierarchical-deploy"
	@echo "Then apply constraints: make hierarchical-constrain-tactical"

example-tactical: ## Example: Full tactical radio scenario
	@echo "Example: Tactical Radio Network Scenario"
	@echo "========================================="
	@echo ""
	@echo "Simulates 3 squads with VHF radios + satellite backhaul:"
	@echo ""
	@echo "  make hierarchical-deploy              # Deploy 40 nodes (3 squads)"
	@echo "  make hierarchical-constrain-tactical  # Apply VHF radio constraints"
	@echo ""
	@echo "Constraints applied:"
	@echo "  - Intra-squad: 256 kbps, 50ms latency (VHF radio)"
	@echo "  - Backhaul: 10 Mbps, 500ms latency (satellite)"
	@echo ""
	@echo "View profiles: make hierarchical-list-profiles"

##@ Lab 4: Hierarchical Aggregation Testing
# BANDWIDTH options: 1gbps (default), 100mbps, 1mbps, 256kbps

lab4-24: ## Deploy Lab 4 with 24 nodes (BACKEND=automerge, BANDWIDTH=1gbps by default)
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 4: 24 nodes (BACKEND=$${BACKEND:-automerge}, BW=$${BANDWIDTH:-1gbps}) ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@if [ ! -f ../.env ]; then \
		echo "ERROR: ../.env file required with HIVE_APP_ID, HIVE_SHARED_KEY"; \
		exit 1; \
	fi
	@TOPO="topologies/lab4-24n-$${BANDWIDTH:-1gbps}.yaml"; \
	if [ ! -f "$$TOPO" ]; then \
		echo "Generating topology: $$TOPO"; \
		python3 generate-lab4-hierarchical-topology.py --nodes 24 --bandwidth $${BANDWIDTH:-1gbps} --output "$$TOPO" --name lab4-24n; \
	fi; \
	set -a && . ../.env && set +a && \
		BACKEND=$${BACKEND:-automerge} \
		CAP_IN_MEMORY=$${CAP_IN_MEMORY:-true} \
		containerlab deploy -t "$$TOPO" --reconfigure --timeout 5m
	@echo ""
	@echo "✓ Lab 4 (24 nodes) deployed. Monitor with: make lab4-status"

lab4-48: ## Deploy Lab 4 with 48 nodes (BACKEND=automerge, BANDWIDTH=1gbps by default)
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 4: 48 nodes (BACKEND=$${BACKEND:-automerge}, BW=$${BANDWIDTH:-1gbps}) ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@if [ ! -f ../.env ]; then \
		echo "ERROR: ../.env file required with HIVE_APP_ID, HIVE_SECRET_KEY"; \
		exit 1; \
	fi
	@TOPO="topologies/lab4-48n-$${BANDWIDTH:-1gbps}.yaml"; \
	if [ ! -f "$$TOPO" ]; then \
		echo "Generating topology: $$TOPO"; \
		python3 generate-lab4-hierarchical-topology.py --nodes 48 --bandwidth $${BANDWIDTH:-1gbps} --output "$$TOPO" --name lab4-48n; \
	fi; \
	set -a && . ../.env && set +a && \
		BACKEND=$${BACKEND:-automerge} \
		CAP_IN_MEMORY=$${CAP_IN_MEMORY:-true} \
		containerlab deploy -t "$$TOPO" --reconfigure --timeout 5m
	@echo ""
	@echo "✓ Lab 4 (48 nodes) deployed. Monitor with: make lab4-status"

lab4-96: ## Deploy Lab 4 with 96 nodes (BACKEND=automerge, BANDWIDTH=1gbps by default)
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 4: 96 nodes (BACKEND=$${BACKEND:-automerge}, BW=$${BANDWIDTH:-1gbps}) ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@if [ ! -f ../.env ]; then \
		echo "ERROR: ../.env file required with HIVE_APP_ID, HIVE_SECRET_KEY"; \
		exit 1; \
	fi
	@TOPO="topologies/lab4-96n-$${BANDWIDTH:-1gbps}.yaml"; \
	if [ ! -f "$$TOPO" ]; then \
		echo "Generating topology: $$TOPO"; \
		python3 generate-lab4-hierarchical-topology.py --nodes 96 --bandwidth $${BANDWIDTH:-1gbps} --output "$$TOPO" --name lab4-96n; \
	fi; \
	set -a && . ../.env && set +a && \
		BACKEND=$${BACKEND:-automerge} \
		CAP_IN_MEMORY=$${CAP_IN_MEMORY:-true} \
		containerlab deploy -t "$$TOPO" --reconfigure --timeout 5m
	@echo ""
	@echo "✓ Lab 4 (96 nodes) deployed. Monitor with: make lab4-status"

lab4-192: ## Deploy Lab 4 with 192 nodes (BACKEND=automerge, BANDWIDTH=1gbps by default)
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 4: 192 nodes (BACKEND=$${BACKEND:-automerge}, BW=$${BANDWIDTH:-1gbps})║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@if [ ! -f ../.env ]; then \
		echo "ERROR: ../.env file required with HIVE_APP_ID, HIVE_SECRET_KEY"; \
		exit 1; \
	fi
	@TOPO="topologies/lab4-192n-$${BANDWIDTH:-1gbps}.yaml"; \
	if [ ! -f "$$TOPO" ]; then \
		echo "Generating topology: $$TOPO"; \
		python3 generate-lab4-hierarchical-topology.py --nodes 192 --bandwidth $${BANDWIDTH:-1gbps} --output "$$TOPO" --name lab4-192n; \
	fi; \
	set -a && . ../.env && set +a && \
		BACKEND=$${BACKEND:-automerge} \
		CAP_IN_MEMORY=$${CAP_IN_MEMORY:-true} \
		containerlab deploy -t "$$TOPO" --reconfigure --timeout 8m
	@echo ""
	@echo "✓ Lab 4 (192 nodes) deployed. Monitor with: make lab4-status"

lab4-384: ## Deploy Lab 4 with 384 nodes (BACKEND=automerge, BANDWIDTH=1gbps by default)
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 4: 384 nodes (BACKEND=$${BACKEND:-automerge}, BW=$${BANDWIDTH:-1gbps})║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@if [ ! -f ../.env ]; then \
		echo "ERROR: ../.env file required with HIVE_APP_ID, HIVE_SECRET_KEY"; \
		exit 1; \
	fi
	@TOPO="topologies/lab4-384n-$${BANDWIDTH:-1gbps}.yaml"; \
	if [ ! -f "$$TOPO" ]; then \
		echo "Generating topology: $$TOPO"; \
		python3 generate-lab4-hierarchical-topology.py --nodes 384 --bandwidth $${BANDWIDTH:-1gbps} --output "$$TOPO" --name lab4-384n; \
	fi; \
	set -a && . ../.env && set +a && \
		BACKEND=$${BACKEND:-automerge} \
		CAP_IN_MEMORY=$${CAP_IN_MEMORY:-true} \
		containerlab deploy -t "$$TOPO" --reconfigure --timeout 10m
	@echo ""
	@echo "✓ Lab 4 (384 nodes) deployed. Monitor with: make lab4-status"

lab4-start: ## Trigger simulation start for all Lab 4 containers
	@echo "Triggering simulation start..."
	@for container in $$(docker ps --format '{{.Names}}' | grep clab-lab4); do \
		docker exec "$$container" touch /data/start 2>/dev/null || true; \
	done
	@echo "✓ Start triggered for all Lab 4 containers"

lab4-status: ## Show Lab 4 deployment status
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 4: Status                                             ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@TOTAL=$$(docker ps --filter "name=clab-lab4" -q | wc -l); \
	RUNNING=$$(docker ps --filter "name=clab-lab4" --filter "status=running" -q | wc -l); \
	echo "Containers: $$RUNNING running / $$TOTAL total"
	@echo ""
	@echo "Memory usage (leaders):"
	@docker stats --no-stream --format "table {{.Name}}\t{{.MemUsage}}" | grep -E "squad.*leader|platoon.*leader|commander" | head -10
	@echo ""
	@echo "Errors (if any):"
	@docker ps --filter "name=clab-lab4" -q | head -10 | xargs -I{} docker logs {} 2>&1 | grep -iE "error|panic|fatal" | grep -v "No error" | head -5 || echo "  None found"

lab4-metrics: ## Show latency and throughput metrics
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 4: Latency & Throughput Metrics                       ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@echo "=== Aggregation Processing Time (µs) ==="
	@echo -n "  Squad:   "; docker logs $$(docker ps --format '{{.Names}}' | grep "squad-1-leader" | head -1) 2>&1 | grep "AggregationCompleted.*squad" | tail -1 | grep -oP '"processing_time_us":\K[0-9]+' || echo "N/A"
	@echo -n "  Platoon: "; docker logs $$(docker ps --format '{{.Names}}' | grep "platoon-1-leader" | head -1) 2>&1 | grep "AggregationCompleted.*platoon" | tail -1 | grep -oP '"processing_time_us":\K[0-9]+' || echo "N/A"
	@echo -n "  Company: "; docker logs $$(docker ps --format '{{.Names}}' | grep "commander" | head -1) 2>&1 | grep "AggregationCompleted.*company" | tail -1 | grep -oP '"processing_time_us":\K[0-9]+' || echo "N/A"
	@echo ""
	@echo "=== Reduction Ratio (per tier) ==="
	@echo -n "  Squad (7 NodeStates → 1 Summary):   "; docker logs $$(docker ps --format '{{.Names}}' | grep "squad-1-leader" | head -1) 2>&1 | grep "AggregationEfficiency.*squad" | tail -1 | grep -oP '"reduction_ratio":\K[0-9.]+' || echo "N/A"
	@echo -n "  Platoon (3 Squads → 1 Summary):     "; docker logs $$(docker ps --format '{{.Names}}' | grep "platoon-1-leader" | head -1) 2>&1 | grep "AggregationEfficiency.*platoon" | tail -1 | grep -oP '"reduction_ratio":\K[0-9.]+' || echo "N/A"
	@echo -n "  Company (1 Platoon → 1 Summary):    "; docker logs $$(docker ps --format '{{.Names}}' | grep "commander" | head -1) 2>&1 | grep "AggregationEfficiency.*company" | tail -1 | grep -oP '"reduction_ratio":\K[0-9.]+' || echo "N/A"
	@echo ""
	@echo "=== Aggregation Throughput ==="
	@COMMANDER=$$(docker ps --format '{{.Names}}' | grep "commander" | head -1); \
	COUNT=$$(docker logs $$COMMANDER 2>&1 | grep "AggregationCompleted.*company" | wc -l); \
	echo "  Total company aggregations: $$COUNT"
	@echo ""
	@echo "=== P2P Sync Latency (per tier, excluding warmup >500ms) ==="
	@echo -n "  Soldier → Squad:   "; docker ps --format '{{.Names}}' | grep "squad-1-leader" | head -1 | xargs -I{} docker logs {} 2>&1 | grep "soldier_to_squad_leader" | grep -oP '"latency_ms":\K[0-9.]+' | awk '$$1 <= 500 {sum+=$$1;count++;if(min=="" || $$1<min)min=$$1;if($$1>max)max=$$1} END{if(count>0) printf "Min: %.2f ms | Avg: %.2f ms | Max: %.2f ms (n=%d)\n", min, sum/count, max, count; else print "N/A"}'
	@echo -n "  Squad → Platoon:   "; docker ps --format '{{.Names}}' | grep "platoon-1-leader" | head -1 | xargs -I{} docker logs {} 2>&1 | grep "squad_to_platoon_leader" | grep -oP '"latency_ms":\K[0-9.]+' | awk '$$1 <= 500 {sum+=$$1;count++;if(min=="" || $$1<min)min=$$1;if($$1>max)max=$$1} END{if(count>0) printf "Min: %.2f ms | Avg: %.2f ms | Max: %.2f ms (n=%d)\n", min, sum/count, max, count; else print "N/A"}'
	@echo -n "  Platoon → Company: "; docker ps --format '{{.Names}}' | grep "commander" | head -1 | xargs -I{} docker logs {} 2>&1 | grep "platoon_to_company_commander" | grep -oP '"latency_ms":\K[0-9.]+' | awk '$$1 <= 500 {sum+=$$1;count++;if(min=="" || $$1<min)min=$$1;if($$1>max)max=$$1} END{if(count>0) printf "Min: %.2f ms | Avg: %.2f ms | Max: %.2f ms (n=%d)\n", min, sum/count, max, count; else print "N/A"}'
	@echo ""
	@echo "=== CRDT Upsert Latency ==="
	@docker logs $$(docker ps --format '{{.Names}}' | grep "commander" | head -1) 2>&1 | grep "CRDTUpsert" | grep -oP '"latency_ms":\K[0-9.]+' | awk 'BEGIN{min=999999;max=0} {sum+=$$1;count++;if($$1<min)min=$$1;if($$1>max)max=$$1} END{if(count>0) printf "  Min: %.3f ms | Avg: %.3f ms | Max: %.3f ms (n=%d)\n", min, sum/count, max, count; else print "  N/A"}'

lab4-results: ## Show hierarchical aggregation results
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 4: Hierarchical Aggregation Results                   ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo ""
	@echo "=== Topology ==="
	@NODES=$$(docker ps --filter "name=clab-lab4" -q | wc -l); \
	SQUADS=$$(docker ps --filter "name=clab-lab4" --format '{{.Names}}' | grep -E "squad-[0-9]+-leader" | wc -l); \
	PLATOONS=$$(docker ps --filter "name=clab-lab4" --format '{{.Names}}' | grep -E "platoon-[0-9]+-leader" | wc -l); \
	COMPANIES=$$(docker ps --filter "name=clab-lab4" --format '{{.Names}}' | grep "commander" | wc -l); \
	echo "  Nodes: $$NODES | Squads: $$SQUADS | Platoons: $$PLATOONS | Companies: $$COMPANIES"
	@echo ""
	@echo "=== Aggregation Flow ==="
	@SQUAD_METRICS=$$(docker ps --format '{{.Names}}' | grep -E "squad-[0-9]+-leader" | head -1 | xargs -I{} docker logs {} 2>&1 | grep "AggregationEfficiency.*squad" | tail -1); \
	if [ -n "$$SQUAD_METRICS" ]; then \
		INPUT=$$(echo "$$SQUAD_METRICS" | grep -o '"input_docs":[0-9]*' | cut -d: -f2); \
		echo "  Squad: $$INPUT NodeStates → 1 SquadSummary (7 members each)"; \
	fi
	@PLATOON_METRICS=$$(docker ps --format '{{.Names}}' | grep -E "platoon-[0-9]+-leader" | head -1 | xargs -I{} docker logs {} 2>&1 | grep "AggregationEfficiency.*platoon" | tail -1); \
	if [ -n "$$PLATOON_METRICS" ]; then \
		INPUT=$$(echo "$$PLATOON_METRICS" | grep -o '"input_docs":[0-9]*' | cut -d: -f2); \
		MEMBERS=$$(echo "$$PLATOON_METRICS" | grep -o '"total_members":[0-9]*' | cut -d: -f2); \
		echo "  Platoon: $$INPUT SquadSummaries → 1 PlatoonSummary ($$MEMBERS members)"; \
	fi
	@COMPANY_METRICS=$$(docker ps --format '{{.Names}}' | grep "commander" | head -1 | xargs -I{} docker logs {} 2>&1 | grep "AggregationEfficiency.*company" | tail -1); \
	if [ -n "$$COMPANY_METRICS" ]; then \
		INPUT=$$(echo "$$COMPANY_METRICS" | grep -o '"input_docs":[0-9]*' | cut -d: -f2); \
		MEMBERS=$$(echo "$$COMPANY_METRICS" | grep -o '"total_members":[0-9]*' | cut -d: -f2); \
		echo "  Company: $$INPUT PlatoonSummaries → 1 CompanySummary ($$MEMBERS members)"; \
	fi
	@echo ""
	@echo "=== Sync Status ==="
	@COMMANDER=$$(docker ps --format '{{.Names}}' | grep "commander" | head -1); \
	if [ -n "$$COMMANDER" ]; then \
		COMPLETE=$$(docker logs $$COMMANDER 2>&1 | grep "AggregationCompleted.*company" | wc -l); \
		echo "  Company aggregations completed: $$COMPLETE"; \
	fi
	@echo ""
	@echo "=== Memory Usage (Leaders) ==="
	@docker stats --no-stream --format "  {{.Name}}: {{.MemUsage}}" | grep -E "squad.*leader|platoon.*leader|commander" | head -5

lab4-watch: ## Continuously monitor Lab 4 (Ctrl+C to stop)
	@watch -n 5 'docker stats --no-stream --format "table {{.Name}}\t{{.MemUsage}}\t{{.CPUPerc}}" | grep -E "squad.*leader|platoon.*leader|commander" | head -20'

lab4-experiment: ## Run full Lab 4 matrix (all backends × nodes × bandwidths) with analysis
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 4 Full Experiment Matrix                              ║"
	@echo "║  Backends: ditto, automerge                                ║"
	@echo "║  Nodes: 24, 48, 96, 384                                    ║"
	@echo "║  Bandwidths: 1gbps, 100mbps, 1mbps, 256kbps                ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@./run-lab4-comparison.sh --duration $${DURATION:-120}

lab4-single: ## Run single Lab 4 test (NODES=24, BACKEND=automerge, BANDWIDTH=1gbps)
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 4 Single Test: $${NODES:-24}n, $${BACKEND:-automerge}, $${BANDWIDTH:-1gbps}  ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@./run-lab4-experiment.sh \
		--nodes $${NODES:-24} \
		--backend $${BACKEND:-automerge} \
		--bandwidth $${BANDWIDTH:-1gbps} \
		--duration $${DURATION:-120}

lab4-analyze: ## Analyze Lab 4 results (RESULTS_DIR=path or uses --latest)
	@if [ -n "$${RESULTS_DIR:-}" ]; then \
		./process-lab4-metrics.sh "$$RESULTS_DIR"; \
	else \
		./process-lab4-metrics.sh --latest; \
	fi

lab4-save-logs: ## Save current Lab 4 container logs to results directory
	@BACKEND=$${BACKEND:-automerge}; \
	BANDWIDTH=$${BANDWIDTH:-1gbps}; \
	NODES=$$(docker ps --filter "name=clab-lab4" -q | wc -l); \
	TIMESTAMP=$$(date +%Y%m%d-%H%M%S); \
	RESULTS_DIR="/work/hive-sim-results/lab4-$${BACKEND}-$${NODES}n-$${BANDWIDTH}-$${TIMESTAMP}"; \
	mkdir -p "$$RESULTS_DIR/logs"; \
	echo "Saving logs to: $$RESULTS_DIR/logs"; \
	for container in $$(docker ps --format '{{.Names}}' | grep clab-lab4); do \
		docker logs "$$container" > "$$RESULTS_DIR/logs/$${container}.log" 2>&1; \
	done; \
	echo "✓ Saved $$(ls $$RESULTS_DIR/logs/*.log 2>/dev/null | wc -l) log files"; \
	echo "Results: $$RESULTS_DIR"

lab4-destroy: ## Destroy Lab 4 deployment
	@echo "Destroying Lab 4 deployment..."
	-containerlab destroy -t topologies/lab4-24n-$${BANDWIDTH:-1gbps}.yaml --cleanup 2>/dev/null || true
	-containerlab destroy -t topologies/lab4-48n-$${BANDWIDTH:-1gbps}.yaml --cleanup 2>/dev/null || true
	-containerlab destroy -t topologies/lab4-96n-$${BANDWIDTH:-1gbps}.yaml --cleanup 2>/dev/null || true
	-containerlab destroy -t topologies/lab4-192n-$${BANDWIDTH:-1gbps}.yaml --cleanup 2>/dev/null || true
	-containerlab destroy -t topologies/lab4-384n-$${BANDWIDTH:-1gbps}.yaml --cleanup 2>/dev/null || true
	@# Also clean up any orphaned containers
	-docker ps -a --filter "name=clab-lab4" -q | xargs -r docker rm -f 2>/dev/null || true
	-docker network prune -f 2>/dev/null || true
	@echo "✓ Lab 4 destroyed"

##@ Phase 2: Port Berth Operation (58-node multi-hold topology)

.PHONY: clab-deploy-phase2 clab-destroy-phase2 clab-status-phase2

clab-deploy-phase2: ## Deploy Phase 2 port berth operation (58 nodes, 3 holds)
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Phase 2: Port Berth Operation (58 nodes, 3 holds)        ║"
	@echo "║  BACKEND=$${BACKEND:-automerge}                            ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@if [ ! -f ../.env ]; then \
		echo "ERROR: ../.env file required with HIVE_APP_ID, HIVE_SHARED_KEY"; \
		exit 1; \
	fi
	@set -a && . ../.env && set +a && \
		BACKEND=$${BACKEND:-automerge} \
		CAP_IN_MEMORY=$${CAP_IN_MEMORY:-true} \
		containerlab deploy -t topologies/port-berth-operation-58node.yaml --reconfigure --timeout 8m
	@echo ""
	@echo "✓ Phase 2 (58 nodes) deployed. Monitor with: make clab-status-phase2"

clab-destroy-phase2: ## Destroy Phase 2 port berth operation deployment
	@echo "Destroying Phase 2 deployment..."
	-containerlab destroy -t topologies/port-berth-operation-58node.yaml --cleanup 2>/dev/null || true
	-docker ps -a --filter "name=clab-port-berth-58n" -q | xargs -r docker rm -f 2>/dev/null || true
	-docker network prune -f 2>/dev/null || true
	@echo "✓ Phase 2 destroyed"

clab-status-phase2: ## Show Phase 2 deployment status
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Phase 2: Status                                           ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@TOTAL=$$(docker ps --filter "name=clab-port-berth-58n" -q | wc -l); \
	RUNNING=$$(docker ps --filter "name=clab-port-berth-58n" --filter "status=running" -q | wc -l); \
	echo "Containers: $$RUNNING running / $$TOTAL total"
	@echo ""
	@echo "Hold aggregators:"
	@docker stats --no-stream --format "  {{.Name}}: {{.MemUsage}}" 2>/dev/null | grep -E "agg-h|berth-mgr" | head -5 || echo "  (not running)"
	@echo ""
	@echo "Errors (if any):"
	@docker ps --filter "name=clab-port-berth-58n" -q | head -10 | xargs -I{} docker logs {} 2>&1 | grep -iE "error|panic|fatal" | grep -v "No error" | head -5 || echo "  None found"

##@ Lab 5: Chaos Engineering and Mesh Degradation Testing
# Requires: Lab 4 deployment running first (make lab4-96 or lab4-384)
# See: gh issue view 471 for full epic details

lab5-packet-loss: ## Lab 5a: Inject packet loss (LOSS=5, targets 5% loss)
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 5a: Packet Loss Injection ($${LOSS:-5}%)               ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@CONTAINERS=$$(docker ps --filter "name=clab-" --format '{{.Names}}' | wc -l); \
	if [ "$$CONTAINERS" -eq 0 ]; then \
		echo "ERROR: No deployment found. Run 'make lab4-96' first."; \
		exit 1; \
	fi
	@echo "Applying $${LOSS:-5}% packet loss to all containers..."
	@TS=$$(date +%s%3N); \
	echo "{\"type\":\"inject\",\"mode\":\"packet_loss\",\"timestamp_ms\":$$TS,\"details\":{\"loss_pct\":$${LOSS:-5}}}" >> /tmp/lab5-chaos-events.jsonl
	@for container in $$(docker ps --filter "name=clab-" --format '{{.Names}}'); do \
		docker exec $$container tc qdisc replace dev eth0 root netem loss $${LOSS:-5}% 2>/dev/null || \
		docker exec $$container tc qdisc add dev eth0 root netem loss $${LOSS:-5}% 2>/dev/null || true; \
	done
	@echo "✓ Packet loss applied. Monitor with: make lab5-status"

lab5a-sweep: ## Lab 5a: Run full packet loss sweep (1%, 5%, 10%, 20%) with metrics collection
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 5a: Packet Loss Sweep (1%, 5%, 10%, 20%)              ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@CONTAINERS=$$(docker ps --filter "name=clab-" --format '{{.Names}}' | wc -l); \
	if [ "$$CONTAINERS" -eq 0 ]; then \
		echo "ERROR: No deployment found. Run 'make lab4-96' first."; \
		exit 1; \
	fi
	@RESULTS_DIR="/tmp/lab5a-sweep-$$(date +%Y%m%d-%H%M%S)"; \
	mkdir -p "$$RESULTS_DIR"; \
	echo "Results: $$RESULTS_DIR"; \
	echo "timestamp,loss_pct,p50_ms,p95_ms,p99_ms,sample_count" > "$$RESULTS_DIR/latency.csv"; \
	echo ""; \
	echo "Collecting baseline (0% loss)..."; \
	$(MAKE) lab5-recover > /dev/null 2>&1 || true; \
	sleep 30; \
	$(MAKE) lab5a-collect-latency RESULTS_DIR="$$RESULTS_DIR" LOSS_PCT=0; \
	echo ""; \
	for LOSS in 1 5 10 20; do \
		echo "Testing $${LOSS}% packet loss..."; \
		$(MAKE) lab5-packet-loss LOSS=$$LOSS > /dev/null 2>&1; \
		sleep 10; \
		$(MAKE) lab5a-collect-latency RESULTS_DIR="$$RESULTS_DIR" LOSS_PCT=$$LOSS; \
		$(MAKE) lab5-recover > /dev/null 2>&1; \
		sleep 15; \
	done; \
	echo ""; \
	echo "╔════════════════════════════════════════════════════════════╗"; \
	echo "║  Lab 5a Sweep Complete                                     ║"; \
	echo "╚════════════════════════════════════════════════════════════╝"; \
	echo ""; \
	cat "$$RESULTS_DIR/latency.csv"; \
	echo ""; \
	echo "Results saved to: $$RESULTS_DIR"

lab5a-collect-latency: ## Collect latency metrics (internal target, use RESULTS_DIR and LOSS_PCT)
	@RESULTS_DIR=$${RESULTS_DIR:-/tmp/lab5a}; \
	LOSS_PCT=$${LOSS_PCT:-0}; \
	DURATION=$${DURATION:-60}; \
	METRICS_FILE="$$RESULTS_DIR/metrics-$${LOSS_PCT}pct.jsonl"; \
	echo "  Collecting metrics for $${DURATION}s at $${LOSS_PCT}% loss..."; \
	sleep $$DURATION; \
	> "$$METRICS_FILE"; \
	for container in $$(docker ps --filter "name=clab-" --format '{{.Names}}'); do \
		timeout 5 docker logs "$$container" 2>&1 | grep 'METRICS:' | sed 's/.*METRICS: //' >> "$$METRICS_FILE" 2>/dev/null || true; \
	done; \
	LATENCIES=$$(grep '"event_type":"DocumentReceived"' "$$METRICS_FILE" 2>/dev/null | jq -r '.latency_ms // empty' 2>/dev/null | sort -n || true); \
	COUNT=$$(echo "$$LATENCIES" | grep -c . 2>/dev/null || echo 0); \
	if [ "$$COUNT" -gt 0 ]; then \
		P50_IDX=$$((COUNT * 50 / 100)); \
		P95_IDX=$$((COUNT * 95 / 100)); \
		P99_IDX=$$((COUNT * 99 / 100)); \
		P50=$$(echo "$$LATENCIES" | sed -n "$${P50_IDX}p"); \
		P95=$$(echo "$$LATENCIES" | sed -n "$${P95_IDX}p"); \
		P99=$$(echo "$$LATENCIES" | sed -n "$${P99_IDX}p"); \
		echo "  $${LOSS_PCT}% loss: P50=$${P50}ms P95=$${P95}ms P99=$${P99}ms (n=$$COUNT)"; \
		echo "$$(date -Iseconds),$${LOSS_PCT},$${P50},$${P95},$${P99},$$COUNT" >> "$$RESULTS_DIR/latency.csv"; \
	else \
		echo "  $${LOSS_PCT}% loss: No metrics collected"; \
		echo "$$(date -Iseconds),$${LOSS_PCT},N/A,N/A,N/A,0" >> "$$RESULTS_DIR/latency.csv"; \
	fi

lab5a-report: ## Generate Lab 5a degradation curve analysis from sweep results
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 5a: Packet Loss Degradation Analysis                  ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@LATEST=$$(ls -td /tmp/lab5a-sweep-* 2>/dev/null | head -1); \
	if [ -z "$$LATEST" ] || [ ! -f "$$LATEST/latency.csv" ]; then \
		echo "ERROR: No Lab 5a results found. Run 'make lab5a-sweep' first."; \
		exit 1; \
	fi; \
	echo ""; \
	echo "Results from: $$LATEST"; \
	echo ""; \
	echo "=== Latency vs Packet Loss ==="; \
	echo ""; \
	printf "%-10s %-12s %-12s %-12s %-10s\n" "Loss%" "P50 (ms)" "P95 (ms)" "P99 (ms)" "Samples"; \
	echo "------------------------------------------------------------"; \
	tail -n +2 "$$LATEST/latency.csv" | while IFS=, read -r ts loss p50 p95 p99 count; do \
		printf "%-10s %-12s %-12s %-12s %-10s\n" "$${loss}%" "$$p50" "$$p95" "$$p99" "$$count"; \
	done; \
	echo ""; \
	echo "=== Success Criteria (Epic #471) ==="; \
	echo ""; \
	P95_AT_10=$$(tail -n +2 "$$LATEST/latency.csv" | awk -F, '$$2 == 10 {print $$4}'); \
	if [ -n "$$P95_AT_10" ] && [ "$$P95_AT_10" != "N/A" ]; then \
		if [ $$(echo "$$P95_AT_10 < 1000" | bc -l 2>/dev/null || echo 0) -eq 1 ]; then \
			echo "[PASS] System functional at 10% loss (P95=$${P95_AT_10}ms < 1000ms)"; \
		else \
			echo "[FAIL] System degraded at 10% loss (P95=$${P95_AT_10}ms >= 1000ms)"; \
		fi; \
	else \
		echo "[SKIP] No data at 10% loss"; \
	fi; \
	BASELINE_P95=$$(tail -n +2 "$$LATEST/latency.csv" | awk -F, '$$2 == 0 {print $$4}'); \
	P95_AT_20=$$(tail -n +2 "$$LATEST/latency.csv" | awk -F, '$$2 == 20 {print $$4}'); \
	if [ -n "$$BASELINE_P95" ] && [ "$$BASELINE_P95" != "N/A" ] && [ -n "$$P95_AT_20" ] && [ "$$P95_AT_20" != "N/A" ]; then \
		RATIO=$$(echo "scale=2; $$P95_AT_20 / $$BASELINE_P95" | bc -l 2>/dev/null || echo "N/A"); \
		echo "Degradation factor at 20% loss: $${RATIO}x baseline"; \
	fi

lab5-intermittent: ## Lab 5b: Simulate intermittent connectivity (CYCLE=30s on/off)
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 5b: Intermittent Connectivity (cycle=$${CYCLE:-30}s)   ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@CONTAINERS=$$(docker ps --filter "name=clab-" --format '{{.Names}}' | wc -l); \
	if [ "$$CONTAINERS" -eq 0 ]; then \
		echo "ERROR: No deployment found. Run 'make lab4-96' first."; \
		exit 1; \
	fi
	@echo "Starting intermittent connectivity simulation..."
	@echo "  - $${CYCLE:-30}s connected, $${CYCLE:-30}s disconnected"
	@echo "  - Press Ctrl+C to stop"
	@./chaos-inject.sh intermittent --cycle $${CYCLE:-30}

lab5b-sweep: ## Lab 5b: Run intermittent connectivity sweep (5s, 15s, 30s disconnect durations)
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 5b: Intermittent Connectivity Sweep                   ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@CONTAINERS=$$(docker ps --filter "name=clab-" --format '{{.Names}}' | wc -l); \
	if [ "$$CONTAINERS" -eq 0 ]; then \
		echo "ERROR: No deployment found. Run 'make lab4-96' first."; \
		exit 1; \
	fi
	@RESULTS_DIR="/tmp/lab5b-sweep-$$(date +%Y%m%d-%H%M%S)"; \
	mkdir -p "$$RESULTS_DIR"; \
	echo "Results: $$RESULTS_DIR"; \
	echo "disconnect_secs,recovery_ms,consistency,status" > "$$RESULTS_DIR/recovery.csv"; \
	echo ""; \
	for DISCONNECT in 5 15 30; do \
		echo "Testing $${DISCONNECT}s disconnect..."; \
		echo ""; \
		echo "  Applying 100% packet loss (simulating disconnect)..."; \
		TS_START=$$(date +%s%3N); \
		for container in $$(docker ps --filter "name=clab-" --format '{{.Names}}'); do \
			docker exec $$container tc qdisc replace dev eth0 root netem loss 100% 2>/dev/null || \
			docker exec $$container tc qdisc add dev eth0 root netem loss 100% 2>/dev/null || true; \
		done; \
		echo "{\"type\":\"inject\",\"mode\":\"intermittent\",\"timestamp_ms\":$$TS_START,\"details\":{\"disconnect_secs\":$$DISCONNECT}}" >> /tmp/lab5-chaos-events.jsonl; \
		sleep $$DISCONNECT; \
		echo "  Restoring connectivity..."; \
		for container in $$(docker ps --filter "name=clab-" --format '{{.Names}}'); do \
			docker exec $$container tc qdisc del dev eth0 root 2>/dev/null || true; \
		done; \
		TS_RECONNECT=$$(date +%s%3N); \
		echo "{\"type\":\"recover\",\"mode\":\"intermittent\",\"timestamp_ms\":$$TS_RECONNECT,\"details\":{\"disconnect_secs\":$$DISCONNECT}}" >> /tmp/lab5-chaos-events.jsonl; \
		echo "  Monitoring for sync recovery..."; \
		LEADER=$$(docker ps --filter "name=clab-" --format '{{.Names}}' | grep -E "squad.*leader" | head -1); \
		BASELINE_COUNT=$$(docker logs "$$LEADER" 2>&1 | grep -c "AggregationCompleted" || echo 0); \
		RECOVERY_MS=0; \
		for i in $$(seq 1 60); do \
			sleep 1; \
			CURRENT_COUNT=$$(docker logs "$$LEADER" 2>&1 | grep -c "AggregationCompleted" || echo 0); \
			if [ "$$CURRENT_COUNT" -gt "$$BASELINE_COUNT" ]; then \
				TS_RECOVERED=$$(date +%s%3N); \
				RECOVERY_MS=$$((TS_RECOVERED - TS_RECONNECT)); \
				echo "  Sync resumed after $${RECOVERY_MS}ms"; \
				break; \
			fi; \
		done; \
		if [ "$$RECOVERY_MS" -eq 0 ]; then \
			RECOVERY_MS=60000; \
			echo "  No sync detected within 60s"; \
		fi; \
		echo "  Verifying document convergence..."; \
		sleep 5; \
		DOC_COUNTS=""; \
		for leader in $$(docker ps --filter "name=clab-" --format '{{.Names}}' | grep -E "squad.*leader" | head -4); do \
			COUNT=$$(docker logs "$$leader" 2>&1 | grep "AggregationCompleted" | tail -1 | grep -oP '"input_count":\K[0-9]+' || echo 0); \
			DOC_COUNTS="$$DOC_COUNTS $$COUNT"; \
		done; \
		UNIQUE_COUNTS=$$(echo $$DOC_COUNTS | tr ' ' '\n' | sort -u | grep -v '^$$' | wc -l); \
		if [ "$$UNIQUE_COUNTS" -le 1 ] && [ -n "$$DOC_COUNTS" ]; then \
			CONVERGED="CONVERGED"; \
		else \
			CONVERGED="DIVERGED"; \
		fi; \
		echo "  Squad leader input counts:$$DOC_COUNTS ($$CONVERGED)"; \
		echo "  $${DISCONNECT}s disconnect: Recovery=$${RECOVERY_MS}ms Consistency=$$CONVERGED"; \
		if [ $$RECOVERY_MS -lt 30000 ]; then \
			STATUS="PASS"; \
		else \
			STATUS="FAIL"; \
		fi; \
		echo "$$DISCONNECT,$$RECOVERY_MS,$${CONVERGED},$$STATUS" >> "$$RESULTS_DIR/recovery.csv"; \
		echo ""; \
		sleep 10; \
	done; \
	echo "╔════════════════════════════════════════════════════════════╗"; \
	echo "║  Lab 5b Sweep Complete                                     ║"; \
	echo "╚════════════════════════════════════════════════════════════╝"; \
	echo ""; \
	cat "$$RESULTS_DIR/recovery.csv"; \
	echo ""; \
	echo "Results saved to: $$RESULTS_DIR"

lab5b-report: ## Lab 5b: Generate intermittent connectivity analysis report
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 5b: Intermittent Connectivity Analysis                ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@LATEST=$$(ls -td /tmp/lab5b-sweep-* 2>/dev/null | head -1); \
	if [ -z "$$LATEST" ] || [ ! -f "$$LATEST/recovery.csv" ]; then \
		echo "ERROR: No Lab 5b results found. Run 'make lab5b-sweep' first."; \
		exit 1; \
	fi; \
	echo ""; \
	echo "Results from: $$LATEST"; \
	echo ""; \
	echo "=== Recovery Time vs Disconnect Duration ==="; \
	echo ""; \
	printf "%-15s %-15s %-15s %-10s\n" "Disconnect" "Recovery" "Consistency" "Status"; \
	echo "------------------------------------------------------------"; \
	tail -n +2 "$$LATEST/recovery.csv" | while IFS=, read -r disc recov consist status; do \
		printf "%-15s %-15s %-15s %-10s\n" "$${disc}s" "$${recov}ms" "$$consist" "$$status"; \
	done; \
	echo ""; \
	echo "=== Success Criteria (Epic #471) ==="; \
	echo ""; \
	PASSED=$$(tail -n +2 "$$LATEST/recovery.csv" | grep -c ",PASS$$" || echo 0); \
	TOTAL=$$(tail -n +2 "$$LATEST/recovery.csv" | wc -l); \
	if [ "$$PASSED" -eq "$$TOTAL" ] && [ "$$TOTAL" -gt 0 ]; then \
		echo "[PASS] All recovery times < 30s ($$PASSED/$$TOTAL tests)"; \
	else \
		echo "[FAIL] Some recovery times >= 30s ($$PASSED/$$TOTAL passed)"; \
	fi; \
	CONSISTENT=$$(tail -n +2 "$$LATEST/recovery.csv" | grep -c ",CONVERGED," || echo 0); \
	if [ "$$CONSISTENT" -eq "$$TOTAL" ] && [ "$$TOTAL" -gt 0 ]; then \
		echo "[PASS] Documents converged after all tests ($$CONSISTENT/$$TOTAL)"; \
	else \
		echo "[WARN] Some convergence issues ($$CONSISTENT/$$TOTAL converged)"; \
	fi

lab5-churn: ## Lab 5c: Simulate node churn (RATE=1 node every 10s)
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 5c: Node Churn Simulation                             ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@CONTAINERS=$$(docker ps --filter "name=clab-" --format '{{.Names}}' | wc -l); \
	if [ "$$CONTAINERS" -eq 0 ]; then \
		echo "ERROR: No deployment found. Run 'make lab4-96' first."; \
		exit 1; \
	fi
	@echo "Starting node churn simulation..."
	@./chaos-inject.sh churn --rate $${RATE:-10}

lab5c-sweep: ## Lab 5c: Run node churn sweep (5%, 10%, 20% of soldiers killed/restarted)
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 5c: Node Churn Sweep (5%, 10%, 20%)                   ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@CONTAINERS=$$(docker ps --filter "name=clab-" --format '{{.Names}}' | wc -l); \
	if [ "$$CONTAINERS" -eq 0 ]; then \
		echo "ERROR: No deployment found. Run 'make lab4-96' first."; \
		exit 1; \
	fi
	@RESULTS_DIR="/tmp/lab5c-sweep-$$(date +%Y%m%d-%H%M%S)"; \
	mkdir -p "$$RESULTS_DIR"; \
	echo "Results: $$RESULTS_DIR"; \
	echo "churn_pct,nodes_killed,rejoin_ms,consistency,status" > "$$RESULTS_DIR/churn.csv"; \
	SOLDIERS=$$(docker ps --filter "name=clab-" --format '{{.Names}}' | grep "soldier" | grep -v "leader"); \
	TOTAL_SOLDIERS=$$(echo "$$SOLDIERS" | wc -l); \
	echo "Total soldiers: $$TOTAL_SOLDIERS"; \
	echo ""; \
	for PCT in 5 10 20; do \
		echo "Testing $${PCT}% churn..."; \
		KILL_COUNT=$$((TOTAL_SOLDIERS * PCT / 100)); \
		if [ "$$KILL_COUNT" -lt 1 ]; then KILL_COUNT=1; fi; \
		echo "  Killing $$KILL_COUNT soldiers..."; \
		VICTIMS=$$(echo "$$SOLDIERS" | shuf -n $$KILL_COUNT); \
		TS_KILL=$$(date +%s%3N); \
		echo "{\"type\":\"inject\",\"mode\":\"churn\",\"timestamp_ms\":$$TS_KILL,\"details\":{\"pct\":$$PCT,\"count\":$$KILL_COUNT}}" >> /tmp/lab5-chaos-events.jsonl; \
		for victim in $$VICTIMS; do \
			docker kill --signal=SIGKILL "$$victim" 2>/dev/null || true; \
		done; \
		echo "  Waiting for containers to restart..."; \
		sleep 10; \
		for victim in $$VICTIMS; do \
			docker start "$$victim" 2>/dev/null || true; \
		done; \
		TS_RESTART=$$(date +%s%3N); \
		echo "  Monitoring for rejoin..."; \
		REJOIN_MS=0; \
		LEADER=$$(docker ps --filter "name=clab-" --format '{{.Names}}' | grep -E "squad.*leader" | head -1); \
		BASELINE=$$(docker logs "$$LEADER" 2>&1 | grep -c "AggregationCompleted" || echo 0); \
		for i in $$(seq 1 60); do \
			sleep 1; \
			CURRENT=$$(docker logs "$$LEADER" 2>&1 | grep -c "AggregationCompleted" || echo 0); \
			if [ "$$CURRENT" -gt "$$BASELINE" ]; then \
				TS_REJOIN=$$(date +%s%3N); \
				REJOIN_MS=$$((TS_REJOIN - TS_RESTART)); \
				echo "  Nodes rejoined after $${REJOIN_MS}ms"; \
				break; \
			fi; \
		done; \
		if [ "$$REJOIN_MS" -eq 0 ]; then \
			REJOIN_MS=60000; \
			echo "  No rejoin detected within 60s"; \
		fi; \
		echo "{\"type\":\"recover\",\"mode\":\"churn\",\"timestamp_ms\":$$(date +%s%3N),\"details\":{\"rejoin_ms\":$$REJOIN_MS}}" >> /tmp/lab5-chaos-events.jsonl; \
		echo "  Verifying convergence..."; \
		sleep 5; \
		DOC_COUNTS=""; \
		for leader in $$(docker ps --filter "name=clab-" --format '{{.Names}}' | grep -E "squad.*leader" | head -4); do \
			COUNT=$$(docker logs "$$leader" 2>&1 | grep "AggregationCompleted" | tail -1 | grep -oP '"input_count":\K[0-9]+' || echo 0); \
			DOC_COUNTS="$$DOC_COUNTS $$COUNT"; \
		done; \
		UNIQUE=$$(echo $$DOC_COUNTS | tr ' ' '\n' | sort -u | grep -v '^$$' | wc -l); \
		if [ "$$UNIQUE" -le 1 ] && [ -n "$$DOC_COUNTS" ]; then \
			CONVERGED="CONVERGED"; \
		else \
			CONVERGED="DIVERGED"; \
		fi; \
		echo "  Squad counts:$$DOC_COUNTS ($$CONVERGED)"; \
		if [ $$REJOIN_MS -lt 30000 ]; then \
			STATUS="PASS"; \
		else \
			STATUS="FAIL"; \
		fi; \
		echo "$$PCT,$$KILL_COUNT,$$REJOIN_MS,$$CONVERGED,$$STATUS" >> "$$RESULTS_DIR/churn.csv"; \
		echo "  $${PCT}% churn: Rejoin=$${REJOIN_MS}ms Consistency=$$CONVERGED $$STATUS"; \
		echo ""; \
		sleep 10; \
	done; \
	echo "╔════════════════════════════════════════════════════════════╗"; \
	echo "║  Lab 5c Sweep Complete                                     ║"; \
	echo "╚════════════════════════════════════════════════════════════╝"; \
	echo ""; \
	cat "$$RESULTS_DIR/churn.csv"; \
	echo ""; \
	echo "Results saved to: $$RESULTS_DIR"

lab5c-report: ## Lab 5c: Generate node churn analysis report
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 5c: Node Churn Analysis                               ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@LATEST=$$(ls -td /tmp/lab5c-sweep-* 2>/dev/null | head -1); \
	if [ -z "$$LATEST" ] || [ ! -f "$$LATEST/churn.csv" ]; then \
		echo "ERROR: No Lab 5c results found. Run 'make lab5c-sweep' first."; \
		exit 1; \
	fi; \
	echo ""; \
	echo "Results from: $$LATEST"; \
	echo ""; \
	echo "=== Rejoin Time vs Churn Rate ==="; \
	echo ""; \
	printf "%-10s %-12s %-12s %-12s %-10s\n" "Churn%" "Killed" "Rejoin" "Consistency" "Status"; \
	echo "------------------------------------------------------------"; \
	tail -n +2 "$$LATEST/churn.csv" | while IFS=, read -r pct killed rejoin consist status; do \
		printf "%-10s %-12s %-12s %-12s %-10s\n" "$${pct}%" "$$killed" "$${rejoin}ms" "$$consist" "$$status"; \
	done; \
	echo ""; \
	echo "=== Success Criteria (Epic #471) ==="; \
	echo ""; \
	PASSED=$$(tail -n +2 "$$LATEST/churn.csv" | grep -c ",PASS$$" || echo 0); \
	TOTAL=$$(tail -n +2 "$$LATEST/churn.csv" | wc -l); \
	if [ "$$PASSED" -eq "$$TOTAL" ] && [ "$$TOTAL" -gt 0 ]; then \
		echo "[PASS] All rejoin times < 30s ($$PASSED/$$TOTAL tests)"; \
	else \
		echo "[FAIL] Some rejoin times >= 30s ($$PASSED/$$TOTAL passed)"; \
	fi; \
	CONSISTENT=$$(tail -n +2 "$$LATEST/churn.csv" | grep -c ",CONVERGED," || echo 0); \
	if [ "$$CONSISTENT" -eq "$$TOTAL" ] && [ "$$TOTAL" -gt 0 ]; then \
		echo "[PASS] CRDT consistency maintained ($$CONSISTENT/$$TOTAL)"; \
	else \
		echo "[WARN] Some consistency issues ($$CONSISTENT/$$TOTAL converged)"; \
	fi

lab5-knockout: ## Lab 5d: Kill random soldier node (TARGET=soldier)
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 5d: Single Node Knockout (type=$${TARGET:-soldier})    ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@CONTAINERS=$$(docker ps --filter "name=clab-lab4" --format '{{.Names}}' | wc -l); \
	if [ "$$CONTAINERS" -eq 0 ]; then \
		echo "ERROR: No Lab 4 deployment found. Run 'make lab4-96' first."; \
		exit 1; \
	fi
	@TARGET_TYPE=$${TARGET:-soldier}; \
	VICTIM=$$(docker ps --filter "name=clab-lab4" --format '{{.Names}}' | grep "$$TARGET_TYPE" | shuf -n 1); \
	if [ -z "$$VICTIM" ]; then \
		echo "ERROR: No container matching type '$$TARGET_TYPE' found"; \
		exit 1; \
	fi; \
	echo "Killing node: $$VICTIM"; \
	echo "Time: $$(date +%H:%M:%S.%3N)"; \
	TS=$$(date +%s%3N); \
	echo "{\"type\":\"inject\",\"mode\":\"knockout\",\"timestamp_ms\":$$TS,\"details\":{\"node\":\"$$VICTIM\",\"node_type\":\"$$TARGET_TYPE\"}}" >> /tmp/lab5-chaos-events.jsonl; \
	docker kill --signal=SIGKILL $$VICTIM; \
	TS=$$(date +%s%3N); \
	echo "{\"type\":\"recover\",\"mode\":\"knockout\",\"timestamp_ms\":$$TS,\"details\":{\"node\":\"$$VICTIM\",\"status\":\"killed\"}}" >> /tmp/lab5-chaos-events.jsonl; \
	echo "✓ Node killed. Monitor recovery with: make lab5-status"

lab5-leader-kill: ## Lab 5e: Kill a leader node (LEVEL=squad|platoon|company)
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 5e: Leader Assassination ($${LEVEL:-squad} leader)     ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@CONTAINERS=$$(docker ps --filter "name=clab-lab4" --format '{{.Names}}' | wc -l); \
	if [ "$$CONTAINERS" -eq 0 ]; then \
		echo "ERROR: No Lab 4 deployment found. Run 'make lab4-96' first."; \
		exit 1; \
	fi
	@LEVEL=$${LEVEL:-squad}; \
	case "$$LEVEL" in \
		squad) PATTERN="squad-[0-9]+-leader" ;; \
		platoon) PATTERN="platoon-[0-9]+-leader" ;; \
		company) PATTERN="commander" ;; \
		*) echo "ERROR: LEVEL must be squad, platoon, or company"; exit 1 ;; \
	esac; \
	VICTIM=$$(docker ps --filter "name=clab-lab4" --format '{{.Names}}' | grep -E "$$PATTERN" | shuf -n 1); \
	if [ -z "$$VICTIM" ]; then \
		echo "ERROR: No $$LEVEL leader found"; \
		exit 1; \
	fi; \
	echo "Assassinating $$LEVEL leader: $$VICTIM"; \
	echo "Time: $$(date +%H:%M:%S.%3N)"; \
	TS=$$(date +%s%3N); \
	echo "{\"type\":\"inject\",\"mode\":\"leader_kill\",\"timestamp_ms\":$$TS,\"details\":{\"node\":\"$$VICTIM\",\"level\":\"$$LEVEL\"}}" >> /tmp/lab5-chaos-events.jsonl; \
	docker kill --signal=SIGKILL $$VICTIM; \
	TS=$$(date +%s%3N); \
	echo "{\"type\":\"recover\",\"mode\":\"leader_kill\",\"timestamp_ms\":$$TS,\"details\":{\"node\":\"$$VICTIM\",\"status\":\"killed\"}}" >> /tmp/lab5-chaos-events.jsonl; \
	echo "✓ Leader killed. Monitor hierarchy recovery with: make lab5-status"

lab5-partition: ## Lab 5f: Create network partition (GROUPS=2 splits network in half)
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 5f: Network Partition ($${GROUPS:-2}-way split)        ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@CONTAINERS=$$(docker ps --filter "name=clab-lab4" --format '{{.Names}}' | wc -l); \
	if [ "$$CONTAINERS" -eq 0 ]; then \
		echo "ERROR: No Lab 4 deployment found. Run 'make lab4-96' first."; \
		exit 1; \
	fi
	@echo "Creating network partition..."
	@./chaos-inject.sh partition --groups $${GROUPS:-2}

lab5-blackout: ## Lab 5g: Total network blackout (DURATION=10s)
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 5g: Burst Jamming / Blackout ($${DURATION:-10}s)       ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@CONTAINERS=$$(docker ps --filter "name=clab-lab4" --format '{{.Names}}' | wc -l); \
	if [ "$$CONTAINERS" -eq 0 ]; then \
		echo "ERROR: No Lab 4 deployment found. Run 'make lab4-96' first."; \
		exit 1; \
	fi
	@echo "Initiating $${DURATION:-10}s blackout..."
	@TS=$$(date +%s%3N); \
	echo "{\"type\":\"inject\",\"mode\":\"blackout\",\"timestamp_ms\":$$TS,\"details\":{\"duration_seconds\":$${DURATION:-10}}}" >> /tmp/lab5-chaos-events.jsonl
	@echo "Start time: $$(date +%H:%M:%S.%3N)"
	@for container in $$(docker ps --filter "name=clab-lab4" --format '{{.Names}}'); do \
		docker exec $$container tc qdisc replace dev eth0 root netem loss 100% 2>/dev/null || \
		docker exec $$container tc qdisc add dev eth0 root netem loss 100% 2>/dev/null || true; \
	done
	@echo "Network blacked out. Recovering in $${DURATION:-10}s..."
	@sleep $${DURATION:-10}
	@echo "Restoring network..."
	@for container in $$(docker ps --filter "name=clab-lab4" --format '{{.Names}}'); do \
		docker exec $$container tc qdisc del dev eth0 root 2>/dev/null || true; \
	done
	@TS=$$(date +%s%3N); \
	echo "{\"type\":\"recover\",\"mode\":\"blackout\",\"timestamp_ms\":$$TS,\"details\":{\"duration_seconds\":$${DURATION:-10}}}" >> /tmp/lab5-chaos-events.jsonl
	@echo "End time: $$(date +%H:%M:%S.%3N)"
	@echo "✓ Blackout complete. Monitor recovery with: make lab5-status"

lab5-recover: ## Remove all chaos injections and restore normal operation
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 5: Removing Chaos Injections                          ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@echo "Removing tc qdisc rules from all containers..."
	@for container in $$(docker ps --filter "name=clab-lab4" --format '{{.Names}}'); do \
		docker exec $$container tc qdisc del dev eth0 root 2>/dev/null || true; \
	done
	@echo "✓ Chaos removed. Network restored to normal."

lab5-status: ## Show chaos test status and recovery metrics
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 5: Chaos Status                                       ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@TOTAL=$$(docker ps --filter "name=clab-lab4" -q 2>/dev/null | wc -l); \
	RUNNING=$$(docker ps --filter "name=clab-lab4" --filter "status=running" -q 2>/dev/null | wc -l); \
	EXITED=$$(docker ps -a --filter "name=clab-lab4" --filter "status=exited" -q 2>/dev/null | wc -l); \
	echo "Containers: $$RUNNING running / $$TOTAL total ($$EXITED exited)"
	@echo ""
	@echo "=== Active tc qdisc rules (chaos injections) ==="
	@SAMPLE=$$(docker ps --filter "name=clab-lab4" --format '{{.Names}}' | head -1); \
	if [ -n "$$SAMPLE" ]; then \
		docker exec $$SAMPLE tc qdisc show dev eth0 2>/dev/null || echo "  No tc rules (clean)"; \
	else \
		echo "  No containers running"; \
	fi
	@echo ""
	@echo "=== Recent Errors (last 5 across all nodes) ==="
	@docker ps --filter "name=clab-lab4" -q | head -5 | xargs -I{} docker logs --tail 50 {} 2>&1 | \
		grep -iE "error|panic|fatal|timeout|disconnect" | grep -v "No error" | tail -5 || echo "  None"
	@echo ""
	@echo "=== Sync Health (sample from commander) ==="
	@COMMANDER=$$(docker ps --format '{{.Names}}' | grep "commander" | head -1); \
	if [ -n "$$COMMANDER" ]; then \
		LAST_SYNC=$$(docker logs $$COMMANDER 2>&1 | grep "AggregationCompleted" | tail -1); \
		if [ -n "$$LAST_SYNC" ]; then \
			echo "  Last aggregation: $$(echo $$LAST_SYNC | grep -oP '"timestamp":"[^"]+"' | head -1)"; \
		else \
			echo "  No aggregations yet"; \
		fi; \
	else \
		echo "  No commander found"; \
	fi

lab5-all: ## Run complete Lab 5 chaos test suite (requires lab4-96 running)
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 5: Full Chaos Engineering Suite                       ║"
	@echo "║  This will run all chaos scenarios sequentially            ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@CONTAINERS=$$(docker ps --filter "name=clab-lab4" --format '{{.Names}}' | wc -l); \
	if [ "$$CONTAINERS" -eq 0 ]; then \
		echo "ERROR: No Lab 4 deployment found. Run 'make lab4-96' first."; \
		exit 1; \
	fi
	@./run-lab5-chaos-suite.sh

lab5-recovery-time: ## Measure recovery time after chaos event (TARGET=60000ms)
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 5: Recovery Time Measurement                          ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@./measure-recovery.sh --target $${TARGET:-60000}

lab5-verify-consistency: ## Verify CRDT data consistency across all nodes
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 5: Data Consistency Verification                      ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@./verify-consistency.sh

lab5-report: ## Generate Lab 5 results report with pass/fail against targets
	@echo "╔════════════════════════════════════════════════════════════╗"
	@echo "║  Lab 5: Test Results Report                                ║"
	@echo "╚════════════════════════════════════════════════════════════╝"
	@./lab5-report.sh
