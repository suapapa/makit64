# makit64

ESP32-S3 firmware for a **HUB75E 64×64** LED matrix (`LED_metrix_mcu_ver_rev.1`).

- Hardware notes: [`HARDWARE.md`](HARDWARE.md)
- Schematic: [`sch/sch.png`](sch/sch.png)
- Pin constants: [`src/board.rs`](src/board.rs)
- Agent guide: [`AGENTS.md`](AGENTS.md)
- Network plan: [`docs/PLAN-wifi-coap.md`](docs/PLAN-wifi-coap.md)

## Toolchain

Generated with [`esp-generate`](https://github.com/esp-rs/esp-generate) (successor to the deprecated `cargo generate` + `esp-rs/esp-template`).

Requires the Espressif Rust toolchain (`espup`) and `espflash`.

## Wi-Fi credentials

Copy the example env file and fill in your LAN AP:

```bash
cp .env.example .env
# edit WIFI_SSID / WIFI_PASS
```

Credentials and `DISPLAY_ROTATION` (clockwise degrees: `0`/`90`/`180`/`270`) are
baked in at **compile time** via `build.rs`. `.env` is gitignored — never commit it.

## Build & flash

```bash
make build      # release build
make flash      # build + flash (no log)
make monitor    # serial log only
make run        # build + flash + log  (same idea as cargo run --release)
```

`espflash` resets the ESP32-S3 over native USB (DTR/RTS) — you usually do **not**
need the BOOT/RESET buttons. If several boards are plugged in:

```bash
PORT=/dev/cu.usbmodemXXXX make flash
```

Or use Cargo directly:

```bash
cargo build --release
cargo run --release
```

After flash the panel shows bring-up text (`boot` → `wifi` / `retry` → `dhcp`),
then **`makit.local`** and the assigned IPv4. Serial also prints them. mDNS answers
A queries for `makit.local`.

## Push an image (CoAP PUT)

Status text stays until the first frame arrives.

```bash
pip install pillow aiocoap
python tools/put_frame.py tools/tiger.png                  # → makit.local
python tools/put_frame.py -H 192.168.x.y your.png          # direct IP
```

- Endpoint: `coap://makit.local:5683/frame` (or the panel IP)
- Body: raw **64×64 RGB888** (12 288 bytes), Block1 for chunking
- Success: CoAP `2.04 Changed`

## Current status

- HUB75 DMA + Wi-Fi STA + mDNS (`makit.local`) + CoAP `PUT /frame`
