//! WebSocket transport: handshake admission and per-connection frame loop.
//!
//! Wire behaviour mirrors `src/gateway/server-http-upgrades.ts` plus
//! `src/gateway/server/ws-connection/message-handler.ts`: the first client frame
//! must be a `req` with method `connect`, which is answered with a `res` whose
//! payload is the `hello-ok` object.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use openclaw_protocol::{
    ConnectParams, ErrorShape, EventFrame, Features, GatewayFrame, HelloAuth, HelloOk, Policy,
    RequestFrame, ResponseFrame, ServerInfo, GATEWAY_EVENTS, GATEWAY_METHODS, GATEWAY_SERVER_CAPS,
    PROTOCOL_VERSION,
};
use serde_json::json;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use crate::methods::dispatch;
use crate::state::AppState;
use crate::store::now_ms;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

pub async fn gateway_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    let first = match tokio::time::timeout(HANDSHAKE_TIMEOUT, receiver.next()).await {
        Ok(Some(Ok(Message::Text(text)))) => text.to_string(),
        Ok(Some(Ok(_))) => {
            let _ = close_with_error(
                &mut sender,
                ErrorShape::invalid_params("first frame must be a text connect request"),
            )
            .await;
            return;
        }
        Ok(Some(Err(_))) | Ok(None) | Err(_) => return,
    };

    let request = match GatewayFrame::parse(&first) {
        Ok(GatewayFrame::Req(request)) => request,
        Ok(_) => {
            let _ = close_with_error(
                &mut sender,
                ErrorShape::invalid_params("first frame must be a request frame"),
            )
            .await;
            return;
        }
        Err(error) => {
            let _ = close_with_error(
                &mut sender,
                ErrorShape::invalid_params(format!("invalid json frame: {error}")),
            )
            .await;
            return;
        }
    };

    if request.method != "connect" {
        let response = ResponseFrame::err(
            request.id,
            ErrorShape::invalid_params("invalid handshake: first request must be connect"),
        );
        let _ = send_frame(&mut sender, &GatewayFrame::Res(response)).await;
        return;
    }

    let params: ConnectParams = match request
        .params
        .clone()
        .ok_or_else(|| "missing connect params".to_string())
        .and_then(|value| {
            serde_json::from_value(value).map_err(|e| format!("invalid connect params: {e}"))
        }) {
        Ok(params) => params,
        Err(message) => {
            let response = ResponseFrame::err(request.id, ErrorShape::invalid_params(message));
            let _ = send_frame(&mut sender, &GatewayFrame::Res(response)).await;
            return;
        }
    };

    if !params.protocol_overlap(PROTOCOL_VERSION) {
        let response = ResponseFrame::err(
            request.id,
            ErrorShape::invalid_params(format!(
                "unsupported protocol range {}-{}; server speaks {PROTOCOL_VERSION}",
                params.min_protocol, params.max_protocol
            )),
        );
        let _ = send_frame(&mut sender, &GatewayFrame::Res(response)).await;
        return;
    }

    let grant = match state.auth.verify(params.auth.as_ref()) {
        Ok(grant) => grant,
        Err(error) => {
            tracing::warn!(client = %params.client.id, "connect rejected: {}", error.message);
            let response = ResponseFrame::err(request.id, error);
            let _ = send_frame(&mut sender, &GatewayFrame::Res(response)).await;
            return;
        }
    };

    let conn_id = Uuid::new_v4().to_string();
    tracing::info!(
        %conn_id,
        client = %params.client.id,
        version = %params.client.version,
        mode = %params.client.mode,
        auth = %grant.method,
        "client connected"
    );

    let hello = HelloOk::new(
        PROTOCOL_VERSION,
        ServerInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            build_id: None,
            boot_id: Some(state.boot_id.clone()),
            conn_id: conn_id.clone(),
        },
        Features {
            methods: GATEWAY_METHODS.iter().map(|m| m.to_string()).collect(),
            events: GATEWAY_EVENTS.iter().map(|e| e.to_string()).collect(),
            capabilities: Some(GATEWAY_SERVER_CAPS.iter().map(|c| c.to_string()).collect()),
        },
        state.snapshot(),
        HelloAuth {
            method: Some(grant.method.clone()),
            device_token: None,
            role: grant.role.clone(),
            scopes: grant.scopes.clone(),
            issued_at_ms: Some(now_ms() as u64),
        },
        Policy {
            max_payload: state.config.max_payload,
            ..Policy::default()
        },
    );

    let payload = match serde_json::to_value(hello) {
        Ok(payload) => payload,
        Err(error) => {
            tracing::error!(%error, "failed to encode hello-ok");
            return;
        }
    };
    let response = ResponseFrame::ok(request.id, payload);
    if send_frame(&mut sender, &GatewayFrame::Res(response))
        .await
        .is_err()
    {
        return;
    }

    state.events.publish(
        "presence",
        json!({
            "connectionId": conn_id,
            "clientId": params.client.id,
            "platform": params.client.platform,
            "mode": params.client.mode,
            "ts": now_ms(),
        }),
    );

    // One write queue, one writer: responses and events share the same FIFO, so
    // a burst of broadcasts can never starve request/response traffic.
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    let events_tx = tx.clone();
    let mut events = state.events.subscribe();
    let event_pump = tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(frame) => {
                    let Ok(text) = serde_json::to_string(&GatewayFrame::Event(frame)) else {
                        continue;
                    };
                    if events_tx.send(text).is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "event subscriber lagged");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    let tick_tx = tx.clone();
    let tick_interval_ms = Policy::default().tick_interval_ms;
    let tick_pump = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(tick_interval_ms));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        ticker.tick().await;
        loop {
            ticker.tick().await;
            let frame = GatewayFrame::Event(EventFrame::new("tick", json!({ "ts": now_ms() })));
            let Ok(text) = serde_json::to_string(&frame) else {
                continue;
            };
            if tick_tx.send(text).is_err() {
                break;
            }
        }
    });

    let writer = tokio::spawn(async move {
        while let Some(text) = rx.recv().await {
            if sender.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(incoming) = receiver.next().await {
        let text = match incoming {
            Ok(Message::Text(text)) => text.to_string(),
            Ok(Message::Close(_)) => break,
            Ok(Message::Binary(_)) => {
                tracing::warn!(%conn_id, "binary frames are not supported");
                break;
            }
            Ok(_) => continue,
            Err(error) => {
                tracing::debug!(%conn_id, %error, "socket read error");
                break;
            }
        };

        let request: RequestFrame = match GatewayFrame::parse(&text) {
            Ok(GatewayFrame::Req(request)) => request,
            Ok(_) => {
                tracing::debug!(%conn_id, "ignoring non-request frame from client");
                continue;
            }
            Err(error) => {
                tracing::debug!(%conn_id, %error, "ignoring malformed frame");
                continue;
            }
        };

        let state = state.clone();
        let grant = grant.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let response = dispatch(state, &grant, &request).await;
            let frame = GatewayFrame::Res(response);
            if let Ok(text) = serde_json::to_string(&frame) {
                let _ = tx.send(text);
            }
        });
    }

    drop(tx);
    event_pump.abort();
    tick_pump.abort();
    writer.abort();
    tracing::info!(%conn_id, "client disconnected");
}

async fn send_frame(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    frame: &GatewayFrame,
) -> Result<(), axum::Error> {
    let text = match serde_json::to_string(frame) {
        Ok(text) => text,
        Err(_) => return Ok(()),
    };
    sender.send(Message::Text(text.into())).await
}

async fn close_with_error(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    error: ErrorShape,
) -> Result<(), axum::Error> {
    let response = ResponseFrame::err("handshake", error);
    let _ = send_frame(sender, &GatewayFrame::Res(response)).await;
    sender.close().await
}
