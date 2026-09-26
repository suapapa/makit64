//! Shared RGB888 frame inbox between CoAP and the display loop.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;

use crate::board::{PANEL_HEIGHT, PANEL_WIDTH};

/// Packed RGB888 frame size for the 64×64 panel.
pub const FRAME_BYTES: usize = PANEL_WIDTH * PANEL_HEIGHT * 3;

/// Double-buffered RGB888 inbox. CoAP writes the free slot; display reads the
/// committed slot after [`FRAME_SIGNAL`].
pub struct FrameInbox {
    bufs: Mutex<CriticalSectionRawMutex, [[u8; FRAME_BYTES]; 2]>,
    /// Index of the buffer that display should read (0 or 1).
    published: AtomicU32,
    has_frame: AtomicBool,
    signal: Signal<CriticalSectionRawMutex, ()>,
}

impl Default for FrameInbox {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameInbox {
    pub const fn new() -> Self {
        Self {
            bufs: Mutex::new([[0; FRAME_BYTES]; 2]),
            published: AtomicU32::new(0),
            has_frame: AtomicBool::new(false),
            signal: Signal::new(),
        }
    }

    /// Copy a complete RGB888 frame into the free slot and notify display.
    pub async fn publish(&self, rgb: &[u8; FRAME_BYTES]) {
        let published = self.published.load(Ordering::Acquire) as usize;
        let write = 1 - published;
        {
            let mut bufs = self.bufs.lock().await;
            bufs[write].copy_from_slice(rgb);
        }
        self.published.store(write as u32, Ordering::Release);
        self.has_frame.store(true, Ordering::Release);
        self.signal.signal(());
    }

    pub fn has_frame(&self) -> bool {
        self.has_frame.load(Ordering::Acquire)
    }

    /// Wait until a (new) frame is available, then copy it into `out`.
    pub async fn wait_copy_into(&self, out: &mut [u8; FRAME_BYTES]) {
        self.signal.wait().await;
        let idx = self.published.load(Ordering::Acquire) as usize;
        let bufs = self.bufs.lock().await;
        out.copy_from_slice(&bufs[idx]);
    }

    /// Non-blocking copy of the latest published frame.
    pub async fn try_copy_into(&self, out: &mut [u8; FRAME_BYTES]) -> bool {
        if !self.has_frame() {
            return false;
        }
        let idx = self.published.load(Ordering::Acquire) as usize;
        let bufs = self.bufs.lock().await;
        out.copy_from_slice(&bufs[idx]);
        true
    }

    /// Clear the pending signal so the next `wait_copy_into` waits for a new PUT.
    pub fn clear_signal(&self) {
        self.signal.reset();
    }
}
