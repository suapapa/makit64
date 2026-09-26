#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use embassy_executor::Spawner;
use embassy_time::{Duration, Instant, Timer};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::Pin;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_hub75::{Hub75, Hub75Config, Hub75Pins16};
use log::info;
use makit64::board::{self, PANEL_HEIGHT, PANEL_WIDTH};
use makit64::welcome::{self, FrameBuffer, WelcomePhase};

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

#[allow(
    clippy::large_stack_frames,
    reason = "framebuffers and Hub75 state live in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    info!(
        "makit64 — {}x{} HUB75E welcome (refresh ~{} Hz @ {} MHz)",
        PANEL_WIDTH,
        PANEL_HEIGHT,
        esp_hub75::refresh_hz::<FrameBuffer>(PIXEL_CLOCK),
        PIXEL_CLOCK.as_hz() / 1_000_000,
    );

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

    let hub75 = Hub75::new_async(
        peripherals.LCD_CAM,
        pins,
        peripherals.DMA_CH0,
        tx_descriptors,
        Hub75Config::new().with_frequency(PIXEL_CLOCK),
        &*fb0,
    )
    .expect("Hub75 init failed");

    let _ = spawner;
    let _ = board::ui::LED_D13;

    run_welcome(hub75, fb1).await
}

async fn run_welcome(
    hub75: Hub75<esp_hal::Async, FrameBuffer>,
    mut fb: &'static mut FrameBuffer,
) -> ! {
    let mut phase = WelcomePhase::Red;
    let mut phase_started = Instant::now();
    let mut hue: u8 = 0;

    info!("welcome: {}", phase.name());
    welcome::draw_welcome(fb, phase, hue);
    let mut xfer = hub75.swap(fb).expect("swap");
    xfer.wait_for_done().await;
    fb = xfer.wait().expect("dma");

    loop {
        if phase_started.elapsed() >= WelcomePhase::HOLD {
            phase = phase.next();
            phase_started = Instant::now();
            info!("welcome: {}", phase.name());
        }

        if phase == WelcomePhase::Rainbow {
            hue = hue.wrapping_add(3);
        }

        welcome::draw_welcome(fb, phase, hue);
        let mut xfer = hub75.swap(fb).expect("swap");
        xfer.wait_for_done().await;
        fb = xfer.wait().expect("dma");

        // Solid colours only need occasional updates; rainbow wants a steady
        // but not max-rate redraw so the hue scroll is visible.
        let delay = if phase == WelcomePhase::Rainbow {
            Duration::from_millis(30)
        } else {
            Duration::from_millis(100)
        };
        Timer::after(delay).await;
    }
}
