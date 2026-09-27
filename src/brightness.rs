//! Shared panel brightness (0–255) between CoAP and the display loop.

use core::sync::atomic::{AtomicU8, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;

/// Full brightness at boot (no dimming until `PUT /brightness`).
pub const DEFAULT: u8 = 255;

/// Runtime brightness level + change notification for redraws.
pub struct Brightness {
    level: AtomicU8,
    changed: Signal<CriticalSectionRawMutex, ()>,
}

impl Default for Brightness {
    fn default() -> Self {
        Self::new(DEFAULT)
    }
}

impl Brightness {
    pub const fn new(level: u8) -> Self {
        Self {
            level: AtomicU8::new(level),
            changed: Signal::new(),
        }
    }

    pub fn get(&self) -> u8 {
        self.level.load(Ordering::Acquire)
    }

    /// Store a new level (0–255) and wake the display loop.
    pub fn set(&self, level: u8) {
        self.level.store(level, Ordering::Release);
        self.changed.signal(());
    }

    /// Wait until [`Self::set`] is called.
    pub async fn wait_changed(&self) {
        self.changed.wait().await;
    }
}

/// Scale one RGB channel by `brightness` (`out = c * brightness / 255`).
#[inline]
pub fn scale_channel(c: u8, brightness: u8) -> u8 {
    ((u16::from(c) * u16::from(brightness)) / 255) as u8
}
