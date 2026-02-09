# HIVE Port Operations Simulation
# ================================
#
# Quick start:
#   make install         # one-time setup
#   make phase0          # single crane agent → relay → viewer
#   make phase1a         # multi-agent (2 cranes + aggregator)
#
# Individual components:
#   make sim-phase0      # just the Python sim (stdout)
#   make relay           # just the Rust relay (stdin ingest)
#   make viewer          # just the React viewer dev server
#
# Docker:
#   make docker-build    # build agent container
#   make docker-phase0   # run Phase 0 in Docker

SHELL := /bin/bash

# Paths (relative to this Makefile)
ROOT         := $(shell cd ../.. && pwd)
SIM_DIR      := $(shell pwd)
VIEWER_UI    := $(ROOT)/hive-viewer-ui
COMMANDER_UI := $(ROOT)/hive-commander-ui
RELAY_BIN    := $(ROOT)/target/release/hive-viewer

# Python
PYTHONPATH := $(SIM_DIR)/bridge/src:$(SIM_DIR)/agent/src
export PYTHONPATH

# Defaults (override on command line: make phase0 MAX_CYCLES=5)
MAX_CYCLES       ?= 50
TIME_COMPRESSION ?= 600
PROVIDER         ?= anthropic
LISTEN           ?= 0.0.0.0:9090

# --------------------------------------------------------------------------
#  Setup
# --------------------------------------------------------------------------

.PHONY: install
install: install-python install-viewer install-commander build-relay  ## One-time setup (Python + npm + Rust)

.PHONY: install-python
install-python:  ## Install Python agent + bridge deps
	pip install -e $(SIM_DIR)/agent -e $(SIM_DIR)/bridge

.PHONY: install-viewer
install-viewer:  ## npm install for viewer UI
	cd $(VIEWER_UI) && npm install

.PHONY: install-commander
install-commander:  ## npm install for commander UI
	cd $(COMMANDER_UI) && npm install

.PHONY: build-relay
build-relay:  ## Build the Rust relay server
	cd $(ROOT) && cargo build --release -p hive-viewer

# --------------------------------------------------------------------------
#  Run: Full pipeline (sim | relay, viewer in background)
# --------------------------------------------------------------------------

.PHONY: phase0
phase0: $(RELAY_BIN)  ## Phase 0: single crane → relay → viewer (3 processes)
	@echo "=== HIVE Port Ops — Phase 0 ==="
	@echo "  Viewer:  http://localhost:5173"
	@echo "  Relay:   ws://localhost:9090/ws"
	@echo ""
	@# Start viewer dev server in background
	cd $(VIEWER_UI) && npm run dev &
	@# Pipe sim stdout into relay stdin
	python -m port_agent.main \
		--node-id crane-1 \
		--persona gantry-crane \
		--provider $(PROVIDER) \
		--max-cycles $(MAX_CYCLES) \
		--time-compression $(TIME_COMPRESSION) \
	  2>/dev/null | $(RELAY_BIN) --ingest stdin --listen $(LISTEN)

.PHONY: phase1a
phase1a: $(RELAY_BIN)  ## Phase 1a: multi-agent (2 cranes + aggregator) → relay → viewer
	@echo "=== HIVE Port Ops — Phase 1a ==="
	@echo "  Viewer:  http://localhost:5173"
	@echo "  Relay:   ws://localhost:9090/ws"
	@echo ""
	cd $(VIEWER_UI) && npm run dev &
	python -m port_agent.main \
		--mode multi \
		--agents 2c1a \
		--provider $(PROVIDER) \
		--max-cycles $(MAX_CYCLES) \
		--time-compression $(TIME_COMPRESSION) \
	  2>/dev/null | $(RELAY_BIN) --ingest stdin --listen $(LISTEN)

.PHONY: phase0-dry
phase0-dry: $(RELAY_BIN)  ## Phase 0 with dry-run provider (no API key needed)
	@$(MAKE) phase0 PROVIDER=dry-run

.PHONY: phase1a-dry
phase1a-dry: $(RELAY_BIN)  ## Phase 1a with dry-run provider (no API key needed)
	@$(MAKE) phase1a PROVIDER=dry-run

.PHONY: phase1b
phase1b: $(RELAY_BIN)  ## Phase 1b: 15-node hold team → relay → viewer
	@echo "=== HIVE Port Ops — Phase 1b (15-Node Hold Team) ==="
	@echo "  Viewer:  http://localhost:5173"
	@echo "  Relay:   ws://localhost:9090/ws"
	@echo "  Agents:  2c5w4t1s1a2x (15 total)"
	@echo ""
	cd $(VIEWER_UI) && npm run dev &
	python -m port_agent.main \
		--mode multi \
		--agents 2c5w4t1s1a2x \
		--provider $(PROVIDER) \
		--max-cycles $(MAX_CYCLES) \
		--time-compression $(TIME_COMPRESSION) \
	  2>/dev/null | $(RELAY_BIN) --ingest stdin --listen $(LISTEN)

.PHONY: phase1b-dry
phase1b-dry: $(RELAY_BIN)  ## Phase 1b with dry-run provider (no API key needed)
	@$(MAKE) phase1b PROVIDER=dry-run

.PHONY: phase2
phase2: $(RELAY_BIN)  ## Phase 2: lifecycle integration (degradation + logistics + gaps)
	@echo "=== HIVE Port Ops — Phase 2 ==="
	@echo "  Viewer:  http://localhost:3001"
	@echo "  Relay:   ws://localhost:9090/ws"
	@echo "  (Relay stays alive after sim ends — Ctrl-C to stop)"
	@echo ""
	cd $(VIEWER_UI) && npm run dev &
	@# tail -f /dev/null keeps the pipe open so relay survives after sim exits
	(python -m port_agent.main \
		--mode multi \
		--agents 2c1a \
		--provider $(PROVIDER) \
		--max-cycles $(MAX_CYCLES) \
		--time-compression $(TIME_COMPRESSION) \
	  2>/dev/null; echo "--- sim complete, relay holding ---" >&2; tail -f /dev/null) \
	  | $(RELAY_BIN) --ingest stdin --listen $(LISTEN)

.PHONY: phase2-dry
phase2-dry: $(RELAY_BIN)  ## Phase 2 with dry-run provider (no API key needed)
	@$(MAKE) phase2 PROVIDER=dry-run

# --------------------------------------------------------------------------
#  Recording (for offline replay in viewer)
# --------------------------------------------------------------------------

.PHONY: record-phase1b
record-phase1b:  ## Record Phase 1b dry-run to JSONL for replay
	@mkdir -p recordings
	python -m port_agent.main --mode multi --agents 2c5w4t1s1a2x \
		--provider dry-run --max-cycles $(MAX_CYCLES) \
		--time-compression $(TIME_COMPRESSION) \
		2>/dev/null > recordings/phase1b-$$(date +%Y%m%d-%H%M%S).jsonl
	@echo "Recording saved to recordings/"

# --------------------------------------------------------------------------
#  Run: Individual components
# --------------------------------------------------------------------------

.PHONY: sim-phase0
sim-phase0:  ## Run Phase 0 sim only (prints events to stdout)
	python -m port_agent.main \
		--node-id crane-1 \
		--persona gantry-crane \
		--provider $(PROVIDER) \
		--max-cycles $(MAX_CYCLES) \
		--time-compression $(TIME_COMPRESSION)

.PHONY: sim-phase1a
sim-phase1a:  ## Run Phase 1a sim only (prints events to stdout)
	python -m port_agent.main \
		--mode multi \
		--agents 2c1a \
		--provider $(PROVIDER) \
		--max-cycles $(MAX_CYCLES) \
		--time-compression $(TIME_COMPRESSION)

.PHONY: relay
relay: $(RELAY_BIN)  ## Start relay server (reads stdin, serves ws://localhost:9090)
	$(RELAY_BIN) --ingest stdin --listen $(LISTEN)

.PHONY: relay-tcp
relay-tcp: $(RELAY_BIN)  ## Start relay server with TCP ingest on port 9100
	$(RELAY_BIN) --ingest tcp://0.0.0.0:9100 --listen $(LISTEN)

.PHONY: relay-file
relay-file: $(RELAY_BIN)  ## Replay a recorded sim run (usage: make relay-file FILE=run.jsonl)
	$(RELAY_BIN) --ingest file://$(FILE) --listen $(LISTEN)

.PHONY: viewer
viewer:  ## Start viewer UI dev server (http://localhost:5173)
	cd $(VIEWER_UI) && npm run dev

.PHONY: viewer-build
viewer-build:  ## Build viewer for production
	cd $(VIEWER_UI) && npm run build

# --------------------------------------------------------------------------
#  Docker
# --------------------------------------------------------------------------

.PHONY: docker-build
docker-build:  ## Build port-ops agent Docker image
	docker build -f docker/Dockerfile -t hive-port-ops:latest .

.PHONY: docker-phase0
docker-phase0:  ## Run Phase 0 via Docker Compose
	docker compose -f docker-compose.phase0.yml up --build

.PHONY: docker-phase0-ollama
docker-phase0-ollama:  ## Run Phase 0 with local Ollama (GPU)
	docker compose -f docker-compose.phase0.yml --profile ollama up --build

# --------------------------------------------------------------------------
#  Development
# --------------------------------------------------------------------------

.PHONY: check
check:  ## Type-check viewer + commander and compile-check relay
	cd $(VIEWER_UI) && npx tsc --noEmit
	cd $(COMMANDER_UI) && npx tsc --noEmit
	cd $(ROOT) && cargo check -p hive-viewer

.PHONY: clean
clean:  ## Remove build artifacts
	cd $(ROOT) && cargo clean -p hive-viewer
	rm -rf $(VIEWER_UI)/dist

# --------------------------------------------------------------------------
#  Help
# --------------------------------------------------------------------------

.PHONY: help
help:  ## Show this help
	@awk -F '##' '/^[a-zA-Z].*:.*##/ {split($$1,a,":"); gsub(/^[ \t]+|[ \t]+$$/,"",$$2); printf "  \033[36m%-20s\033[0m %s\n", a[1], $$2}' $(MAKEFILE_LIST)

.DEFAULT_GOAL := help

# Ensure relay binary exists
$(RELAY_BIN):
	@echo "Building relay server..."
	cd $(ROOT) && cargo build --release -p hive-viewer
