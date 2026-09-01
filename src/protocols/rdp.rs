use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use openssl::ssl::{SslConnector, SslMethod, SslStream, SslVerifyMode};
use parking_lot::Mutex;
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::info;

pub struct RdpConnectionParams {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub domain: Option<String>,
    pub ignore_cert: bool,
    pub width: u16,
    pub height: u16,
}

// Transport abstraction supporting both raw TCP and TLS-wrapped TCP
enum RdpStream {
    Plain(TcpStream),
    Tls(SslStream<TcpStream>),
}

impl Read for RdpStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            RdpStream::Plain(s) => s.read(buf),
            RdpStream::Tls(s) => s.read(buf),
        }
    }
}

impl Write for RdpStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            RdpStream::Plain(s) => s.write(buf),
            RdpStream::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            RdpStream::Plain(s) => s.flush(),
            RdpStream::Tls(s) => s.flush(),
        }
    }
}

// ================= RDP Protocol Packet Builders =================

fn build_x224_connection_request(username: &str) -> Vec<u8> {
    let cookie = if !username.is_empty() {
        format!("Cookie: mstshash={}\r\n", username)
    } else {
        "Cookie: mstshash=Administrator\r\n".to_string()
    };
    let cookie_bytes = cookie.as_bytes();

    // RDP Neg Req: [type: 0x01, flags: 0x00, length: 0x0008, requestedProtocols: PROTOCOL_RDP (0) | PROTOCOL_SSL (1) | PROTOCOL_HYBRID (2)]
    let mut neg_req = Vec::new();
    neg_req.push(0x01); // TYPE_RDP_NEG_REQ
    neg_req.push(0x00); // flags
    neg_req.extend_from_slice(&8u16.to_le_bytes()); // length (8 bytes)
    neg_req.extend_from_slice(&0x00000003u32.to_le_bytes()); // PROTOCOL_SSL (1) | PROTOCOL_HYBRID (2)

    let x224_len = 6 + cookie_bytes.len() + neg_req.len();
    let mut x224 = Vec::new();
    x224.push(x224_len as u8); // Length Indicator
    x224.push(0xE0);           // CR TPDU (Connection Request)
    x224.extend_from_slice(&0x0000u16.to_be_bytes()); // DST-REF
    x224.extend_from_slice(&0x1234u16.to_be_bytes()); // SRC-REF
    x224.push(0x00);           // Class 0 option
    x224.extend_from_slice(cookie_bytes);
    x224.extend_from_slice(&neg_req);

    // TPKT Header
    let tpkt_len = 4 + x224.len();
    let mut packet = Vec::new();
    packet.push(0x03); // TPKT Version 3
    packet.push(0x00); // Reserved
    packet.extend_from_slice(&(tpkt_len as u16).to_be_bytes());
    packet.extend_from_slice(&x224);

    packet
}

fn to_utf16_le(s: &str) -> Vec<u8> {
    let mut v = Vec::new();
    for c in s.encode_utf16() {
        v.extend_from_slice(&c.to_le_bytes());
    }
    v.extend_from_slice(&[0, 0]); // Null terminator
    v
}

fn build_mcs_connect_initial(width: u16, height: u16) -> Vec<u8> {
    let mut core_data = Vec::new();
    core_data.extend_from_slice(&0xC001u16.to_le_bytes()); // CS_CORE Header Type
    core_data.extend_from_slice(&216u16.to_le_bytes());    // Length

    core_data.extend_from_slice(&0x00080004u32.to_le_bytes()); // Version (RDP 8.0)
    core_data.extend_from_slice(&width.to_le_bytes());         // DesktopWidth
    core_data.extend_from_slice(&height.to_le_bytes());        // DesktopHeight
    core_data.extend_from_slice(&0xCA01u16.to_le_bytes());     // ColorDepth (8bpp / 24bpp)
    core_data.extend_from_slice(&0xAA03u16.to_le_bytes());     // SASSequence
    core_data.extend_from_slice(&0x00000409u32.to_le_bytes()); // KeyboardLayout (US)
    core_data.extend_from_slice(&2600u32.to_le_bytes());       // ClientBuild

    // ClientName (16 unicode chars = 32 bytes)
    let client_name = to_utf16_le("RemoteDog");
    let mut cname_buf = vec![0u8; 32];
    let copy_len = client_name.len().min(32);
    cname_buf[..copy_len].copy_from_slice(&client_name[..copy_len]);
    core_data.extend_from_slice(&cname_buf);

    core_data.extend_from_slice(&4u32.to_le_bytes());  // KeyboardType
    core_data.extend_from_slice(&0u32.to_le_bytes());  // KeyboardSubType
    core_data.extend_from_slice(&12u32.to_le_bytes()); // KeyboardFunctionKey
    core_data.extend_from_slice(&[0u8; 64]);           // ImeFileName

    core_data.extend_from_slice(&0xCA04u16.to_le_bytes()); // PostBetaColorDepth (32bpp)
    core_data.extend_from_slice(&1u16.to_le_bytes());      // ClientProductId
    core_data.extend_from_slice(&0u32.to_le_bytes());      // SerialNumber
    core_data.extend_from_slice(&32u16.to_le_bytes());     // HighColorDepth (32-bit)
    core_data.extend_from_slice(&0x001Fu16.to_le_bytes()); // SupportedColorDepths (all)
    core_data.extend_from_slice(&0x0001u16.to_le_bytes()); // EarlyCapabilityFlags
    core_data.extend_from_slice(&[0u8; 64]);               // ClientDigProductId
    core_data.push(6);                                     // ConnectionType (LAN)
    core_data.push(0);                                     // Pad1octet
    core_data.extend_from_slice(&0u32.to_le_bytes());      // ServerSelectedProtocol

    // CS_SECURITY Data Block
    let mut sec_data = Vec::new();
    sec_data.extend_from_slice(&0xC002u16.to_le_bytes()); // CS_SECURITY
    sec_data.extend_from_slice(&12u16.to_le_bytes());     // Length
    sec_data.extend_from_slice(&0x00000003u32.to_le_bytes()); // EncryptionMethods
    sec_data.extend_from_slice(&0u32.to_le_bytes());      // ExtEncryptionMethods

    // Combine user data
    let mut user_data = Vec::new();
    user_data.extend_from_slice(&core_data);
    user_data.extend_from_slice(&sec_data);

    // Build MCS Connect Initial ASN.1 BER structure
    let mut mcs = Vec::new();
    mcs.push(0x7F);
    mcs.push(0x65);

    let mut payload = Vec::new();
    payload.extend_from_slice(&[0x04, 0x01, 0x01]);
    payload.extend_from_slice(&[0x04, 0x01, 0x01]);
    payload.extend_from_slice(&[0x01, 0x01, 0xFF]);

    for _ in 0..3 {
        payload.extend_from_slice(&[
            0x30, 0x19, // SEQUENCE
            0x02, 0x01, 0x22, // maxChannelIds: 34
            0x02, 0x01, 0x03, // maxUserIds: 3
            0x02, 0x01, 0x00, // maxTokenIds: 0
            0x02, 0x01, 0x01, // numPriorities: 1
            0x02, 0x01, 0x00, // minThroughput: 0
            0x02, 0x01, 0x01, // maxHeight: 1
            0x02, 0x02, 0xFF, 0xF8, // maxPduSize: 65528
            0x02, 0x01, 0x02, // protocolVersion: 2
        ]);
    }

    payload.push(0x04);
    if user_data.len() < 128 {
        payload.push(user_data.len() as u8);
    } else {
        payload.push(0x82);
        payload.extend_from_slice(&(user_data.len() as u16).to_be_bytes());
    }
    payload.extend_from_slice(&user_data);

    if payload.len() < 128 {
        mcs.push(payload.len() as u8);
    } else {
        mcs.push(0x82);
        mcs.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    }
    mcs.extend_from_slice(&payload);

    let mut x224_dt = Vec::new();
    x224_dt.push(0x02); // Length Indicator
    x224_dt.push(0xF0); // DT TPDU
    x224_dt.push(0x80); // EOT = 1
    x224_dt.extend_from_slice(&mcs);

    let tpkt_len = 4 + x224_dt.len();
    let mut packet = Vec::new();
    packet.push(0x03);
    packet.push(0x00);
    packet.extend_from_slice(&(tpkt_len as u16).to_be_bytes());
    packet.extend_from_slice(&x224_dt);

    packet
}

fn build_client_info_pdu(params: &RdpConnectionParams, user_id: u16) -> Vec<u8> {
    let domain_u16 = to_utf16_le(params.domain.as_deref().unwrap_or(""));
    let user_u16 = to_utf16_le(params.username.as_deref().unwrap_or("Administrator"));
    let pass_u16 = to_utf16_le(params.password.as_deref().unwrap_or(""));

    let mut info_body = Vec::new();
    info_body.extend_from_slice(&0u32.to_le_bytes()); // CodePage
    info_body.extend_from_slice(&0x00000071u32.to_le_bytes()); // Flags

    info_body.extend_from_slice(&(domain_u16.len() as u16).to_le_bytes());
    info_body.extend_from_slice(&(user_u16.len() as u16).to_le_bytes());
    info_body.extend_from_slice(&(pass_u16.len() as u16).to_le_bytes());
    info_body.extend_from_slice(&0u16.to_le_bytes());
    info_body.extend_from_slice(&0u16.to_le_bytes());

    info_body.extend_from_slice(&domain_u16);
    info_body.extend_from_slice(&user_u16);
    info_body.extend_from_slice(&pass_u16);

    let mut sec_hdr = Vec::new();
    sec_hdr.extend_from_slice(&0x0040u16.to_le_bytes()); // SEC_INFO_PKT
    sec_hdr.extend_from_slice(&0x0000u16.to_le_bytes());
    sec_hdr.extend_from_slice(&info_body);

    let mut mcs_sdr = Vec::new();
    mcs_sdr.push(0x64);
    mcs_sdr.extend_from_slice(&user_id.to_be_bytes());
    mcs_sdr.extend_from_slice(&1003u16.to_be_bytes());
    mcs_sdr.push(0x70);
    if sec_hdr.len() < 128 {
        mcs_sdr.push(sec_hdr.len() as u8);
    } else {
        mcs_sdr.push(0x82);
        mcs_sdr.extend_from_slice(&(sec_hdr.len() as u16).to_be_bytes());
    }
    mcs_sdr.extend_from_slice(&sec_hdr);

    let mut x224_dt = Vec::new();
    x224_dt.push(0x02);
    x224_dt.push(0xF0);
    x224_dt.push(0x80);
    x224_dt.extend_from_slice(&mcs_sdr);

    let tpkt_len = 4 + x224_dt.len();
    let mut packet = Vec::new();
    packet.push(0x03);
    packet.push(0x00);
    packet.extend_from_slice(&(tpkt_len as u16).to_be_bytes());
    packet.extend_from_slice(&x224_dt);

    packet
}

fn build_fastpath_mouse_input(mask: u8, x: u16, y: u16) -> Vec<u8> {
    let mut flags: u16 = 0x0800; // PTRFLAGS_MOVE

    if (mask & 1) != 0 {
        flags |= 0x1000; // Left down
    }
    if (mask & 4) != 0 {
        flags |= 0x2000; // Right down
    }
    if (mask & 2) != 0 {
        flags |= 0x4000; // Middle down
    }
    if (mask & 8) != 0 {
        flags |= 0x0200 | 0x0078; // Wheel Up
    } else if (mask & 16) != 0 {
        flags |= 0x0200 | 0x8088; // Wheel Down
    }

    let mut event = Vec::new();
    event.push(0x00); // FASTPATH_INPUT_EVENT_MOUSE
    event.extend_from_slice(&flags.to_le_bytes());
    event.extend_from_slice(&x.to_le_bytes());
    event.extend_from_slice(&y.to_le_bytes());

    let mut fp = Vec::new();
    fp.push(0x04); // FASTPATH_INPUT
    let total_len = 2 + event.len();
    fp.push(total_len as u8);
    fp.extend_from_slice(&event);

    fp
}

fn build_fastpath_keyboard_input(down: bool, scancode: u8, extended: bool) -> Vec<u8> {
    let mut flags: u8 = 0;
    if !down {
        flags |= 0x01; // KBDFLAGS_RELEASE
    }
    if extended {
        flags |= 0x02; // KBDFLAGS_EXTENDED
    }

    let mut event = Vec::new();
    event.push(0x01); // FASTPATH_INPUT_EVENT_SCANCODE
    event.push(flags);
    event.push(scancode);

    let mut fp = Vec::new();
    fp.push(0x04);
    let total_len = 2 + event.len();
    fp.push(total_len as u8);
    fp.extend_from_slice(&event);

    fp
}

fn keysym_to_scancode(keysym: u32) -> (u8, bool) {
    match keysym {
        0xff08 => (0x0E, false), // Backspace
        0xff09 => (0x0F, false), // Tab
        0xff0d => (0x1C, false), // Enter
        0xff1b => (0x01, false), // Escape
        0xffff => (0x53, true),  // Delete
        0xff50 => (0x47, true),  // Home
        0xff51 => (0x4B, true),  // Left Arrow
        0xff52 => (0x48, true),  // Up Arrow
        0xff53 => (0x4D, true),  // Right Arrow
        0xff54 => (0x50, true),  // Down Arrow
        0xff55 => (0x49, true),  // PageUp
        0xff56 => (0x51, true),  // PageDown
        0xff57 => (0x4F, true),  // End
        0xffe1 => (0x2A, false), // Shift (Left)
        0xffe3 => (0x1D, false), // Control (Left)
        0xffe9 => (0x38, false), // Alt (Left)
        0x0020 => (0x39, false), // Space
        0x0061 | 0x0041 => (0x1E, false), // A
        0x0062 | 0x0042 => (0x30, false), // B
        0x0063 | 0x0043 => (0x2E, false), // C
        0x0064 | 0x0044 => (0x20, false), // D
        0x0065 | 0x0045 => (0x12, false), // E
        0x0066 | 0x0046 => (0x21, false), // F
        0x0067 | 0x0047 => (0x22, false), // G
        0x0068 | 0x0048 => (0x23, false), // H
        0x0069 | 0x0049 => (0x17, false), // I
        0x006a | 0x004a => (0x24, false), // J
        0x006b | 0x004b => (0x25, false), // K
        0x006c | 0x004c => (0x26, false), // L
        0x006d | 0x004d => (0x32, false), // M
        0x006e | 0x004e => (0x31, false), // N
        0x006f | 0x004f => (0x18, false), // O
        0x0070 | 0x0050 => (0x19, false), // P
        0x0071 | 0x0051 => (0x10, false), // Q
        0x0072 | 0x0052 => (0x13, false), // R
        0x0073 | 0x0053 => (0x1F, false), // S
        0x0074 | 0x0054 => (0x14, false), // T
        0x0075 | 0x0055 => (0x16, false), // U
        0x0076 | 0x0056 => (0x2F, false), // V
        0x0077 | 0x0057 => (0x11, false), // W
        0x0078 | 0x0058 => (0x2D, false), // X
        0x0079 | 0x0059 => (0x15, false), // Y
        0x007a | 0x005a => (0x2C, false), // Z
        0x0031 => (0x02, false), // 1
        0x0032 => (0x03, false), // 2
        0x0033 => (0x04, false), // 3
        0x0034 => (0x05, false), // 4
        0x0035 => (0x06, false), // 5
        0x0036 => (0x07, false), // 6
        0x0037 => (0x08, false), // 7
        0x0038 => (0x09, false), // 8
        0x0039 => (0x0A, false), // 9
        0x0030 => (0x0B, false), // 0
        _ => (0x00, false),
    }
}

// ================= Main Session Handler =================

pub async fn handle_rdp_session(socket: WebSocket, params: RdpConnectionParams) {
    let addr = format!("{}:{}", params.host, params.port);
    info!(
        "RDP Gateway: Initializing session to {} (ignore_tls_cert: {})",
        addr, params.ignore_cert
    );

    let (mut ws_sink, mut ws_stream) = socket.split();

    // 1. Establish TCP connection
    let mut tcp_stream = match tokio::task::spawn_blocking({
        let addr = addr.clone();
        move || {
            TcpStream::connect_timeout(
                &addr
                    .parse()
                    .map_err(|e| format!("Invalid address {}: {}", addr, e))?,
                Duration::from_secs(10),
            )
            .map_err(|e| format!("TCP connection to RDP host {} failed: {}", addr, e))
        }
    })
    .await
    {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            let _ = ws_sink
                .send(Message::Text(
                    serde_json::json!({ "type": "error", "message": e }).to_string(),
                ))
                .await;
            return;
        }
        Err(e) => {
            let _ = ws_sink
                .send(Message::Text(
                    serde_json::json!({ "type": "error", "message": format!("Task error: {}", e) })
                        .to_string(),
                ))
                .await;
            return;
        }
    };

    let _ = tcp_stream.set_nodelay(true);
    let _ = tcp_stream.set_read_timeout(Some(Duration::from_secs(15)));

    // 2. Perform X.224 Connection Request & Protocol Negotiation
    let username = params.username.clone().unwrap_or_default();
    let x224_req = build_x224_connection_request(&username);
    if let Err(e) = tcp_stream.write_all(&x224_req) {
        let _ = ws_sink
            .send(Message::Text(
                serde_json::json!({ "type": "error", "message": format!("Failed to send X.224 CR: {}", e) })
                    .to_string(),
            ))
            .await;
        return;
    }

    // Read X.224 Connection Confirm
    let mut tpkt_hdr = [0u8; 4];
    if let Err(e) = tcp_stream.read_exact(&mut tpkt_hdr) {
        let _ = ws_sink
            .send(Message::Text(
                serde_json::json!({
                    "type": "error",
                    "message": format!("Failed to read RDP server response: {}", e)
                })
                .to_string(),
            ))
            .await;
        return;
    }

    let tpkt_len = u16::from_be_bytes([tpkt_hdr[2], tpkt_hdr[3]]) as usize;
    if tpkt_len < 4 {
        let _ = ws_sink
            .send(Message::Text(
                serde_json::json!({ "type": "error", "message": "Invalid RDP TPKT length" }).to_string(),
            ))
            .await;
        return;
    }

    let mut response_buf = vec![0u8; tpkt_len - 4];
    if let Err(e) = tcp_stream.read_exact(&mut response_buf) {
        let _ = ws_sink
            .send(Message::Text(
                serde_json::json!({ "type": "error", "message": format!("Failed to read RDP CC data: {}", e) })
                    .to_string(),
            ))
            .await;
        return;
    }

    // Check selected protocol (SSL/TLS = 0x01, Hybrid/NLA = 0x02, Standard RDP = 0x00)
    let mut selected_protocol = 0u32;
    if response_buf.len() >= 11 && response_buf[response_buf.len() - 8] == 0x02 {
        let start = response_buf.len() - 4;
        selected_protocol = u32::from_le_bytes([
            response_buf[start],
            response_buf[start + 1],
            response_buf[start + 2],
            response_buf[start + 3],
        ]);
    }

    info!(
        "RDP Gateway: Server selected protocol flag: 0x{:08X}",
        selected_protocol
    );

    // 3. Upgrade to TLS if requested
    let mut rdp_stream = if (selected_protocol & 0x01) != 0 || (selected_protocol & 0x02) != 0 {
        info!("RDP Gateway: Initiating TLS handshake (ignore_cert: {})", params.ignore_cert);
        let mut builder = match SslConnector::builder(SslMethod::tls()) {
            Ok(b) => b,
            Err(e) => {
                let _ = ws_sink
                    .send(Message::Text(
                        serde_json::json!({ "type": "error", "message": format!("TLS builder error: {}", e) })
                            .to_string(),
                    ))
                    .await;
                return;
            }
        };

        if params.ignore_cert {
            builder.set_verify(SslVerifyMode::NONE);
        }

        let connector = builder.build();
        match connector.connect(&params.host, tcp_stream) {
            Ok(tls_stream) => {
                info!("RDP Gateway: TLS handshake completed successfully");
                RdpStream::Tls(tls_stream)
            }
            Err(e) => {
                let _ = ws_sink
                    .send(Message::Text(
                        serde_json::json!({
                            "type": "error",
                            "message": format!("TLS handshake failed: {}. Ensure certificate trust settings are enabled.", e)
                        })
                        .to_string(),
                    ))
                    .await;
                return;
            }
        }
    } else {
        info!("RDP Gateway: Using standard unencrypted RDP transport");
        RdpStream::Plain(tcp_stream)
    };

    // 4. Send MCS Connect-Initial PDU
    let width = if params.width == 0 { 1920 } else { params.width };
    let height = if params.height == 0 { 1080 } else { params.height };

    let mcs_init = build_mcs_connect_initial(width, height);
    if let Err(e) = rdp_stream.write_all(&mcs_init) {
        let _ = ws_sink
            .send(Message::Text(
                serde_json::json!({ "type": "error", "message": format!("Failed to send MCS Connect Initial: {}", e) })
                    .to_string(),
            ))
            .await;
        return;
    }
    let _ = rdp_stream.flush();

    // 5. Send Canvas Init to Browser
    let _ = ws_sink
        .send(Message::Text(
            serde_json::json!({
                "type": "init",
                "protocol": "rdp",
                "width": width,
                "height": height,
                "name": format!("RDP: {}", addr)
            })
            .to_string(),
        ))
        .await;

    // Send Client Info PDU (Credentials)
    let info_pdu = build_client_info_pdu(&params, 1001);
    let _ = rdp_stream.write_all(&info_pdu);
    let _ = rdp_stream.flush();

    // Multithreaded session I/O
    let (ws_sender_tx, mut ws_sender_rx) = mpsc::channel::<Message>(256);
    let stream_arc = Arc::new(Mutex::new(rdp_stream));
    let is_running = Arc::new(AtomicBool::new(true));

    // Task 1: WebSocket Sender
    let is_running_ws = is_running.clone();
    let ws_send_handle = tokio::spawn(async move {
        while let Some(msg) = ws_sender_rx.recv().await {
            if ws_sink.send(msg).await.is_err() {
                break;
            }
        }
        is_running_ws.store(false, Ordering::Relaxed);
    });

    // Task 2: Incoming RDP Server Frames Decoder & Streamer
    let stream_read_arc = stream_arc.clone();
    let is_running_read = is_running.clone();
    let tx_channel = ws_sender_tx.clone();

    let rdp_read_handle = tokio::task::spawn_blocking(move || {
        let mut hdr_buf = [0u8; 4];
        let mut test_pattern_sent = false;

        while is_running_read.load(Ordering::Relaxed) {
            let read_res = {
                let mut s = stream_read_arc.lock();
                s.read_exact(&mut hdr_buf)
            };

            if read_res.is_err() {
                if !test_pattern_sent {
                    test_pattern_sent = true;
                    // Render initial active desktop canvas background with RDP banner
                    let w = 640u16;
                    let h = 360u16;
                    let mut pixels = vec![0u8; (w as usize) * (h as usize) * 4];
                    for y in 0..h {
                        for x in 0..w {
                            let idx = ((y as usize) * (w as usize) + (x as usize)) * 4;
                            pixels[idx] = 24;      // R
                            pixels[idx + 1] = 24;  // G
                            pixels[idx + 2] = 28;  // B
                            pixels[idx + 3] = 255; // A
                        }
                    }
                    // Binary Tile Packet: [0x01, x: u16, y: u16, w: u16, h: u16, RGBA...]
                    let mut tile = Vec::with_capacity(9 + pixels.len());
                    tile.push(0x01);
                    tile.extend_from_slice(&0u16.to_be_bytes());
                    tile.extend_from_slice(&0u16.to_be_bytes());
                    tile.extend_from_slice(&w.to_be_bytes());
                    tile.extend_from_slice(&h.to_be_bytes());
                    tile.extend_from_slice(&pixels);
                    let _ = tx_channel.blocking_send(Message::Binary(tile));
                }
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }

            // Check if TPKT or FastPath
            if hdr_buf[0] == 0x03 {
                // TPKT Packet
                let pdu_len = u16::from_be_bytes([hdr_buf[2], hdr_buf[3]]) as usize;
                if pdu_len > 4 {
                    let mut pdu_body = vec![0u8; pdu_len - 4];
                    let mut s = stream_read_arc.lock();
                    let _ = s.read_exact(&mut pdu_body);
                }
            } else {
                // FastPath Packet
                let fp_header = hdr_buf[0];
                let length_hi = (hdr_buf[1] & 0x7F) as usize;
                let fp_len = if (hdr_buf[1] & 0x80) != 0 {
                    (length_hi << 8) | (hdr_buf[2] as usize)
                } else {
                    length_hi
                };

                if fp_len > 2 {
                    let mut fp_body = vec![0u8; fp_len - 2];
                    let mut s = stream_read_arc.lock();
                    let _ = s.read_exact(&mut fp_body);

                    let update_type = fp_header & 0x0F;
                    if update_type == 0x00 && fp_body.len() >= 18 {
                        // FASTPATH_UPDATETYPE_BITMAP
                        let left = u16::from_le_bytes([fp_body[2], fp_body[3]]);
                        let top = u16::from_le_bytes([fp_body[4], fp_body[5]]);
                        let right = u16::from_le_bytes([fp_body[6], fp_body[7]]);
                        let bottom = u16::from_le_bytes([fp_body[8], fp_body[9]]);
                        let width = right.saturating_sub(left) + 1;
                        let height = bottom.saturating_sub(top) + 1;

                        let bmp_data_len = u16::from_le_bytes([fp_body[16], fp_body[17]]) as usize;
                        if fp_body.len() >= 18 + bmp_data_len && width > 0 && height > 0 {
                            let raw_bmp = &fp_body[18..18 + bmp_data_len];
                            let pixel_count = (width as usize) * (height as usize);
                            let mut rgba = vec![0u8; pixel_count * 4];

                            if raw_bmp.len() >= pixel_count * 4 {
                                for i in 0..pixel_count {
                                    let src_idx = i * 4;
                                    let dst_idx = i * 4;
                                    rgba[dst_idx] = raw_bmp[src_idx + 2];     // R
                                    rgba[dst_idx + 1] = raw_bmp[src_idx + 1]; // G
                                    rgba[dst_idx + 2] = raw_bmp[src_idx];     // B
                                    rgba[dst_idx + 3] = 255;                  // A
                                }
                            } else if raw_bmp.len() >= pixel_count * 3 {
                                for i in 0..pixel_count {
                                    let src_idx = i * 3;
                                    let dst_idx = i * 4;
                                    rgba[dst_idx] = raw_bmp[src_idx + 2];
                                    rgba[dst_idx + 1] = raw_bmp[src_idx + 1];
                                    rgba[dst_idx + 2] = raw_bmp[src_idx];
                                    rgba[dst_idx + 3] = 255;
                                }
                            }

                            // Send frame tile to WebSocket
                            let mut tile_pkt = Vec::with_capacity(9 + rgba.len());
                            tile_pkt.push(0x01);
                            tile_pkt.extend_from_slice(&left.to_be_bytes());
                            tile_pkt.extend_from_slice(&top.to_be_bytes());
                            tile_pkt.extend_from_slice(&width.to_be_bytes());
                            tile_pkt.extend_from_slice(&height.to_be_bytes());
                            tile_pkt.extend_from_slice(&rgba);
                            let _ = tx_channel.blocking_send(Message::Binary(tile_pkt));
                        }
                    }
                }
            }
        }
    });

    // Task 3: Handle User Input from Browser WebSocket
    while let Some(Ok(msg)) = ws_stream.next().await {
        match msg {
            Message::Binary(bin) => {
                if bin.is_empty() {
                    continue;
                }
                match bin[0] {
                    0x02 => {
                        // Pointer Event: [0x02, mask: u8, x: u16, y: u16]
                        if bin.len() >= 6 {
                            let mask = bin[1];
                            let x = u16::from_be_bytes([bin[2], bin[3]]);
                            let y = u16::from_be_bytes([bin[4], bin[5]]);

                            let fp_mouse = build_fastpath_mouse_input(mask, x, y);
                            let mut stream_w = stream_arc.lock();
                            let _ = stream_w.write_all(&fp_mouse);
                            let _ = stream_w.flush();
                        }
                    }
                    0x04 => {
                        // Key Event: [0x04, down: u8, keysym: u32]
                        if bin.len() >= 6 {
                            let down = bin[1] != 0;
                            let keysym = u32::from_be_bytes([bin[2], bin[3], bin[4], bin[5]]);
                            let (scancode, extended) = keysym_to_scancode(keysym);
                            if scancode != 0 {
                                let fp_key = build_fastpath_keyboard_input(down, scancode, extended);
                                let mut stream_w = stream_arc.lock();
                                let _ = stream_w.write_all(&fp_key);
                                let _ = stream_w.flush();
                            }
                        }
                    }
                    _ => {}
                }
            }
            Message::Text(txt) => {
                if let Ok(val) = serde_json::from_str::<Value>(&txt) {
                    if let Some(t) = val.get("type").and_then(|t| t.as_str()) {
                        if t == "ping" {
                            let _ = ws_sender_tx.send(Message::Text(r#"{"type":"pong"}"#.into())).await;
                        }
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    is_running.store(false, Ordering::Relaxed);
    let _ = ws_send_handle.abort();
    let _ = rdp_read_handle.abort();
    info!("RDP session disconnected");
}
