//! Wi-Fi STA + CoAP UDP server + mDNS.

pub mod block1;
pub mod coap_server;
pub mod mdns;
pub mod wifi;

pub use coap_server::coap_task;
pub use mdns::{mdns_task, HOSTNAME};
pub use wifi::{net_task, spawn_network, wifi_ssid_short, wifi_task, WifiPhase};
