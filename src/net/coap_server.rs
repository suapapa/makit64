//! CoAP UDP server: `PUT /frame` (RGB888 + Block1), `GET|PUT /brightness`.

use alloc::string::String;
use alloc::vec::Vec;
use core::net::SocketAddr;

use coap_lite::{
    CoapOption, CoapRequest, MessageClass, Packet, RequestType, ResponseType,
};
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::Stack;
use embassy_time::{Duration, Instant};
use log::{info, warn};

use crate::brightness::Brightness;
use crate::frame::{FrameInbox, FRAME_BYTES};
use crate::net::block1::BlockValue;

const COAP_PORT: u16 = {
    let s = env!("COAP_PORT");
    parse_u16(s)
};

const fn parse_u16(s: &str) -> u16 {
    let bytes = s.as_bytes();
    let mut n: u16 = 0;
    let mut i = 0;
    while i < bytes.len() {
        let d = bytes[i];
        assert!(d.is_ascii_digit(), "COAP_PORT must be digits");
        n = n * 10 + (d - b'0') as u16;
        i += 1;
    }
    n
}

/// Max Block1 payload we accept per datagram (SZX ≤ 6 → 1024).
const MAX_BLOCK_PAYLOAD: usize = 1024;

struct Block1Assembler {
    buf: &'static mut [u8; FRAME_BYTES],
    filled: usize,
    /// Expected next block number, or `None` if idle.
    next_num: Option<u16>,
    last_activity: Option<Instant>,
}

impl Block1Assembler {
    fn new(buf: &'static mut [u8; FRAME_BYTES]) -> Self {
        Self {
            buf,
            filled: 0,
            next_num: None,
            last_activity: None,
        }
    }

    fn reset(&mut self) {
        self.filled = 0;
        self.next_num = None;
        self.last_activity = None;
    }

    fn maybe_timeout(&mut self) {
        if self.next_num.is_some()
            && self
                .last_activity
                .is_some_and(|t| t.elapsed() > Duration::from_secs(5))
        {
            warn!("coap: Block1 assembly timed out — reset");
            self.reset();
        }
    }
}

static ASSEMBLE_BUF: static_cell::StaticCell<[u8; FRAME_BYTES]> = static_cell::StaticCell::new();
static PUBLISH_BUF: static_cell::StaticCell<[u8; FRAME_BYTES]> = static_cell::StaticCell::new();

#[embassy_executor::task]
pub async fn coap_task(
    stack: Stack<'static>,
    inbox: &'static FrameInbox,
    brightness: &'static Brightness,
) -> ! {
    let mut rx_meta = [PacketMetadata::EMPTY; 4];
    let mut rx_buffer = [0u8; 1536];
    let mut tx_meta = [PacketMetadata::EMPTY; 4];
    let mut tx_buffer = [0u8; 1536];

    let mut socket = UdpSocket::new(
        stack,
        &mut rx_meta,
        &mut rx_buffer,
        &mut tx_meta,
        &mut tx_buffer,
    );
    socket.bind(COAP_PORT).expect("CoAP bind");
    info!("coap: listening on UDP :{COAP_PORT}  PUT /frame  GET|PUT /brightness");

    let assemble = ASSEMBLE_BUF.init([0u8; FRAME_BYTES]);
    let publish = PUBLISH_BUF.init([0u8; FRAME_BYTES]);
    let mut assembler = Block1Assembler::new(assemble);
    let mut pkt_buf = [0u8; 1536];

    loop {
        let (len, meta) = match socket.recv_from(&mut pkt_buf).await {
            Ok(v) => v,
            Err(e) => {
                warn!("coap: recv error: {e:?}");
                continue;
            }
        };
        let src = meta.endpoint;
        assembler.maybe_timeout();

        let packet = match Packet::from_bytes(&pkt_buf[..len]) {
            Ok(p) => p,
            Err(e) => {
                warn!("coap: bad packet from {src}: {e:?}");
                continue;
            }
        };

        let endpoint = SocketAddr::from((core::net::Ipv4Addr::UNSPECIFIED, 0));
        let mut request = CoapRequest::from_packet(packet, endpoint);
        let path = normalize_path(request.get_path());
        let method = *request.get_method();

        let reply = match (method, path.as_str()) {
            (RequestType::Get, ".well-known/core") => handle_well_known(&mut request),
            (RequestType::Put, "frame") => {
                handle_put_frame(&mut request, &mut assembler, publish, inbox).await
            }
            (RequestType::Get, "frame") => handle_get_frame(&mut request, inbox),
            (RequestType::Get, "brightness") => handle_get_brightness(&mut request, brightness),
            (RequestType::Put, "brightness") => handle_put_brightness(&mut request, brightness),
            _ => {
                set_code(&mut request, ResponseType::NotFound);
                request.response.take()
            }
        };

        if let Some(response) = reply {
            match response.message.to_bytes() {
                Ok(bytes) => {
                    if let Err(e) = socket.send_to(&bytes, src).await {
                        warn!("coap: send error: {e:?}");
                    }
                }
                Err(e) => warn!("coap: encode error: {e:?}"),
            }
        }
    }
}

fn handle_well_known(
    request: &mut CoapRequest<SocketAddr>,
) -> Option<coap_lite::CoapResponse> {
    set_code(request, ResponseType::Content);
    if let Some(ref mut resp) = request.response {
        resp.message.payload = b"</frame>;rt=\"makit64.frame\";sz=12288,\
</brightness>;rt=\"makit64.brightness\",\
</.well-known/core>"
            .to_vec();
    }
    request.response.take()
}

fn handle_get_frame(
    request: &mut CoapRequest<SocketAddr>,
    inbox: &FrameInbox,
) -> Option<coap_lite::CoapResponse> {
    if inbox.has_frame() {
        set_code(request, ResponseType::Content);
        if let Some(ref mut resp) = request.response {
            resp.message.payload = b"ready".to_vec();
        }
    } else {
        set_code(request, ResponseType::NotFound);
        if let Some(ref mut resp) = request.response {
            resp.message.payload = b"no frame".to_vec();
        }
    }
    request.response.take()
}

fn handle_get_brightness(
    request: &mut CoapRequest<SocketAddr>,
    brightness: &Brightness,
) -> Option<coap_lite::CoapResponse> {
    set_code(request, ResponseType::Content);
    if let Some(ref mut resp) = request.response {
        let mut buf = [0u8; 3];
        let s = write_u8_decimal(&mut buf, brightness.get());
        resp.message.payload = s.to_vec();
    }
    request.response.take()
}

fn handle_put_brightness(
    request: &mut CoapRequest<SocketAddr>,
    brightness: &Brightness,
) -> Option<coap_lite::CoapResponse> {
    match parse_brightness_payload(&request.message.payload) {
        Some(level) => {
            brightness.set(level);
            info!("coap: brightness={level}");
            set_code(request, ResponseType::Changed);
            if let Some(ref mut resp) = request.response {
                let mut buf = [0u8; 3];
                let s = write_u8_decimal(&mut buf, level);
                resp.message.payload = s.to_vec();
            }
        }
        None => {
            warn!(
                "coap: PUT /brightness bad payload (len={})",
                request.message.payload.len()
            );
            set_code(request, ResponseType::BadRequest);
            if let Some(ref mut resp) = request.response {
                resp.message.payload = b"expected 0-255 (1 byte or ascii)".to_vec();
            }
        }
    }
    request.response.take()
}

/// Accept a single binary byte `0..=255`, or ASCII decimal `"0"`…`"255"`.
fn parse_brightness_payload(payload: &[u8]) -> Option<u8> {
    if payload.len() == 1 && !payload[0].is_ascii_digit() {
        return Some(payload[0]);
    }
    if payload.is_empty() || payload.len() > 3 {
        return None;
    }
    let mut n: u16 = 0;
    for &b in payload {
        if !b.is_ascii_digit() {
            return None;
        }
        n = n * 10 + u16::from(b - b'0');
        if n > 255 {
            return None;
        }
    }
    Some(n as u8)
}

fn write_u8_decimal(buf: &mut [u8; 3], n: u8) -> &[u8] {
    if n >= 100 {
        buf[0] = b'0' + n / 100;
        buf[1] = b'0' + (n / 10) % 10;
        buf[2] = b'0' + n % 10;
        &buf[..3]
    } else if n >= 10 {
        buf[0] = b'0' + n / 10;
        buf[1] = b'0' + n % 10;
        &buf[..2]
    } else {
        buf[0] = b'0' + n;
        &buf[..1]
    }
}

async fn handle_put_frame(
    request: &mut CoapRequest<SocketAddr>,
    assembler: &mut Block1Assembler,
    publish: &mut [u8; FRAME_BYTES],
    inbox: &FrameInbox,
) -> Option<coap_lite::CoapResponse> {
    let payload = request.message.payload.clone();
    let block1 = request
        .message
        .get_option(CoapOption::Block1)
        .and_then(|opts| opts.front().cloned())
        .and_then(|v| BlockValue::from_bytes(&v));

    match block1 {
        None => {
            if payload.len() != FRAME_BYTES {
                warn!(
                    "coap: PUT /frame wrong size {} (want {FRAME_BYTES}, use Block1)",
                    payload.len()
                );
                set_code(request, ResponseType::BadRequest);
                if let Some(ref mut resp) = request.response {
                    resp.message.payload = b"expected 12288 bytes rgb888".to_vec();
                }
                return request.response.take();
            }
            publish.copy_from_slice(&payload);
            inbox.publish(publish).await;
            info!("coap: frame accepted (single PUT)");
            set_code(request, ResponseType::Changed);
            request.response.take()
        }
        Some(block) => {
            let block_size = block.block_size();
            if block_size > MAX_BLOCK_PAYLOAD || payload.len() > block_size {
                set_code(request, ResponseType::RequestEntityTooLarge);
                return request.response.take();
            }

            let offset = block.num as usize * block_size;
            if offset + payload.len() > FRAME_BYTES {
                warn!(
                    "coap: Block1 overflow num={} len={}",
                    block.num,
                    payload.len()
                );
                assembler.reset();
                set_code(request, ResponseType::RequestEntityTooLarge);
                return request.response.take();
            }

            match assembler.next_num {
                None => {
                    if block.num != 0 {
                        set_code(request, ResponseType::BadRequest);
                        return request.response.take();
                    }
                    assembler.filled = 0;
                    assembler.next_num = Some(0);
                }
                Some(expected) if expected != block.num => {
                    warn!(
                        "coap: Block1 out of order (got {}, expected {expected})",
                        block.num
                    );
                    assembler.reset();
                    set_code(request, ResponseType::BadRequest);
                    return request.response.take();
                }
                Some(_) => {}
            }

            assembler.buf[offset..offset + payload.len()].copy_from_slice(&payload);
            assembler.filled = offset + payload.len();
            assembler.last_activity = Some(Instant::now());

            let echo_bytes: Vec<u8> = block.to_bytes().to_vec();

            if block.more {
                assembler.next_num = Some(block.num.wrapping_add(1));
                set_code(request, ResponseType::Continue);
                if let Some(ref mut resp) = request.response {
                    resp.message.clear_option(CoapOption::Block1);
                    resp.message.add_option(CoapOption::Block1, echo_bytes);
                }
                request.response.take()
            } else {
                if assembler.filled != FRAME_BYTES {
                    warn!(
                        "coap: final Block1 filled {} != {FRAME_BYTES}",
                        assembler.filled
                    );
                    assembler.reset();
                    set_code(request, ResponseType::BadRequest);
                    return request.response.take();
                }
                publish.copy_from_slice(assembler.buf);
                assembler.reset();
                inbox.publish(publish).await;
                info!("coap: frame accepted (Block1)");
                set_code(request, ResponseType::Changed);
                if let Some(ref mut resp) = request.response {
                    resp.message.clear_option(CoapOption::Block1);
                    resp.message.add_option(CoapOption::Block1, echo_bytes);
                }
                request.response.take()
            }
        }
    }
}

fn set_code(request: &mut CoapRequest<SocketAddr>, code: ResponseType) {
    if let Some(ref mut resp) = request.response {
        resp.message.header.code = MessageClass::Response(code);
    }
}

fn normalize_path(path: String) -> String {
    String::from(path.trim_start_matches('/'))
}
