//! Connect handshake: client hello (`ConnectParams`) and server accept (`HelloOk`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::snapshot::Snapshot;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_id: Option<String>,
    pub platform: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_zone: Option<String>,
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
}

/// Device proof supplied by paired clients.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceProof {
    pub id: String,
    pub public_key: String,
    pub signature: String,
    pub signed_at: u64,
    pub nonce: String,
}

/// Credential material accepted during connect.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectAuth {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bootstrap_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_runtime_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_runtime_identity_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectParams {
    pub min_protocol: u32,
    pub max_protocol: u32,
    pub client: ClientInfo,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commands: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computer_use: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_catalog: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_runs: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<DeviceProof>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<ConnectAuth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
}

impl ConnectParams {
    /// True when the client speaks a protocol version this server understands.
    pub fn protocol_overlap(&self, server_version: u32) -> bool {
        self.min_protocol <= server_version && server_version <= self.max_protocol
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot_id: Option<String>,
    pub conn_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Features {
    pub methods: Vec<String>,
    pub events: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HelloAuth {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_token: Option<String>,
    pub role: String,
    pub scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issued_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    pub max_payload: u64,
    pub max_buffered_bytes: u64,
    pub tick_interval_ms: u64,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            max_payload: 4 * 1024 * 1024,
            max_buffered_bytes: 50 * 1024 * 1024,
            tick_interval_ms: 30_000,
        }
    }
}

/// Server accept frame payload (sent as `type: "hello-ok"`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HelloOk {
    #[serde(rename = "type")]
    pub frame_type: String,
    pub protocol: u32,
    pub server: ServerInfo,
    pub features: Features,
    pub snapshot: Snapshot,
    pub auth: HelloAuth,
    pub policy: Policy,
}

impl HelloOk {
    pub fn new(
        protocol: u32,
        server: ServerInfo,
        features: Features,
        snapshot: Snapshot,
        auth: HelloAuth,
        policy: Policy,
    ) -> Self {
        Self {
            frame_type: "hello-ok".to_string(),
            protocol,
            server,
            features,
            snapshot,
            auth,
            policy,
        }
    }
}
