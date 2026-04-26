# M5Stack Core2 Peat Demo - Makefile
#
# Commands:
#   make build      - Build the firmware
#   make flash      - Flash to first available device
#   make flash-all  - Flash to all three devices in parallel
#   make monitor    - Monitor first device
#   make clean      - Clean build artifacts

# Device serial ports (update if your devices differ)
DEV1 := /dev/ttyACM0
DEV2 := /dev/ttyACM1
DEV3 := /dev/ttyACM2

TARGET := target/xtensa-esp32-espidf/release/m5stack-core2-peat

.PHONY: build flash flash-all flash1 flash2 flash3 monitor clean

build:
	@echo "Building firmware..."
	@source ~/export-esp.sh && cargo build --release

flash: build
	@echo "Flashing to first available device..."
	espflash flash $(TARGET)

flash-all: build
	@echo "Flashing to all three devices in parallel..."
	@$(MAKE) -j3 flash1 flash2 flash3
	@echo "All devices flashed!"

flash1:
	@echo "Flashing device 1: $(DEV1)"
	@espflash flash --port $(DEV1) $(TARGET) 2>&1 | sed 's/^/[DEV1] /'

flash2:
	@echo "Flashing device 2: $(DEV2)"
	@espflash flash --port $(DEV2) $(TARGET) 2>&1 | sed 's/^/[DEV2] /'

flash3:
	@echo "Flashing device 3: $(DEV3)"
	@espflash flash --port $(DEV3) $(TARGET) 2>&1 | sed 's/^/[DEV3] /'

monitor:
	espflash monitor --port $(DEV1)

monitor1:
	espflash monitor --port $(DEV1)

monitor2:
	espflash monitor --port $(DEV2)

monitor3:
	espflash monitor --port $(DEV3)

clean:
	cargo clean
