//! OpenClaw gateway wire protocol, ported from
//! `packages/gateway-protocol` of the TypeScript reference implementation.
//!
//! The protocol is a JSON frame protocol carried over a single WebSocket:
//! `req` frames from clients, `res` and `event` frames from the server.

pub mod connect;
pub mod error;
pub mod frames;
pub mod session;
pub mod snapshot;

pub use connect::{
    ClientInfo, ConnectAuth, ConnectParams, DeviceProof, Features, HelloAuth, HelloOk, Policy,
    ServerInfo,
};
pub use error::ErrorShape;
pub use frames::{EventFrame, GatewayFrame, RequestFrame, ResponseFrame, StateVersion};
pub use session::{
    ChatMessage, ChatSendParams, MessageRole, RunStatus, SessionKind, SessionRow,
    SessionsCreateParams, SessionsHistoryResult, SessionsKeyParams, SessionsListParams,
    SessionsListResult,
};
pub use snapshot::{HealthSnapshot, PresenceEntry, Snapshot};

/// Current gateway protocol version emitted by modern clients and servers.
pub const PROTOCOL_VERSION: u32 = 4;
/// Lowest general client protocol version accepted by the gateway.
pub const MIN_CLIENT_PROTOCOL_VERSION: u32 = 4;
/// Lowest authenticated node protocol version accepted by the gateway.
pub const MIN_NODE_PROTOCOL_VERSION: u32 = 3;
/// Default control-plane port.
pub const DEFAULT_GATEWAY_PORT: u16 = 18_789;

/// Server capabilities advertised in `hello-ok`.
pub const GATEWAY_SERVER_CAPS: &[&str] = &["sessions", "chat", "streaming", "openai-compat"];

/// RPC methods implemented by this gateway build.
pub const GATEWAY_METHODS: &[&str] = &[
    "health",
    "status",
    "config.get",
    "sessions.list",
    "sessions.create",
    "sessions.get",
    "sessions.history",
    "sessions.delete",
    "chat.send",
    "chat.abort",
];

/// Event names this gateway may push.
pub const GATEWAY_EVENTS: &[&str] = &[
    "chat",
    "session.message",
    "sessions.changed",
    "tick",
    "health",
    "presence",
];
