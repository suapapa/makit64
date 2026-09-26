//! Welcome-screen helpers for the 64×64 HUB75 panel.

use embedded_graphics::geometry::Point;
use esp_hub75::Color;
use esp_hub75::framebuffer::bitplane::plain::DmaFrameBuffer;

use crate::board::{PANEL_HEIGHT, PANEL_WIDTH, SCAN_LINES};

pub const PLANES: usize = 6;
pub type FrameBuffer = DmaFrameBuffer<SCAN_LINES, PANEL_WIDTH, PLANES>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WelcomePhase {
    Red,
    Green,
    Blue,
    Rainbow,
}

impl WelcomePhase {
    pub const SEQUENCE: [Self; 4] = [Self::Red, Self::Green, Self::Blue, Self::Rainbow];

    /// How long to hold each solid colour / rainbow segment.
    pub const HOLD: embassy_time::Duration = embassy_time::Duration::from_millis(1200);

    pub fn next(self) -> Self {
        match self {
            Self::Red => Self::Green,
            Self::Green => Self::Blue,
            Self::Blue => Self::Rainbow,
            Self::Rainbow => Self::Red,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Rainbow => "rainbow",
        }
    }
}

/// Fill the framebuffer for the current welcome phase.
///
/// `hue_offset` advances the rainbow animation (0..=255).
pub fn draw_welcome(fb: &mut FrameBuffer, phase: WelcomePhase, hue_offset: u8) {
    fb.erase();
    match phase {
        WelcomePhase::Red => fill_solid(fb, Color::new(255, 0, 0)),
        WelcomePhase::Green => fill_solid(fb, Color::new(0, 255, 0)),
        WelcomePhase::Blue => fill_solid(fb, Color::new(0, 0, 255)),
        WelcomePhase::Rainbow => fill_rainbow(fb, hue_offset),
    }
}

fn fill_solid(fb: &mut FrameBuffer, color: Color) {
    for y in 0..PANEL_HEIGHT as i32 {
        for x in 0..PANEL_WIDTH as i32 {
            fb.set_pixel(Point::new(x, y), color);
        }
    }
}

fn fill_rainbow(fb: &mut FrameBuffer, hue_offset: u8) {
    for y in 0..PANEL_HEIGHT as i32 {
        for x in 0..PANEL_WIDTH as i32 {
            // Diagonal hue sweep so both axes feel alive.
            let hue = hue_offset
                .wrapping_add((x * 4) as u8)
                .wrapping_add((y * 2) as u8);
            fb.set_pixel(Point::new(x, y), hsv_to_rgb(hue, 255, 255));
        }
    }
}

/// Compact HSV→RGB for full-saturation rainbows (`s`/`v` in 0..=255).
fn hsv_to_rgb(h: u8, s: u8, v: u8) -> Color {
    if s == 0 {
        return Color::new(v, v, v);
    }

    let region = h / 43;
    let remainder = (h - region * 43) as u16 * 6;

    let p = ((v as u16 * (255 - s as u16)) / 255) as u8;
    let q = ((v as u16 * (255 - (s as u16 * remainder) / 255)) / 255) as u8;
    let t = ((v as u16 * (255 - (s as u16 * (255 - remainder)) / 255)) / 255) as u8;

    match region {
        0 => Color::new(v, t, p),
        1 => Color::new(q, v, p),
        2 => Color::new(p, v, t),
        3 => Color::new(p, q, v),
        4 => Color::new(t, p, v),
        _ => Color::new(v, p, q),
    }
}
