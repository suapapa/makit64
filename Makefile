# makit64 — ESP32-S3 HUB75 firmware helpers
#
# espflash talks to the ESP32-S3 native USB (IO19/IO20). It toggles DTR/RTS to
# enter download mode and reset the chip, so you normally do not press BOOT/RESET.
#
# Optional: PORT=/dev/cu.usbmodemXXXX make flash

CHIP    ?= esp32s3
TARGET  ?= xtensa-esp32s3-none-elf
BIN     := target/$(TARGET)/release/makit64
PORT_ARG := $(if $(PORT),--port $(PORT),)

ESPFLASH ?= espflash
CARGO    ?= cargo

.PHONY: help build check clippy flash monitor run clean

help:
	@echo "Targets:"
	@echo "  make build     - release build"
	@echo "  make flash     - build + flash (no monitor)"
	@echo "  make monitor   - serial log only (CTRL+C to exit)"
	@echo "  make run       - build + flash + monitor (same as cargo run --release)"
	@echo "  make check     - cargo check --release"
	@echo "  make clippy    - cargo clippy --release"
	@echo "  make clean     - cargo clean"
	@echo ""
	@echo "PORT=/dev/cu.usbmodemXXXX make flash|monitor|run  # pick a serial port"

build:
	$(CARGO) build --release

check:
	$(CARGO) check --release

clippy:
	$(CARGO) clippy --release

flash: build
	$(ESPFLASH) flash --chip $(CHIP) $(PORT_ARG) $(BIN)

monitor:
	$(ESPFLASH) monitor --chip $(CHIP) $(PORT_ARG)

run: build
	$(ESPFLASH) flash --chip $(CHIP) --monitor $(PORT_ARG) $(BIN)

clean:
	$(CARGO) clean
