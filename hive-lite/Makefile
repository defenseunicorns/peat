SHELL := /bin/bash

# HIVE-Lite Firmware - Makefile
#
# ESP32 build commands for hive-lite firmware targets.
# Requires: ESP Rust toolchain (espup install)
#
# Commands:
#   make check-lib        - Check library compiles (no_std, no features)
#   make check-wifi       - Check WiFi firmware compiles
#   make check-ota        - Check WiFi+OTA firmware compiles
#   make build            - Build WiFi+OTA firmware (release)
#   make build-image      - Build + save OTA-flashable app image
#   make flash            - Flash firmware with OTA partition table
#   make monitor          - Monitor serial output
#   make test             - Run host-side unit tests
#   make clean            - Clean build artifacts

.PHONY: help check-lib check-wifi check-ota build build-image flash monitor test clean

# ESP toolchain setup
ESP_EXPORT := source $(HOME)/export-esp.sh
TARGET := xtensa-esp32-none-elf
TOOLCHAIN := +esp

# WiFi credentials (override on command line or via environment)
SSID ?= TestNet
WIFI_PWD ?= password

# OTA signing public key (64 hex chars = 32 bytes Ed25519 pubkey)
# Get from hive-mesh-node startup log. Leave empty to disable verification.
OTA_SIGNING_PUBKEY ?=

# Feature sets
FEATURES_WIFI := m5stack-core2-wifi
FEATURES_OTA := m5stack-core2-wifi,ota

# Output paths
RELEASE_BIN := target/$(TARGET)/release/hive-lite-wifi
OTA_IMAGE := ota_firmware.bin

# Serial port (override with: make flash PORT=/dev/ttyUSB1)
PORT ?= /dev/ttyUSB0

help:
	@echo "HIVE-Lite Firmware Development"
	@echo ""
	@echo "Check (compile-only):"
	@echo "  check-lib     - Check library (no_std core, no features)"
	@echo "  check-wifi    - Check WiFi binary compiles"
	@echo "  check-ota     - Check WiFi+OTA binary compiles"
	@echo ""
	@echo "Build:"
	@echo "  build         - Build WiFi+OTA firmware (release)"
	@echo "  build-image   - Build + save OTA-flashable app image"
	@echo ""
	@echo "Flash & Monitor:"
	@echo "  flash         - Flash firmware with OTA partition table"
	@echo "  monitor       - Monitor serial output"
	@echo "  flash-monitor - Flash then monitor"
	@echo ""
	@echo "Test:"
	@echo "  test          - Run host-side unit tests"
	@echo ""
	@echo "Environment variables:"
	@echo "  SSID=name                - WiFi network name (default: TestNet)"
	@echo "  WIFI_PWD=pass            - WiFi password (default: password)"
	@echo "  PORT=/dev/x              - Serial port (default: /dev/ttyUSB0)"
	@echo "  OTA_SIGNING_PUBKEY=hex   - Ed25519 pubkey (64 hex chars, from mesh-node log)"

# ============================================
# Check (compile-only, fast feedback)
# ============================================

check-lib:
	@echo "Checking hive-lite library (no_std)..."
	$(ESP_EXPORT) && cargo $(TOOLCHAIN) check \
		--lib \
		--target $(TARGET)

check-wifi:
	@echo "Checking hive-lite WiFi firmware..."
	$(ESP_EXPORT) && SSID="$(SSID)" PWD="$(WIFI_PWD)" cargo $(TOOLCHAIN) check \
		--release \
		--features $(FEATURES_WIFI) \
		--target $(TARGET) \
		--bin hive-lite-wifi

check-ota:
	@echo "Checking hive-lite WiFi+OTA firmware..."
	$(ESP_EXPORT) && SSID="$(SSID)" PWD="$(WIFI_PWD)" OTA_SIGNING_PUBKEY="$(OTA_SIGNING_PUBKEY)" cargo $(TOOLCHAIN) check \
		--release \
		--features $(FEATURES_OTA) \
		--target $(TARGET) \
		--bin hive-lite-wifi

# ============================================
# Build
# ============================================

build:
	@echo "Building hive-lite WiFi+OTA firmware (release)..."
	$(ESP_EXPORT) && SSID="$(SSID)" PWD="$(WIFI_PWD)" OTA_SIGNING_PUBKEY="$(OTA_SIGNING_PUBKEY)" cargo $(TOOLCHAIN) build \
		--release \
		--features $(FEATURES_OTA) \
		--target $(TARGET) \
		--bin hive-lite-wifi
	@echo "Firmware built: $(RELEASE_BIN)"

build-image: build
	@echo "Creating OTA app image..."
	espflash save-image --chip esp32 $(RELEASE_BIN) $(OTA_IMAGE)
	@ls -la $(OTA_IMAGE)
	@echo "OTA image ready: $(OTA_IMAGE)"

# ============================================
# Flash & Monitor
# ============================================

flash: build
	@echo "Flashing with OTA partition table..."
	espflash flash \
		--port $(PORT) \
		--partition-table partitions_ota.csv \
		$(RELEASE_BIN) \
		--monitor

monitor:
	espflash monitor --port $(PORT)

flash-monitor: flash

# ============================================
# Test (host-side)
# ============================================

test:
	@echo "Running host-side unit tests..."
	RUSTUP_TOOLCHAIN=stable cargo test --lib --target x86_64-unknown-linux-gnu

# ============================================
# Clean
# ============================================

clean:
	cargo clean
	@rm -f $(OTA_IMAGE)
