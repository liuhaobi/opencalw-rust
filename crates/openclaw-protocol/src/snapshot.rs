//! Gateway state snapshot sent with `hello-ok` and later state events.

use serde::{Deserialize, Serialize};

use crate::frames::StateVersion;

/// One gateway-visible presence record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PresenceEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub ts: u64,
}

/// Health block. Every field is optional because hello snapshots are emitted
/// before the asynchronous health producer has populated its cache.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HealthSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ts: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub presence: Vec<PresenceEntry>,
    pub health: HealthSnapshot,
    pub state_version: StateVersion,
    pub uptime_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_mode: Option<String>,
}

impl Snapshot {
    pub fn empty(uptime_ms: u64) -> Self {
        Self {
            presence: Vec::new(),
            health: HealthSnapshot::default(),
            state_version: StateVersion::default(),
            uptime_ms,
            config_path: None,
            state_dir: None,
            auth_mode: None,
        }
    }
}
