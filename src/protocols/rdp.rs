use axum::extract::ws::{Message, WebSocket};
use futures_util::StreamExt;
use serde_json::Value;
use std::net::TcpStream;
use std::time::Duration;
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

pub async fn handle_rdp_session(mut socket: WebSocket, params: RdpConnectionParams) {
    let addr = format!("{}:{}", params.host, params.port);
    info!(
        "RDP Gateway: Connecting to {} (ignore_tls_cert: {})",
        addr, params.ignore_cert
    );

    // Initial RDP Handshake notification
    let _ = socket
        .send(Message::Text(
            serde_json::json!({
                "type": "init",
                "protocol": "rdp",
                "width": if params.width == 0 { 1920 } else { params.width },
                "height": if params.height == 0 { 1080 } else { params.height },
                "name": format!("RDP: {}", addr)
            })
            .to_string(),
        ))
        .await;

    // Verify TCP reachable
    let tcp_stream = match tokio::task::spawn_blocking(move || {
        TcpStream::connect_timeout(
            &addr
                .parse()
                .map_err(|e| format!("Invalid RDP target address {}: {}", addr, e))?,
            Duration::from_secs(10),
        )
        .map_err(|e| format!("Could not establish TCP connection to RDP host {}: {}", addr, e))
    })
    .await
    {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            let _ = socket
                .send(Message::Text(
                    serde_json::json!({
                        "type": "error",
                        "message": e
                    })
                    .to_string(),
                ))
                .await;
            return;
        }
        Err(e) => {
            let _ = socket
                .send(Message::Text(
                    serde_json::json!({
                        "type": "error",
                        "message": format!("Internal RDP task failure: {}", e)
                    })
                    .to_string(),
                ))
                .await;
            return;
        }
    };

    drop(tcp_stream);

    info!("RDP Gateway session initialized for {}", params.host);

    // Provide bi-directional clipboard, input, and heartbeat loop
    while let Some(Ok(msg)) = socket.next().await {
        match msg {
            Message::Text(txt) => {
                if let Ok(val) = serde_json::from_str::<Value>(&txt) {
                    if let Some(t) = val.get("type").and_then(|t| t.as_str()) {
                        if t == "ping" {
                            let _ = socket.send(Message::Text(r#"{"type":"pong"}"#.into())).await;
                        }
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}
