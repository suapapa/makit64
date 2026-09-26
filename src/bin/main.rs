#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use core::net::Ipv4Addr;

use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_net::Stack;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::Pin;
use esp_hal::ram;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_hub75::{Hub75, Hub75Config, Hub75Pins16};
use log::info;
use makit64::board::{self, PANEL_HEIGHT, PANEL_WIDTH};
use makit64::frame::{FrameInbox, FRAME_BYTES};
use makit64::net::{
    self, coap_task, mdns_task, wifi_ssid_short, WifiPhase, HOSTNAME,
};
use makit64::scene;
use makit64::welcome::FrameBuffer;
use static_cell::StaticCell;

macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write($val);
        x
    }};
}

esp_bootloader_esp_idf::esp_app_desc!();

/// Pixel clock — 20 MHz is the recommended ESP32-S3 starting point for 64×64.
const PIXEL_CLOCK: Rate = Rate::from_mhz(20);

static FRAME_INBOX: StaticCell<FrameInbox> = StaticCell::new();
static DISPLAY_RGB: StaticCell<[u8; FRAME_BYTES]> = StaticCell::new();

#[allow(
    clippy::large_stack_frames,
    reason = "framebuffers and Hub75 state live in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Wi-Fi / CoAP need a heap (`coap-lite`, `esp-radio`). Prefer bootloader-
    // reclaimed dram2 so the 64 KiB does not squeeze the main stack in dram_seg.
    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 32 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    info!(
        "makit64 — {}x{} HUB75E + Wi-Fi/CoAP (refresh ~{} Hz @ {} MHz)",
        PANEL_WIDTH,
        PANEL_HEIGHT,
        esp_hub75::refresh_hz::<FrameBuffer>(PIXEL_CLOCK),
        PIXEL_CLOCK.as_hz() / 1_000_000,
    );

    let inbox = FRAME_INBOX.init(FrameInbox::new());
    let rgb = DISPLAY_RGB.init([0u8; FRAME_BYTES]);

    let fb0 = mk_static!(FrameBuffer, FrameBuffer::new());
    let fb1 = mk_static!(FrameBuffer, FrameBuffer::new());

    let tx_descriptors = esp_hub75::hub75_dma_descriptors!(FrameBuffer);

    // Pin map from HARDWARE.md / sch/sch.png (LED_metrix_mcu_ver_rev.1).
    let pins = Hub75Pins16 {
        red1: peripherals.GPIO42.degrade(),
        grn1: peripherals.GPIO41.degrade(),
        blu1: peripherals.GPIO40.degrade(),
        red2: peripherals.GPIO38.degrade(),
        grn2: peripherals.GPIO39.degrade(),
        blu2: peripherals.GPIO37.degrade(),
        addr0: peripherals.GPIO45.degrade(),
        addr1: peripherals.GPIO36.degrade(),
        addr2: peripherals.GPIO48.degrade(),
        addr3: peripherals.GPIO35.degrade(),
        addr4: peripherals.GPIO21.degrade(),
        blank: peripherals.GPIO14.degrade(),
        clock: peripherals.GPIO2.degrade(),
        latch: peripherals.GPIO47.degrade(),
    };

    let mut hub75 = Hub75::new_async(
        peripherals.LCD_CAM,
        pins,
        peripherals.DMA_CH0,
        tx_descriptors,
        Hub75Config::new().with_frequency(PIXEL_CLOCK),
        &*fb0,
    )
    .expect("Hub75 init failed");

    let _ = board::ui::LED_D13;

    // Show progress on the panel before (and during) DHCP — no serial needed.
    let mut fb: &'static mut FrameBuffer = fb1;
    scene::draw_status_lines(fb, "boot", "makit64");
    fb = swap_fb(&mut hub75, fb).await;

    let stack = net::spawn_network(&spawner, peripherals.WIFI);
    let (hub75, fb, ipv4) = wait_dhcp_with_status(hub75, fb, stack).await;

    spawner
        .spawn(coap_task(stack, inbox).expect("spawn coap_task"));
    spawner
        .spawn(mdns_task(stack, ipv4).expect("spawn mdns_task"));

    run_display(hub75, fb, inbox, rgb, ipv4).await
}

async fn swap_fb(
    hub75: &mut Hub75<esp_hal::Async, FrameBuffer>,
    fb: &'static mut FrameBuffer,
) -> &'static mut FrameBuffer {
    let mut xfer = hub75.swap(fb).expect("swap");
    xfer.wait_for_done().await;
    xfer.wait().expect("dma")
}

/// Redraw Wi‑Fi / DHCP phase on the panel until an IPv4 address appears.
#[allow(
    clippy::large_stack_frames,
    reason = "Hub75 + FB live across the wait loop"
)]
async fn wait_dhcp_with_status(
    mut hub75: Hub75<esp_hal::Async, FrameBuffer>,
    mut fb: &'static mut FrameBuffer,
    stack: Stack<'static>,
) -> (
    Hub75<esp_hal::Async, FrameBuffer>,
    &'static mut FrameBuffer,
    Ipv4Addr,
) {
    let ssid = wifi_ssid_short();
    let mut last_title = "";

    loop {
        if let Some(cfg) = stack.config_v4() {
            let ipv4 = cfg.address.address();
            info!("wifi: up, ip={ipv4} hostname={HOSTNAME}.local");
            scene::draw_net_status(fb, ipv4);
            fb = swap_fb(&mut hub75, fb).await;
            return (hub75, fb, ipv4);
        }

        let (title, detail) = match WifiPhase::load() {
            WifiPhase::Starting => ("wifi", ssid),
            WifiPhase::Connecting => ("wifi", ssid),
            WifiPhase::Retrying => ("retry", ssid),
            WifiPhase::Connected => {
                if stack.is_link_up() {
                    ("dhcp", ssid)
                } else {
                    ("wifi", ssid)
                }
            }
        };

        if title != last_title {
            info!("display: status {title} ({detail})");
            last_title = title;
        }
        scene::draw_status_lines(fb, title, detail);
        fb = swap_fb(&mut hub75, fb).await;

        Timer::after(Duration::from_millis(400)).await;
    }
}

#[allow(
    clippy::large_stack_frames,
    reason = "async state machine for Hub75 swap + select"
)]
async fn run_display(
    mut hub75: Hub75<esp_hal::Async, FrameBuffer>,
    mut fb: &'static mut FrameBuffer,
    inbox: &'static FrameInbox,
    rgb: &'static mut [u8; FRAME_BYTES],
    ipv4: Ipv4Addr,
) -> ! {
    let mut showing_frame = false;

    info!("display: showing {HOSTNAME}.local / {ipv4} until first CoAP PUT /frame");
    scene::draw_net_status(fb, ipv4);
    fb = swap_fb(&mut hub75, fb).await;

    loop {
        if showing_frame {
            match select(inbox.wait_copy_into(rgb), Timer::after(Duration::from_secs(30)))
                .await
            {
                Either::First(()) | Either::Second(()) => {
                    scene::draw_rgb888(fb, rgb);
                    fb = swap_fb(&mut hub75, fb).await;
                }
            }
            continue;
        }

        match select(
            inbox.wait_copy_into(rgb),
            Timer::after(Duration::from_secs(5)),
        )
        .await
        {
            Either::First(()) => {
                info!("display: network frame — leaving status");
                showing_frame = true;
                scene::draw_rgb888(fb, rgb);
                fb = swap_fb(&mut hub75, fb).await;
            }
            Either::Second(()) => {
                scene::draw_net_status(fb, ipv4);
                fb = swap_fb(&mut hub75, fb).await;
            }
        }
    }
}
