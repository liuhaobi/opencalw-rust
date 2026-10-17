//! Top-level wire frames.
//!
//! Mirrors `packages/gateway-protocol/src/schema/frames.ts` in the TypeScript
//! reference implementation. Three frame kinds share one WebSocket connection:
//! the client sends `req`, the server answers `res` and may push `event` at any
//! time.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::ErrorShape;

/// Client request envelope. `method` selects the payload validator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequestFrame {
    pub id: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traceparent: Option<String>,
}

/// Server response envelope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResponseFrame {
    pub id: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorShape>,
}

impl ResponseFrame {
    pub fn ok(id: impl Into<String>, payload: Value) -> Self {
        Self {
            id: id.into(),
            ok: true,
            payload: Some(payload),
            error: None,
        }
    }

    pub fn err(id: impl Into<String>, error: ErrorShape) -> Self {
        Self {
            id: id.into(),
            ok: false,
            payload: None,
            error: Some(error),
        }
    }
}

/// Server-pushed event envelope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EventFrame {
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_version: Option<StateVersion>,
}

impl EventFrame {
    pub fn new(event: impl Into<String>, payload: Value) -> Self {
        Self {
            event: event.into(),
            payload: Some(payload),
            seq: None,
            state_version: None,
        }
    }
}

/// Monotonic counters for snapshot-backed collections.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateVersion {
    pub presence: u64,
    pub health: u64,
}

/// Discriminated union of every frame a socket may carry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum GatewayFrame {
    #[serde(rename = "req")]
    Req(RequestFrame),
    #[serde(rename = "res")]
    Res(ResponseFrame),
    #[serde(rename = "event")]
    Event(EventFrame),
}

impl GatewayFrame {
    pub fn parse(raw: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_request_frame() {
        let frame = GatewayFrame::parse(r#"{"type":"req","id":"1","method":"health"}"#).unwrap();
        match frame {
            GatewayFrame::Req(req) => {
                assert_eq!(req.id, "1");
                assert_eq!(req.method, "health");
                assert!(req.params.is_none());
            }
            other => panic!("unexpected frame: {other:?}"),
        }
    }

    #[test]
    fn serializes_event_frame_camel_case() {
        let mut frame = EventFrame::new("tick", json!({ "ts": 1 }));
        frame.seq = Some(7);
        let value = serde_json::to_value(GatewayFrame::Event(frame)).unwrap();
        assert_eq!(value["type"], "event");
        assert_eq!(value["event"], "tick");
        assert_eq!(value["seq"], 7);
        assert!(value.get("stateVersion").is_none());
    }

    #[test]
    fn response_error_roundtrip() {
        let frame = ResponseFrame::err(
            "9",
            ErrorShape {
                code: "not_found".into(),
                message: "unknown method".into(),
                details: None,
                retryable: None,
                retry_after_ms: None,
            },
        );
        let value = serde_json::to_value(GatewayFrame::Res(frame.clone())).unwrap();
        assert_eq!(value["type"], "res");
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "not_found");
        assert_eq!(
            GatewayFrame::parse(&value.to_string()).unwrap(),
            GatewayFrame::Res(frame)
        );
    }
}
