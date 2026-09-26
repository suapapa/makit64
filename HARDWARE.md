# Hardware — LED_metrix_mcu_ver_rev.1

Source schematic: [`sch/sch.png`](sch/sch.png) (EasyEDA, 2026-08-03).

ESP32-S3 board that drives a **HUB75E 64×64** (1/32 scan) LED matrix through **3.3V → 5V** level shifting.

## Block overview

| Block | Parts | Role |
| --- | --- | --- |
| MCU | ESP32-S3 module (`U1`) | HUB75 timing, UI, USB |
| Level shifters | 2× 74HC245 (`U3`, `U4`) | Unidirectional 3.3V→5V buffers |
| Panel connector | HUB75 2×8 header `H1` (`SFH11-PBPC-D08-ST-BK`) | Matrix interface |
| 3.3V rail | AMS1117-3.3 (`U2`) | 5V → 3.3V LDO for MCU |
| Power in | USB-C (`USB2`) + SMT pads `U5`/`U6` | 5V / GND high-current feed |
| UI | BOOT, RESET, USER1, USER2 | Active-low buttons |
| Status | `test_1` / `test_2`, 3.3V / 5V LEDs | GPIO + power indicators |

### Level shifters (`U3`, `U4`)

- VCC = **+5V**, OE# = **GND**, DIR = **+5V** → A→B (MCU → panel)
- MCU nets use `_3V` suffix; panel nets use `_5V`
- Address **E** is split to `H1` pin 8 (`E_1_5V`) and pin 16 (`E_2_5V`) via jumpers `R3`–`R6` (typical for 64×64 / 1:32 panels)

## HUB75 pin map (ESP32-S3 → panel)

| HUB75 | GPIO | Net (3.3V) | Notes |
| --- | ---: | --- | --- |
| R1 | **42** | `R1_3V` | Top-half red |
| G1 | **41** | `G1_3V` | Top-half green |
| B1 | **40** | `B1_3V` | Top-half blue |
| R2 | **38** | `R2_3V` | Bottom-half red |
| G2 | **39** | `G2_3V` | Bottom-half green |
| B2 | **37** | `B2_3V` | Bottom-half blue |
| A | **45** | `A_3V` | Row address |
| B | **36** | `B_3V` | Row address |
| C | **48** | `C_3V` | Row address |
| D | **35** | `D_3V` | Row address |
| E | **21** | `E_3V` | Row address (64×64) |
| CLK | **2** | `CLK_3V` | Shift clock |
| LAT | **47** | `LAT_3V` | Latch / STB |
| OE | **14** | `OE_3V` | Output enable (**active low**) |

Firmware constants live in [`src/board.rs`](src/board.rs).

### HUB75 connector `H1` (5V side)

| Pin | Signal | Pin | Signal |
| ---: | --- | ---: | --- |
| 1 | R1 | 2 | G1 |
| 3 | B1 | 4 | GND |
| 5 | R2 | 6 | G2 |
| 7 | B2 | 8 | E (`E_1_5V`) |
| 9 | A | 10 | B |
| 11 | C | 12 | D |
| 13 | CLK | 14 | LAT |
| 15 | OE | 16 | E (`E_2_5V`) / GND option |

## On-board peripherals

| Function | GPIO / pin | Behavior |
| --- | --- | --- |
| BOOT | IO0 | Active low, strap / boot |
| RESET | EN | Active low |
| USER1 | IO6 | Active low, 10k pull-up + debounce cap |
| USER2 | IO7 | Active low, 10k pull-up + debounce cap |
| `test_1` (D13) | IO13 | Status LED |
| `test_2` (D12) | IO12 | Status LED |
| USB D− | IO19 | Native USB |
| USB D+ | IO20 | Native USB |

## Power

- **+5V**: USB-C VBUS and/or `U5`/`U6` terminals (panel + level-shifter VCC). Bulk `C5` 100µF on 5V.
- **3.3V**: AMS1117-3.3 from 5V; decoupling 1µF / 100nF at regulator.
- Panel LED current should come from the 5V terminals, not only USB-C, when the matrix is brightly driven.

## Firmware target

- Chip: **ESP32-S3**
- Runtime: `esp-hal` + Embassy (`no_std`)
- Panel: **64×64**, HUB75E, **1/32** scan (needs address line **E**)
