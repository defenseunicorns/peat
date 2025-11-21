# Hive-Sim Makefile
# Simple interface for running network simulations and tests

.PHONY: help build clean deploy destroy

# Default target
.DEFAULT_GOAL := help

##@ General

help: ## Display this help message
	@echo "Hive-Sim - Network Simulation Testing"
	@echo ""
	@awk 'BEGIN {FS = ":.*##"; printf "Usage:\n  make \033[36m<target>\033[0m\n"} /^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

build: ## Build Docker image for nodes
	@echo "🔨 Building hive-sim-node Docker image..."
	docker build -f Dockerfile -t hive-sim-node:latest ..

clean: ## Destroy all deployments and clean up
	@echo "🧹 Cleaning up all deployments..."
	-docker ps -a --filter "name=clab-" -q | xargs -r docker rm -f 2>/dev/null || true
	-docker network prune -f
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
