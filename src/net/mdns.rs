//! mDNS responder — advertise `makit.local` → this device's IPv4.

use core::net::{Ipv4Addr, Ipv6Addr};

use edge_mdns::buf::VecBufAccess;
use edge_mdns::domain::base::Ttl;
use edge_mdns::host::Host;
use edge_mdns::io::{self, IPV4_DEFAULT_SOCKET};
use edge_mdns::HostAnswersMdnsHandler;
use edge_nal::UdpSplit;
use edge_nal_embassy::{Udp, UdpBuffers};
use embassy_futures::select::{select, Either};
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use esp_hal::rng::Rng;
use log::{error, info, warn};
use static_cell::StaticCell;

/// Hostname label advertised as `{HOSTNAME}.local`.
pub const HOSTNAME: &str = "makit";

/// `Pool` is intentionally `!Sync`; we only touch it from `mdns_task` on one core.
struct SyncUdpBuffers(UdpBuffers<1, 1500, 1500, 4>);

// SAFETY: single-core ESP32-S3; exclusive use by `mdns_task`.
unsafe impl Sync for SyncUdpBuffers {}

static UDP_BUFFERS: StaticCell<SyncUdpBuffers> = StaticCell::new();
static RECV_BUF: StaticCell<VecBufAccess<CriticalSectionRawMutex, 1500>> = StaticCell::new();
static SEND_BUF: StaticCell<VecBufAccess<CriticalSectionRawMutex, 1500>> = StaticCell::new();
static BROADCAST_SIGNAL: StaticCell<Signal<CriticalSectionRawMutex, ()>> = StaticCell::new();

#[embassy_executor::task]
pub async fn mdns_task(stack: Stack<'static>) -> ! {
    let buffers = UDP_BUFFERS.init(SyncUdpBuffers(UdpBuffers::new()));
    let recv_buf = RECV_BUF.init(VecBufAccess::new());
    let send_buf = SEND_BUF.init(VecBufAccess::new());
    let broadcast_signal = BROADCAST_SIGNAL.init(Signal::new());

    let mut last_logged_ip: Option<Ipv4Addr> = None;

    loop {
        stack.wait_config_up().await;
        let Some(cfg) = stack.config_v4() else {
            Timer::after(Duration::from_millis(200)).await;
            continue;
        };
        let ipv4 = cfg.address.address();

        let host = Host {
            hostname: HOSTNAME,
            ipv4,
            ipv6: Ipv6Addr::UNSPECIFIED,
            ttl: Ttl::from_secs(60),
        };

        if last_logged_ip != Some(ipv4) {
            info!("mdns: answering as {HOSTNAME}.local → {ipv4}");
            last_logged_ip = Some(ipv4);
        }

        let udp = Udp::new(stack, &buffers.0);
        let mut socket = match io::bind(&udp, IPV4_DEFAULT_SOCKET, Some(ipv4), None).await {
            Ok(s) => s,
            Err(e) => {
                error!("mdns: bind/join failed: {e}");
                Timer::after(Duration::from_secs(3)).await;
                continue;
            }
        };

        broadcast_signal.signal(());

        let (recv, send) = socket.split();
        let mdns = io::Mdns::new(
            Some(ipv4),
            None,
            recv,
            send,
            &*recv_buf,
            &*send_buf,
            Rng::new(),
            broadcast_signal,
        );

        match select(
            mdns.run(HostAnswersMdnsHandler::new(&host)),
            wait_ip_changed(stack, ipv4),
        )
        .await
        {
            Either::First(Ok(())) => warn!("mdns: responder exited"),
            Either::First(Err(e)) => error!("mdns: run error: {e}"),
            Either::Second(()) => {
                info!("mdns: IPv4 config changed or dropped — rebinding");
                continue;
            }
        }
        Timer::after(Duration::from_secs(2)).await;
    }
}

async fn wait_ip_changed(stack: Stack<'static>, current_ip: Ipv4Addr) {
    loop {
        Timer::after(Duration::from_secs(2)).await;
        let live_ip = stack.config_v4().map(|c| c.address.address());
        if live_ip != Some(current_ip) {
            return;
        }
    }
}
