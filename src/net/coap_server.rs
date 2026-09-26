//! CoAP UDP server: `PUT /frame` (RGB888 + Block1).

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
pub async fn coap_task(stack: Stack<'static>, inbox: &'static FrameInbox) -> ! {
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
    info!("coap: listening on UDP :{COAP_PORT}  PUT /frame");

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
        resp.message.payload =
            b"</frame>;rt=\"makit64.frame\";sz=12288,</.well-known/core>".to_vec();
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
