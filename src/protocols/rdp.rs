use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use ironrdp_client::config::{ConfigBuilder, Destination};
use ironrdp_client::rdp::{RdpClient, RdpInputEvent, RdpOutputEvent};
use ironrdp_pdu::input::fast_path::{FastPathInputEvent, KeyboardFlags};
use ironrdp_pdu::input::mouse::{MousePdu, PointerFlags};
use serde_json::json;
use smallvec::smallvec;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

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

/// Map incoming browser mouse events (bitmask + coordinates) to IronRDP FastPath mouse events
fn map_mouse_event(
    mask: u8,
    prev_mask: u8,
    x: u16,
    y: u16,
) -> smallvec::SmallVec<[FastPathInputEvent; 2]> {
    let mut flags = PointerFlags::MOVE;
    let mut wheel_units = 0i16;

    // Left Button
    if (mask & 1) != (prev_mask & 1) {
        flags |= PointerFlags::LEFT_BUTTON;
        if (mask & 1) != 0 {
            flags |= PointerFlags::DOWN;
        }
    } else if (mask & 1) != 0 {
        flags |= PointerFlags::LEFT_BUTTON | PointerFlags::DOWN;
    }

    // Right Button
    if (mask & 4) != (prev_mask & 4) {
        flags |= PointerFlags::RIGHT_BUTTON;
        if (mask & 4) != 0 {
            flags |= PointerFlags::DOWN;
        }
    } else if (mask & 4) != 0 {
        flags |= PointerFlags::RIGHT_BUTTON | PointerFlags::DOWN;
    }

    // Middle Button
    if (mask & 2) != (prev_mask & 2) {
        flags |= PointerFlags::MIDDLE_BUTTON_OR_WHEEL;
        if (mask & 2) != 0 {
            flags |= PointerFlags::DOWN;
        }
    } else if (mask & 2) != 0 {
        flags |= PointerFlags::MIDDLE_BUTTON_OR_WHEEL | PointerFlags::DOWN;
    }

    // Vertical Wheel
    if (mask & 8) != 0 {
        flags |= PointerFlags::VERTICAL_WHEEL;
        wheel_units = 120;
    } else if (mask & 16) != 0 {
        flags |= PointerFlags::VERTICAL_WHEEL | PointerFlags::WHEEL_NEGATIVE;
        wheel_units = -120;
    }

    smallvec![FastPathInputEvent::MouseEvent(MousePdu {
        flags,
        number_of_wheel_rotation_units: wheel_units,
        x_position: x,
        y_position: y,
    })]
}

/// Map incoming RFB/X11 keysyms to IronRDP FastPath keyboard events
fn map_key_event(
    down: bool,
    keysym: u32,
) -> smallvec::SmallVec<[FastPathInputEvent; 2]> {
    let mut kbd_flags = if down {
        KeyboardFlags::empty()
    } else {
        KeyboardFlags::RELEASE
    };

    let scancode = match keysym {
        0xff08 => Some((0x0e, false)), // Backspace
        0xff09 => Some((0x0f, false)), // Tab
        0xff0d => Some((0x1c, false)), // Enter
        0xff1b => Some((0x01, false)), // Escape
        0xffff => Some((0x53, true)),  // Delete
        0xff50 => Some((0x47, true)),  // Home
        0xff51 => Some((0x4b, true)),  // Arrow Left
        0xff52 => Some((0x48, true)),  // Arrow Up
        0xff53 => Some((0x4d, true)),  // Arrow Right
        0xff54 => Some((0x50, true)),  // Arrow Down
        0xff55 => Some((0x49, true)),  // Page Up
        0xff56 => Some((0x51, true)),  // Page Down
        0xff57 => Some((0x4f, true)),  // End
        0xffe1 => Some((0x2a, false)), // Shift_L
        0xffe2 => Some((0x36, false)), // Shift_R
        0xffe3 => Some((0x1d, false)), // Control_L
        0xffe4 => Some((0x1d, true)),  // Control_R
        0xffe9 => Some((0x38, false)), // Alt_L
        0xffea => Some((0x38, true)),  // Alt_R
        0xffeb => Some((0x5b, true)),  // Super / Windows Key
        _ => None,
    };

    if let Some((code, extended)) = scancode {
        if extended {
            kbd_flags |= KeyboardFlags::EXTENDED;
        }
        smallvec![FastPathInputEvent::KeyboardEvent(kbd_flags, code)]
    } else if keysym <= 0xffff && keysym >= 0x20 {
        // Unicode printable character
        smallvec![FastPathInputEvent::UnicodeKeyboardEvent(kbd_flags, keysym as u16)]
    } else {
        smallvec![]
    }
}

/// Handle a full RDP session using IronRDP with NLA (CredSSP), TLS, and RDPGFX decoding
pub async fn handle_rdp_session(socket: WebSocket, params: RdpConnectionParams) {
    let target_display = format!("{}:{}", params.host, params.port);
    info!(
        "RDP Gateway: Initializing connection to {} (user: {:?}, domain: {:?}, size: {}x{})",
        target_display, params.username, params.domain, params.width, params.height
    );

    let destination = Destination::from_parts(params.host.clone(), params.port);
    let mut config_builder = ConfigBuilder::new()
        .with_destination(destination)
        .with_desktop_width(params.width.max(640))
        .with_desktop_height(params.height.max(480))
        .with_credssp(true)
        .with_tls(true)
        .with_pointer_software_rendering(true)
        .with_compression(true);

    if let Some(user) = &params.username {
        if !user.trim().is_empty() {
            config_builder = config_builder.with_username(user.trim());
        }
    }
    if let Some(pass) = &params.password {
        if !pass.is_empty() {
            config_builder = config_builder.with_password(pass);
        }
    }
    if let Some(dom) = &params.domain {
        if !dom.trim().is_empty() {
            config_builder = config_builder.with_domain(dom.trim());
        }
    }

    let config = match config_builder.build() {
        Ok(c) => c,
        Err(e) => {
            error!("RDP Gateway: Invalid configuration: {:#}", e);
            let mut s = socket;
            let _ = s
                .send(Message::Text(
                    json!({
                        "type": "error",
                        "message": format!("Invalid RDP configuration: {}", e)
                    })
                    .to_string(),
                ))
                .await;
            return;
        }
    };

    let (output_tx, mut output_rx) = mpsc::channel::<RdpOutputEvent>(64);
    let client = RdpClient::new(config, output_tx);
    let input_sender = client.input_sender();

    // Spawn IronRDP client on a dedicated thread with a current_thread tokio runtime
    let thread_target = target_display.clone();
    let _rdp_thread = std::thread::Builder::new()
        .name(format!("rdp-{}", thread_target))
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(r) => r,
                Err(e) => {
                    error!("RDP Gateway: Failed to create thread runtime: {}", e);
                    return;
                }
            };
            rt.block_on(async move {
                client.run().await;
            });
        });

    let (mut ws_tx, mut ws_rx) = socket.split();
    let (ws_out_tx, mut ws_out_rx) = mpsc::channel::<Message>(128);

    let is_running = Arc::new(AtomicBool::new(true));
    let is_running_ws_writer = Arc::clone(&is_running);

    // WebSocket Outbound Writer Task
    let ws_writer_task = tokio::spawn(async move {
        while let Some(msg) = ws_out_rx.recv().await {
            if !is_running_ws_writer.load(Ordering::Relaxed) {
                break;
            }
            if ws_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Send initial "init" message to prepare client canvas
    let _ = ws_out_tx
        .send(Message::Text(
            json!({
                "type": "init",
                "protocol": "rdp",
                "width": params.width,
                "height": params.height,
                "name": format!("RDP ({})", target_display)
            })
            .to_string(),
        ))
        .await;

    let input_sender_rx = input_sender.clone();
    let is_running_reader = Arc::clone(&is_running);
    let ws_out_tx_reader = ws_out_tx.clone();

    // Browser Inbound Input Task
    let ws_reader_task = tokio::spawn(async move {
        let mut prev_mouse_mask = 0u8;

        while let Some(Ok(msg)) = ws_rx.next().await {
            if !is_running_reader.load(Ordering::Relaxed) {
                break;
            }
            match msg {
                Message::Binary(data) => {
                    if data.is_empty() {
                        continue;
                    }
                    let packet_type = data[0];
                    if packet_type == 0x02 && data.len() >= 6 {
                        // Pointer event: [0x02, mask: u8, x: u16 (be), y: u16 (be)]
                        let mask = data[1];
                        let x = u16::from_be_bytes([data[2], data[3]]);
                        let y = u16::from_be_bytes([data[4], data[5]]);

                        let events = map_mouse_event(mask, prev_mouse_mask, x, y);
                        prev_mouse_mask = mask;

                        if !events.is_empty() {
                            let _ = input_sender_rx.send(RdpInputEvent::FastPath(events));
                        }
                    } else if packet_type == 0x04 && data.len() >= 6 {
                        // Key event: [0x04, down: u8, keysym: u32 (be)]
                        let down = data[1] != 0;
                        let keysym = u32::from_be_bytes([data[2], data[3], data[4], data[5]]);

                        let events = map_key_event(down, keysym);
                        if !events.is_empty() {
                            let _ = input_sender_rx.send(RdpInputEvent::FastPath(events));
                        }
                    }
                }
                Message::Text(txt) => {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&txt) {
                        if val.get("type").and_then(|v| v.as_str()) == Some("ping") {
                            let _ = ws_out_tx_reader
                                .send(Message::Text(json!({"type": "pong"}).to_string()))
                                .await;
                        }
                    }
                }
                Message::Close(_) => {
                    let _ = input_sender_rx.send(RdpInputEvent::Close);
                    break;
                }
                _ => {}
            }
        }
        is_running_reader.store(false, Ordering::Relaxed);
    });

    let ws_out_tx_events = ws_out_tx.clone();
    let is_running_events = Arc::clone(&is_running);

    let target_display_clone = target_display.clone();

    // Process output events from the IronRDP client
    let output_loop = async move {
        let mut initial_size_sent = false;

        while let Some(event) = output_rx.recv().await {
            if !is_running_events.load(Ordering::Relaxed) {
                break;
            }

            match event {
                RdpOutputEvent::Image { buffer, width, height } => {
                    let w = width.get();
                    let h = height.get();

                    if !initial_size_sent || w != params.width || h != params.height {
                        let _ = ws_out_tx_events
                            .send(Message::Text(
                                json!({
                                    "type": "init",
                                    "protocol": "rdp",
                                    "width": w,
                                    "height": h,
                                    "name": format!("RDP ({})", target_display_clone)
                                })
                                .to_string(),
                            ))
                            .await;
                        initial_size_sent = true;
                    }

                    // Convert RGB to RGBA binary frame tile [0x01, x:0, y:0, w, h, rgba...]
                    let total_pixels = (w as usize) * (h as usize);
                    let mut payload = Vec::with_capacity(9 + total_pixels * 4);
                    payload.push(0x01); // Frame tile
                    payload.extend_from_slice(&0u16.to_be_bytes()); // x
                    payload.extend_from_slice(&0u16.to_be_bytes()); // y
                    payload.extend_from_slice(&w.to_be_bytes());     // width
                    payload.extend_from_slice(&h.to_be_bytes());     // height

                    for &pixel in &buffer {
                        let r = ((pixel >> 16) & 0xFF) as u8;
                        let g = ((pixel >> 8) & 0xFF) as u8;
                        let b = (pixel & 0xFF) as u8;
                        payload.push(r);
                        payload.push(g);
                        payload.push(b);
                        payload.push(255); // Alpha
                    }

                    if ws_out_tx_events.send(Message::Binary(payload)).await.is_err() {
                        break;
                    }
                }
                RdpOutputEvent::ConnectionFailure(err) => {
                    error!("RDP Gateway: Connection failure: {}", err);
                    let _ = ws_out_tx_events
                        .send(Message::Text(
                            json!({
                                "type": "error",
                                "message": format!("RDP Connection Failed: {}", err)
                            })
                            .to_string(),
                        ))
                        .await;
                    break;
                }
                RdpOutputEvent::Terminated(reason) => {
                    match reason {
                        Ok(graceful) => {
                            info!("RDP Gateway: Session disconnected gracefully: {:?}", graceful);
                        }
                        Err(err) => {
                            warn!("RDP Gateway: Session terminated with error: {}", err);
                            let _ = ws_out_tx_events
                                .send(Message::Text(
                                    json!({
                                        "type": "error",
                                        "message": format!("RDP Session Terminated: {}", err)
                                    })
                                    .to_string(),
                                ))
                                .await;
                        }
                    }
                    break;
                }
                _ => {}
            }
        }
    };

    output_loop.await;

    is_running.store(false, Ordering::Relaxed);
    let _ = input_sender.send(RdpInputEvent::Close);

    ws_reader_task.abort();
    ws_writer_task.abort();

    info!("RDP Gateway: Session ended for {}", target_display);
}
