//! Compile-time panel rotation (`DISPLAY_ROTATION` from `.env`).

use core::convert::Infallible;

use embedded_graphics::geometry::{Point, Size};
use embedded_graphics::prelude::{DrawTarget, OriginDimensions};
use embedded_graphics::Pixel;
use esp_hub75::Color;

use crate::board::{PANEL_HEIGHT, PANEL_WIDTH};
use crate::welcome::FrameBuffer;

/// Clockwise rotation in degrees: `0`, `90`, `180`, or `270`.
pub const DEGREES: u16 = parse_degrees(env!("DISPLAY_ROTATION"));

const fn parse_degrees(s: &str) -> u16 {
    let bytes = s.as_bytes();
    let mut n: u16 = 0;
    let mut i = 0;
    while i < bytes.len() {
        let d = bytes[i];
        assert!(d.is_ascii_digit(), "DISPLAY_ROTATION must be digits");
        n = n * 10 + (d - b'0') as u16;
        i += 1;
    }
    match n {
        0 | 90 | 180 | 270 => n,
        _ => panic!("DISPLAY_ROTATION must be 0, 90, 180, or 270 (clockwise)"),
    }
}

/// Map a logical (content) point to panel framebuffer coordinates.
#[inline]
pub fn map_point(p: Point) -> Point {
    let w = PANEL_WIDTH as i32;
    let h = PANEL_HEIGHT as i32;
    match DEGREES {
        0 => p,
        90 => Point::new(w - 1 - p.y, p.x),
        180 => Point::new(w - 1 - p.x, h - 1 - p.y),
        270 => Point::new(p.y, h - 1 - p.x),
        _ => p,
    }
}

/// DrawTarget that applies [`map_point`] before writing the HUB75 framebuffer.
pub struct RotatedFb<'a>(pub &'a mut FrameBuffer);

impl RotatedFb<'_> {
    pub fn erase(&mut self) {
        self.0.erase();
    }

    pub fn set_pixel(&mut self, p: Point, color: Color) {
        self.0.set_pixel(map_point(p), color);
    }
}

impl DrawTarget for RotatedFb<'_> {
    type Color = Color;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(p, c) in pixels {
            if p.x < 0 || p.y < 0 {
                continue;
            }
            self.set_pixel(p, c);
        }
        Ok(())
    }
}

impl OriginDimensions for RotatedFb<'_> {
    fn size(&self) -> Size {
        Size::new(PANEL_WIDTH as u32, PANEL_HEIGHT as u32)
    }
}
