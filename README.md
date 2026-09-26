# makit64

ESP32-S3 firmware for a **HUB75E 64×64** LED matrix (`LED_metrix_mcu_ver_rev.1`).

- Hardware notes: [`HARDWARE.md`](HARDWARE.md)
- Schematic: [`sch/sch.png`](sch/sch.png)
- Pin constants: [`src/board.rs`](src/board.rs)
- Agent guide: [`AGENTS.md`](AGENTS.md)

## Toolchain

Generated with [`esp-generate`](https://github.com/esp-rs/esp-generate) (successor to the deprecated `cargo generate` + `esp-rs/esp-template`):

```bash
esp-generate --headless --chip esp32s3 \
  -o alloc -o unstable-hal -o embassy -o log -o esp-backtrace -o vscode \
  makit64
```

Requires the Espressif Rust toolchain (`espup`) and `espflash`.

## Build & flash

```bash
cargo build --release
cargo run --release
```

## Current status

Welcome loop: **Red → Green → Blue → Rainbow** via `esp-hub75` (LCD_CAM DMA).

```bash
cargo run --release
```
