//! Wi-Fi station bring-up (credentials from compile-time `.env`).

use core::sync::atomic::{AtomicU8, Ordering};

use embassy_executor::Spawner;
use embassy_net::{Config, Runner, Stack, StackResources};
use embassy_time::{Duration, Instant, Timer};
use esp_hal::peripherals::WIFI;
use esp_hal::rng::Rng;
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config as WifiConfig, Interface, WifiController,
};
use esp_radio::wifi::sta::StationConfig;
use log::{error, info};
use static_cell::StaticCell;

const WIFI_SSID: &str = env!("WIFI_SSID");
const WIFI_PASS: &str = env!("WIFI_PASS");

/// DHCP + CoAP + mDNS sockets (plus headroom).
static STACK_RESOURCES: StaticCell<StackResources<8>> = StaticCell::new();

/// Shared STA progress for the panel (updated by `wifi_task`).
static WIFI_PHASE: AtomicU8 = AtomicU8::new(WifiPhase::Starting as u8);

/// STA association progress (panel / serial without logs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WifiPhase {
    Starting = 0,
    Connecting = 1,
    Connected = 2,
    Retrying = 3,
}

impl WifiPhase {
    pub fn load() -> Self {
        match WIFI_PHASE.load(Ordering::Acquire) {
            1 => Self::Connecting,
            2 => Self::Connected,
            3 => Self::Retrying,
            _ => Self::Starting,
        }
    }

    fn store(self) {
        WIFI_PHASE.store(self as u8, Ordering::Release);
    }
}

/// Compile-time SSID (for status detail line).
pub fn wifi_ssid() -> &'static str {
    WIFI_SSID
}

/// Truncate SSID so it fits a 64px FONT_4X6 row (~12 glyphs).
pub fn wifi_ssid_short() -> &'static str {
    const MAX: usize = 12;
    let s = WIFI_SSID;
    if s.len() <= MAX {
        return s;
    }
    let mut end = MAX;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Spawn Wi-Fi STA + `embassy-net` runner; returns immediately (DHCP may still be pending).
pub fn spawn_network(spawner: &Spawner, wifi: WIFI<'static>) -> Stack<'static> {
    WifiPhase::Starting.store();

    let wifi_controller =
        WifiController::new(wifi, Default::default()).expect("Wi-Fi init failed");
    let station_iface = Interface::station();

    let net_config = Config::dhcpv4(Default::default());
    let rng = Rng::new();
    let seed = (rng.random() as u64) << 32 | (rng.random() as u64);
    let seed = seed ^ Instant::now().as_ticks();

    let (stack, runner) = embassy_net::new(
        station_iface,
        net_config,
        STACK_RESOURCES.init(StackResources::new()),
        seed,
    );

    spawner.spawn(net_task(runner).expect("spawn net_task"));
    spawner.spawn(wifi_task(wifi_controller).expect("spawn wifi_task"));

    info!("wifi: spawned (ssid={WIFI_SSID}) — waiting for DHCP…");
    stack
}

#[embassy_executor::task]
pub async fn net_task(mut runner: Runner<'static, Interface>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
pub async fn wifi_task(mut controller: WifiController<'static>) {
    let auth = if WIFI_PASS.is_empty() {
        AuthenticationMethodConfig::Open
    } else {
        AuthenticationMethodConfig::Wpa2Personal(
            WIFI_PASS
                .try_into()
                .expect("WIFI_PASS must be a valid Wi-Fi password length"),
        )
    };

    let station = StationConfig::default()
        .with_ssid(
            WIFI_SSID
                .try_into()
                .expect("WIFI_SSID must be a valid SSID length"),
        )
        .with_authentication(auth);
    let config = WifiConfig::Station(station);

    if let Err(e) = controller.set_config(&config) {
        error!("wifi: set_config failed: {e:?}");
        WifiPhase::Retrying.store();
    }

    loop {
        WifiPhase::Connecting.store();
        match controller.connect_async().await {
            Ok(info) => {
                WifiPhase::Connected.store();
                info!(
                    "wifi: connected ssid={:?} channel={}",
                    info.ssid, info.channel
                );
            }
            Err(e) => {
                WifiPhase::Retrying.store();
                error!("wifi: connect failed: {e:?}");
                Timer::after(Duration::from_secs(5)).await;
                continue;
            }
        }

        let _ = controller.wait_for_disconnect_async().await;
        WifiPhase::Retrying.store();
        info!("wifi: disconnected — reconnecting");
        Timer::after(Duration::from_millis(500)).await;
    }
}
