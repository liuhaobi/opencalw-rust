//! Event fan-out: every socket subscribes and receives sequenced `event` frames.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use openclaw_protocol::EventFrame;
use serde_json::Value;
use tokio::sync::broadcast;

const CHANNEL_CAPACITY: usize = 1024;

#[derive(Clone)]
pub struct EventHub {
    tx: broadcast::Sender<EventFrame>,
    seq: Arc<AtomicU64>,
}

impl Default for EventHub {
    fn default() -> Self {
        Self::new()
    }
}

impl EventHub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            tx,
            seq: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EventFrame> {
        self.tx.subscribe()
    }

    /// Publishes an event and returns the assigned sequence number.
    pub fn publish(&self, event: impl Into<String>, payload: Value) -> u64 {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let frame = EventFrame {
            event: event.into(),
            payload: Some(payload),
            seq: Some(seq),
            state_version: None,
        };
        // A send error only means nobody is subscribed; that is not a failure.
        let _ = self.tx.send(frame);
        seq
    }

    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn assigns_monotonic_sequence_numbers() {
        let hub = EventHub::new();
        let mut rx = hub.subscribe();
        assert_eq!(hub.publish("tick", json!({"ts": 1})), 1);
        assert_eq!(hub.publish("tick", json!({"ts": 2})), 2);
        let first = rx.recv().await.unwrap();
        let second = rx.recv().await.unwrap();
        assert_eq!(first.seq, Some(1));
        assert_eq!(second.seq, Some(2));
    }
}
