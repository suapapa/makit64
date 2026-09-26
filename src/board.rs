//! Pin map for LED_metrix_mcu_ver_rev.1 — see `HARDWARE.md` / `sch/sch.png`.

/// HUB75E 64×64 panel geometry.
pub const PANEL_WIDTH: usize = 64;
pub const PANEL_HEIGHT: usize = 64;
/// 1/32 scan → five address lines (A–E).
pub const SCAN_LINES: usize = 32;

/// HUB75 GPIO numbers (ESP32-S3).
pub mod hub75 {
    pub const R1: u8 = 42;
    pub const G1: u8 = 41;
    pub const B1: u8 = 40;
    pub const R2: u8 = 38;
    pub const G2: u8 = 39;
    pub const B2: u8 = 37;
    pub const A: u8 = 45;
    pub const B: u8 = 36;
    pub const C: u8 = 48;
    pub const D: u8 = 35;
    pub const E: u8 = 21;
    pub const CLK: u8 = 2;
    pub const LAT: u8 = 47;
    /// Active low.
    pub const OE: u8 = 14;
}

/// On-board buttons and status LEDs.
pub mod ui {
    pub const BOOT: u8 = 0;
    pub const USER1: u8 = 6;
    pub const USER2: u8 = 7;
    /// `test_2` on schematic.
    pub const LED_D12: u8 = 12;
    /// `test_1` on schematic.
    pub const LED_D13: u8 = 13;
}

/// Native USB pins on the ESP32-S3.
pub mod usb {
    pub const D_N: u8 = 19;
    pub const D_P: u8 = 20;
}
