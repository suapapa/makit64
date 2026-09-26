# Plan: Wi‑Fi STA + CoAP PUT (UDP) frame update

Status: **implemented (v1)** — Wi-Fi STA + CoAP `PUT /frame` + mDNS `makit.local` + host tool + on-panel boot status  
Target: makit64 — ESP32-S3, HUB75E 64×64, Embassy / `esp-hal` ~1.2  
Related: [`AGENTS.md`](../AGENTS.md), [`HARDWARE.md`](../HARDWARE.md)

## Goal

1. Join the local Wi‑Fi network using credentials from a **`.env` file baked in at compile time**.
2. Update the LED matrix by receiving **CoAP `PUT` over UDP** with a full (or block-wise) pixel frame from a laptop on the same LAN.

Non-goals for the first shippable slice:

- SoftAP / captive portal
- On-device JPEG/PNG decode
- Persistent multi-client sessions or auth beyond “same LAN”

Done beyond original non-goals:

- mDNS hostname `makit.local` (A record)
- On-panel text for boot / Wi‑Fi / DHCP / IP (no serial required)

## Decisions (locked)

| Topic | Choice | Rationale |
|-------|--------|-----------|
| Link | Wi‑Fi **STA** + DHCP | Same LAN as laptop |
| Credentials | **`.env` → compile-time `env!`** | Simple; no NVS UI yet |
| App protocol | **CoAP** on **UDP/5683** | Named IoT protocol; PUT maps to “replace frame” |
| Payload prep | Laptop resizes to **64×64** | MCU stays dumb; no image codec |
| Pixel format (v1) | **RGB888** packed row-major | Matches `esp_hub75::Color` easily |
| CoAP resource | `PUT coap://<ip\|makit.local>:5683/frame` | Single write endpoint |
| Large body | **Block1** (RFC 7959) | 12 288 B > typical UDP/CoAP chunk |
| Display path | Receive task → channel → draw + `hub75.swap` | Keep DMA loop isolated from Wi‑Fi |
| Discovery | mDNS **`makit.local`** + panel IP text | No serial needed for bring-up |

## Architecture

```
┌──────────── Laptop ────────────┐     UDP/CoAP      ┌──────────── ESP32-S3 ────────────┐
│ resize → RGB888 64×64          │ ────────────────► │ coap_task (embassy-net UdpSocket) │
│ coap PUT /frame (+ Block1)     │ ◄── ACK / 2.04 ── │        │                            │
└────────────────────────────────┘                   │        ▼                            │
                                                     │  FrameInbox (embassy_sync)          │
                                                     │        │                            │
                                                     │        ▼                            │
                                                     │  display loop: status → frame swap  │
                                                     │  wifi_task: STA connect + reconnect│
                                                     │  net_task: embassy-net runner      │
                                                     │  mdns_task: makit.local A answers  │
                                                     └────────────────────────────────────┘
```

Task ownership:

| Task | Responsibility |
|------|----------------|
| `main` | Init clocks, heap, Hub75 **first**, spawn net, status wait, CoAP/mDNS, display loop |
| `net_task` | `embassy_net::Stack` runner |
| `wifi_task` | STA connect / wait disconnect / retry; updates `WifiPhase` |
| `coap_task` | Bind UDP 5683, parse CoAP, assemble Block1, publish frame |
| `mdns_task` | Answer `makit.local` A queries |
| display loop | Boot/wifi/dhcp text → IP idle → RGB frames |

Idle after DHCP: panel shows `makit.local` + IPv4 until first `PUT /frame`; then last frame stays until replaced.

## 1. Compile-time credentials from `.env`

### Files

| File | Role |
|------|------|
| `.env` | Local secrets — **gitignored** |
| `.env.example` | Documented keys only — committed |
| `build.rs` | Load `.env`, emit `cargo:rustc-env=...` |

### Keys (v1)

```bash
# .env.example
WIFI_SSID=your-ssid
WIFI_PASS=your-password
# Optional overrides (defaults in firmware if unset):
# COAP_PORT=5683
```

Hostname for mDNS is fixed in firmware as `makit` → `makit.local`.

### Mechanism

1. `build.rs` uses `dotenvy` (build-dependency) to load `.env` from the crate root when present.
2. Also honor already-exported shell env (CI / `export WIFI_SSID=...`).
3. Fail the build with a clear message if `WIFI_SSID` / `WIFI_PASS` are missing (empty password allowed only if explicitly set).
4. Firmware reads:

```rust
const WIFI_SSID: &str = env!("WIFI_SSID");
const WIFI_PASS: &str = env!("WIFI_PASS");
```

5. Add `.env` to [`.gitignore`](../.gitignore). Never commit real credentials.
6. Document in README: copy `.env.example` → `.env`, then `cargo run --release`.

### Notes

- Changing `.env` must rebuild — `build.rs` should `cargo:rerun-if-changed=.env`.
- Do **not** put secrets in `.cargo/config.toml` (easy to commit by mistake).

## 2. Network stack bring-up

### Dependencies (shipped)

- `esp-radio` 1.0.0-beta.1 (chip feature `esp32s3`)
- `embassy-net` with `udp`, `dhcpv4`, `medium-ethernet`, `multicast`
- `esp-alloc` heap (~96 KiB)
- `embassy-sync` — FrameInbox signal/mutex
- `coap-lite` — CoAP encode/decode
- `edge-mdns` + `edge-nal-embassy` — mDNS responder
- `smoltcp` with UDP + DHCP + multicast

### Init sequence

1. Heap (`esp-alloc`) ~96 KiB.
2. **Hub75 init + `boot` status on panel** (before DHCP wait).
3. Radio + STA; `embassy_net::Config::dhcpv4(...)`; spawn `net_task` + `wifi_task`.
4. Panel loop: `wifi` / `retry` / `dhcp` (+ truncated SSID) until IPv4; then `makit.local` + IP.
5. Spawn `coap_task` + `mdns_task`.

### IRAM / Hub75 risk

AGENTS.md: Wi‑Fi can fight Hub75 `iram` / cause flicker. Mitigation plan:

- First bring-up: Wi‑Fi + CoAP with status/IP on panel; watch for flicker/ghosting.
- Prefer keeping Hub75 features (`iram`, `circular-dma`, …) unchanged initially.
- If unstable: drop pixel clock to 10 MHz, or reduce Wi‑Fi duty (no continuous scan), or revisit IRAM placement after profiling.
- Avoid PSRAM for DMA framebuffers in v1.

## 3. CoAP PUT `/frame` (UDP)

### Endpoint

| Item | Value |
|------|-------|
| Port | `5683` (CoAP default), overridable via compile-time `COAP_PORT` |
| Path | `/frame` |
| Method | `PUT` |
| Success | `2.04 Changed` (or `2.31 Continue` during Block1) |
| Errors | `4.00` bad request, `4.05` method not allowed, `4.13` request entity too large |

### Payload (v1)

- Raw **RGB888**, length **exactly** `64 * 64 * 3 = 12288` bytes.
- Row-major: **`(y * 64 + x) * 3 + {R,G,B}`** with `x,y` in `0..64`.
- Optional later: `Content-Format` and query `?fmt=rgb565` for half size.

### Block1 (required for reliability)

12 KB does not fit one safe UDP datagram without IP fragmentation. Use **Block1**:

- Client sends successive blocks (SZX up to 1024).
- Server ACKs with Block1 echo; on last block (`M=0`) validate total length == 12288, then publish to display.
- Abort incomplete assembly on new PUT / timeout (5 s).

Implementation: `coap-lite` + `src/net/block1.rs` assembler.

### Other paths

| Path | Method | Behavior |
|------|--------|----------|
| `/.well-known/core` | GET | `</frame>;rt="makit64.frame";sz=12288` |
| `/frame` | PUT | RGB888 Block1 → display |
| anything else | — | `4.04 Not Found` |

### Frame handoff

`FrameInbox` in `src/frame.rs`: double buffer + `Signal`; CoAP publishes; display `wait_copy_into` then `draw_rgb888` + `swap`.

## 4. Display integration

1. `src/scene.rs`: `draw_rgb888`, `draw_status_lines`, `draw_net_status`.
2. Display loop:
   - Boot wait: `boot` / `wifi` / `retry` / `dhcp` text.
   - After DHCP, no frame yet → `makit.local` + IP.
   - On signal → draw RGB; hold until next PUT.
3. Pin map / Hub75 config untouched.

## 5. Host tooling (laptop)

`tools/put_frame.py`:

1. Open image, resize to 64×64 RGB888.
2. CoAP PUT with Block1 (aiocoap).
3. Default host: **`makit.local`** (`-H` / `--host` to override).

```bash
python tools/put_frame.py tools/tiger.png
python tools/put_frame.py -H 192.168.0.42 image.png
```

## 6. Module / file layout (current)

```
.env / .env.example
build.rs
src/
  lib.rs
  board.rs
  welcome.rs          # legacy R/G/B/Rainbow helpers
  scene.rs            # draw_rgb888 + status text
  frame.rs            # FrameInbox
  net/
    mod.rs
    wifi.rs           # STA + WifiPhase + spawn_network
    coap_server.rs
    block1.rs
    mdns.rs           # makit.local
src/bin/main.rs
tools/put_frame.py
docs/PLAN-wifi-coap.md
```

## 7. Implementation phases

### Phase A — Credentials + Wi‑Fi STA

- [x] `build.rs` + `.env.example` + gitignore
- [x] Heap + radio + `embassy-net` DHCP
- [x] Log `Wi‑Fi up, ip=...`
- [x] Hub75 alive during/after Wi‑Fi (status text; flicker soak on device)

### Phase B — CoAP skeleton

- [x] UDP bind `:5683`
- [x] `GET /.well-known/core`
- [x] `PUT /frame` rejects wrong size
- [x] CON ACK plumbing with `coap-lite`

### Phase C — Block1 + display

- [x] Block1 assembler → 12288 buffer
- [x] Channel to display; `draw_rgb888` + swap
- [x] Idle status (IP / hostname) until first frame
- [x] Host `put_frame.py` end-to-end

### Phase D — Harden

- [x] Reconnect Wi‑Fi cleanly (`wifi_task` loop)
- [x] Assembly timeout / busy response under load
- [x] Clippy + release build (device soak ongoing)
- [x] README / AGENTS.md update (Wi‑Fi, `.env`, CoAP usage)

### Phase E — Optional follow-ups

- [x] mDNS `makit.local` (A record via `edge-mdns`; panel shows IP + hostname)
- [x] On-panel boot / wifi / dhcp progress (no serial)
- [ ] DNS-SD `_coap._udp` service record
- [ ] RGB565 / query `fmt=`
- [ ] `GET /frame` thumbnail or hash
- [ ] SoftAP provisioning instead of compile-time SSID

## 8. Test plan

| Test | Pass criteria |
|------|----------------|
| Build without `.env` | Clear compile error |
| Build with `.env` | Links; boots; DHCP IP in log **and on panel** |
| Boot status text | Panel shows `boot` → `wifi`/`dhcp` before IP |
| Status + Wi‑Fi idle | No severe flicker / watchdog |
| `PUT` wrong size | `4.00` / `4.13`, panel unchanged |
| `PUT` full frame Block1 | Panel shows image; `2.04` |
| Second `PUT` | Replaces previous image |
| `makit.local` PUT | Resolves on LAN; same as IP |
| Wi‑Fi drop / restore | Reconnect; CoAP works again |
| Bright white frame | External 5V supply OK (HARDWARE.md) |

## 9. Resolved questions

1. Stack: `esp-radio` 1.0.0-beta.1 + `embassy-net` 0.9 (not legacy `esp-wifi`).
2. Block1: up to 1024-byte chunks (`SZX` ≤ 6).
3. Display loop stays in `main` (Hub75 ownership).
4. mDNS hostname: **`makit`** → `makit.local` (not `makit64`).

## 10. Doc updates

- [x] README: `.env`, flash, `put_frame.py`, CoAP URL, panel status
- [x] AGENTS.md: Wi‑Fi/CoAP/mDNS; IRAM caution
- [x] `.gitignore`: `.env`

---

**Status:** v1 complete. Remaining work is Phase E extras (DNS-SD, RGB565, SoftAP, …).
