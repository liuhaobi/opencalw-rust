//! Shared application state.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use openclaw_protocol::Snapshot;
use uuid::Uuid;

use crate::auth::Authenticator;
use crate::config::Config;
use crate::events::EventHub;
use crate::llm::LlmClient;
use crate::store::{now_ms, Store};

pub struct AppState {
    pub config: Config,
    pub auth: Authenticator,
    pub store: Store,
    pub events: EventHub,
    pub llm: LlmClient,
    pub started_at: Instant,
    pub boot_id: String,
    pub aborts: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl AppState {
    pub fn new(config: Config) -> anyhow::Result<Arc<Self>> {
        let store = Store::open(&config.db_path)?;
        let auth = Authenticator::new(config.auth.clone());
        let llm = LlmClient::new(config.model.clone())?;

        Ok(Arc::new(Self {
            config,
            auth,
            store,
            events: EventHub::new(),
            llm,
            started_at: Instant::now(),
            boot_id: Uuid::new_v4().to_string(),
            aborts: Mutex::new(HashMap::new()),
        }))
    }

    pub fn uptime_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = Snapshot::empty(self.uptime_ms());
        snapshot.health.ts = Some(now_ms() as u64);
        snapshot.health.ok = Some(true);
        snapshot.config_path = Some(self.config.db_path.to_string_lossy().to_string());
        snapshot.state_dir = Some(self.config.state_dir.to_string_lossy().to_string());
        snapshot.auth_mode = Some(self.config.auth.mode.as_str().to_string());
        snapshot
    }

    pub fn register_run(&self, run_id: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.aborts
            .lock()
            .unwrap()
            .insert(run_id.to_string(), flag.clone());
        flag
    }

    pub fn abort_run(&self, run_id: &str) -> bool {
        match self.aborts.lock().unwrap().get(run_id) {
            Some(flag) => {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                true
            }
            None => false,
        }
    }

    pub fn finish_run(&self, run_id: &str) {
        self.aborts.lock().unwrap().remove(run_id);
    }
}
