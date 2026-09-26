# AGENTS.md — makit64

Guide for automated agents (and humans) working on this repository.

## Project summary

**makit64** is `no_std` Rust firmware for a custom **ESP32-S3** board
(`LED_metrix_mcu_ver_rev.1`) that drives a **HUB75E 64×64** (1/32 scan) LED
matrix through 74HC245 3.3V→5V level shifters.

Current bring-up shows a looping welcome: **Red → Green → Blue → Rainbow**.

Hardware source of truth:

| Doc | Role |
|-----|------|
| [`HARDWARE.md`](HARDWARE.md) | Pin map, power, UI |
| [`sch/sch.png`](sch/sch.png) | EasyEDA schematic |
| [`src/board.rs`](src/board.rs) | Firmware GPIO constants |

Do **not** invent pin numbers. Change pins only after updating schematic +
`HARDWARE.md` + `board.rs` together.

## Tech stack

| Layer | Choice |
|-------|--------|
| Language | Rust edition 2024, `no_std` + `alloc` |
| Target | `xtensa-esp32s3-none-elf` (Espressif toolchain via `espup`) |
| HAL / RTOS | `esp-hal` ~1.2, `esp-rtos` + Embassy |
| Display | [`esp-hub75`](https://crates.io/crates/esp-hub75) 0.17 (LCD_CAM + DMA) |
| Framebuffer | `hub75-framebuffer` bitplane `plain` (via `esp-hub75`) |
| Graphics | `embedded-graphics` |
| Flash / monitor | `espflash` (see `.cargo/config.toml` runner) |

Scaffold origin: [`esp-generate`](https://github.com/esp-rs/esp-generate)
(not the deprecated `esp-rs/esp-template` + `cargo generate`).

## Directory layout

```
src/
  bin/main.rs     Embassy entry: Hub75 init + welcome loop
  lib.rs          Crate root
  board.rs        Panel size + GPIO constants
  welcome.rs      R/G/B/Rainbow draw helpers
sch/sch.png       Schematic image
HARDWARE.md       Human-readable hardware doc
README.md         Build / flash quick start
.cargo/config.toml  Target, runner, ESP_LOG
```

## Key design decisions

1. **Direct-drive 16-bit pins** (`Hub75Pins16`): no external address latch on
   this PCB. Use address lines A–E on GPIO.
2. **Double-buffered DMA**: two static `FrameBuffer`s; draw into the free one,
   then `hub75.swap(fb)` → `wait_for_done().await` → `wait()`.
3. **esp-hub75 features** (ESP32-S3 defaults that work on this board class):
   `circular-dma`, `iram`, `skip-black-pixels`, `trail-blank-4`,
   `invert-blank`, `invert-oe`. Adjust blanking if ghosting/dimming appears.
4. **Pixel clock**: 20 MHz in `main.rs`. Drop to 10 MHz if the panel/cabling
   misbehaves.
5. **OE / blank**: schematic OE is active-low (`GPIO14`). Driver features
   invert blank/OE for LCD_CAM idle-low behaviour — do not “fix” OE by
   randomly flipping features without reading `esp-hub75` docs.
6. **Power**: matrix LED current must come from 5V terminals (`U5`/`U6`), not
   USB-C alone, when the panel is bright.

### HUB75 GPIO (quick reference)

| Signal | GPIO | Signal | GPIO |
|--------|------|--------|------|
| R1/G1/B1 | 42/41/40 | R2/G2/B2 | 38/39/37 |
| A/B/C/D/E | 45/36/48/35/21 | CLK/LAT/OE | 2/47/14 |
| USER1/USER2 | 6/7 | LED D12/D13 | 12/13 |
| USB D−/D+ | 19/20 | BOOT | 0 |

## Commands

```bash
# Check / build (Espressif toolchain must be active)
cargo check
cargo build --release

# Flash + serial monitor
cargo run --release

# Clippy
cargo clippy --release
```

Toolchain: `espup install` / Espressif `rust-toolchain.toml` channel `esp`.
Do not switch this crate to host `stable` for firmware builds.

## Coding conventions

- Keep pin numbers in `board.rs`; wire them in `main.rs` via `Hub75Pins16`.
- Prefer `esp-hub75` + `embedded-graphics` over bit-banging HUB75.
- Welcome / scene drawing stays in modules (`welcome.rs`, future scene modules);
  `main.rs` owns peripherals, DMA, and the swap loop.
- Use `log::info!` (ESP_LOG via `esp-println`); avoid `println!`.
- No `std`. Heap only if reintroduced deliberately (`esp-alloc`); framebuffers
  are currently `static_cell` statics.
- Match existing style: concise comments, no drive-by refactors.

## Do not

- Change HUB75 pinout without verifying `sch/sch.png` / `HARDWARE.md`
- Enable Wi-Fi/PSRAM paths that fight IRAM without re-validating flicker
- Commit secrets, local `sdkconfig`-style env files, or `target/`
- Assume panels are 1/16 scan — this board is **1/32** (needs address **E**)
- Reintroduce deprecated `esp-rs/esp-template` scaffolding

## Troubleshooting

| Symptom | Likely fix |
|---------|------------|
| Ghosting / lit blacks | Raise `trail-blank-*` or try `inter-row-blank-*` |
| Dim panel | Lower blanking; check 5V supply current |
| Wrong colours / scrambled | Recheck R1..B2 and A..E vs `board.rs` |
| Build on wrong toolchain | Use `esp` toolchain; `rustup show` |
| Flash fails | Confirm USB port; ESP32-S3 native USB is IO19/IO20 |

## Related crates

- Upstream examples: <https://github.com/liebman/esp-hub75/tree/main/examples>
  (especially `gradient-embassy` for async swap patterns)
- Framebuffer details: `hub75-framebuffer`
