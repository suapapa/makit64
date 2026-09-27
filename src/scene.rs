//! Draw helpers for network-delivered RGB888 frames and status text.

use core::fmt::Write;
use core::net::Ipv4Addr;

use embedded_graphics::geometry::Point;
use embedded_graphics::mono_font::ascii::{FONT_4X6, FONT_5X7};
use embedded_graphics::mono_font::MonoTextStyleBuilder;
use embedded_graphics::prelude::Drawable;
use embedded_graphics::text::{Alignment, Text};
use esp_hub75::Color;
use heapless::String;

use crate::board::{PANEL_HEIGHT, PANEL_WIDTH};
use crate::frame::FRAME_BYTES;
use crate::net::HOSTNAME;
use crate::rotate::RotatedFb;
use crate::welcome::FrameBuffer;

/// Paint a packed RGB888 buffer (`y`-major, then `x`, 3 bytes per pixel) onto `fb`.
pub fn draw_rgb888(fb: &mut FrameBuffer, rgb: &[u8; FRAME_BYTES]) {
    let mut fb = RotatedFb(fb);
    fb.erase();
    for y in 0..PANEL_HEIGHT {
        for x in 0..PANEL_WIDTH {
            let i = (y * PANEL_WIDTH + x) * 3;
            let color = Color::new(rgb[i], rgb[i + 1], rgb[i + 2]);
            fb.set_pixel(Point::new(x as i32, y as i32), color);
        }
    }
}

/// Two-line centered status (boot / wifi / dhcp / retry).
pub fn draw_status_lines(fb: &mut FrameBuffer, title: &str, detail: &str) {
    let mut fb = RotatedFb(fb);
    fb.erase();

    let title_style = MonoTextStyleBuilder::new()
        .font(&FONT_5X7)
        .text_color(Color::new(255, 200, 40))
        .build();
    let detail_style = MonoTextStyleBuilder::new()
        .font(&FONT_4X6)
        .text_color(Color::new(200, 200, 200))
        .build();

    let cx = (PANEL_WIDTH as i32) / 2;
    let _ = Text::with_alignment(title, Point::new(cx, 24), title_style, Alignment::Center)
        .draw(&mut fb);
    if !detail.is_empty() {
        let _ = Text::with_alignment(detail, Point::new(cx, 38), detail_style, Alignment::Center)
            .draw(&mut fb);
    }
}

/// Idle screen after DHCP: hostname + IPv4 until the first CoAP frame arrives.
pub fn draw_net_status(fb: &mut FrameBuffer, ipv4: Ipv4Addr) {
    let mut fb = RotatedFb(fb);
    fb.erase();

    let mut host: String<24> = String::new();
    let _ = write!(host, "{HOSTNAME}.local");

    let mut ip: String<16> = String::new();
    let _ = write!(ip, "{ipv4}");

    let host_style = MonoTextStyleBuilder::new()
        .font(&FONT_5X7)
        .text_color(Color::new(0, 220, 80))
        .build();
    let ip_style = MonoTextStyleBuilder::new()
        .font(&FONT_4X6)
        .text_color(Color::new(255, 255, 255))
        .build();
    let hint_style = MonoTextStyleBuilder::new()
        .font(&FONT_4X6)
        .text_color(Color::new(120, 120, 120))
        .build();

    let cx = (PANEL_WIDTH as i32) / 2;
    let _ = Text::with_alignment(host.as_str(), Point::new(cx, 18), host_style, Alignment::Center)
        .draw(&mut fb);
    let _ = Text::with_alignment(ip.as_str(), Point::new(cx, 32), ip_style, Alignment::Center)
        .draw(&mut fb);
    let _ =
        Text::with_alignment("PUT /frame", Point::new(cx, 46), hint_style, Alignment::Center)
            .draw(&mut fb);
}
