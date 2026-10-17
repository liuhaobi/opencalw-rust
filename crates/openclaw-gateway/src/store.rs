//! SQLite-backed session and transcript store.
//!
//! The reference implementation keeps session state in SQLite
//! (`src/state/openclaw-state-db*.ts`). This crate stores the minimum durable
//! projection needed by the gateway: session rows plus append-only messages.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use openclaw_protocol::{ChatMessage, MessageRole, RunStatus, SessionKind, SessionRow};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS sessions (
    key         TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL,
    kind        TEXT NOT NULL DEFAULT 'direct',
    label       TEXT,
    agent_id    TEXT,
    created_at  REAL NOT NULL,
    updated_at  REAL NOT NULL,
    status      TEXT,
    archived    INTEGER NOT NULL DEFAULT 0,
    preview     TEXT,
    last_seq    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS messages (
    id          TEXT PRIMARY KEY,
    session_key TEXT NOT NULL REFERENCES sessions(key) ON DELETE CASCADE,
    role        TEXT NOT NULL,
    content     TEXT NOT NULL,
    created_at  REAL NOT NULL,
    seq         INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS messages_session_seq ON messages(session_key, seq);
"#;

pub struct Store {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn session_count(&self) -> anyhow::Result<u32> {
        let conn = self.conn.lock().unwrap();
        let count: u32 = conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn list_sessions(&self, archived: bool, limit: u32) -> anyhow::Result<Vec<SessionRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT key, session_id, kind, label, agent_id, created_at, updated_at, status,
                    archived, preview
             FROM sessions
             WHERE archived = ?1
             ORDER BY updated_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![archived as i64, limit as i64], map_session_row)?;
        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row?);
        }
        Ok(sessions)
    }

    pub fn get_session(&self, key: &str) -> anyhow::Result<Option<SessionRow>> {
        let conn = self.conn.lock().unwrap();
        let row = conn
            .query_row(
                "SELECT key, session_id, kind, label, agent_id, created_at, updated_at, status,
                        archived, preview
                 FROM sessions WHERE key = ?1",
                params![key],
                map_session_row,
            )
            .optional()?;
        Ok(row)
    }

    pub fn create_session(
        &self,
        key: Option<&str>,
        label: Option<&str>,
        kind: SessionKind,
        agent_id: Option<&str>,
    ) -> anyhow::Result<SessionRow> {
        let key = key.map(str::to_string).unwrap_or_else(|| {
            format!("agent:main:{}", &Uuid::new_v4().simple().to_string()[..12])
        });
        let session_id = Uuid::new_v4().to_string();
        let now = now_ms();
        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT OR IGNORE INTO sessions
                    (key, session_id, kind, label, agent_id, created_at, updated_at, status, archived)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, 0)",
                params![
                    key,
                    session_id,
                    kind_str(kind),
                    label,
                    agent_id,
                    now,
                    run_status_str(RunStatus::Done),
                ],
            )?;
        }
        self.get_session(&key)?
            .ok_or_else(|| anyhow::anyhow!("session vanished after insert"))
    }

    /// Ensures a session exists, creating it on first use.
    pub fn ensure_session(&self, key: &str) -> anyhow::Result<SessionRow> {
        if let Some(row) = self.get_session(key)? {
            return Ok(row);
        }
        self.create_session(Some(key), None, SessionKind::Direct, None)
    }

    pub fn delete_session(&self, key: &str) -> anyhow::Result<bool> {
        let conn = self.conn.lock().unwrap();
        let removed = conn.execute("DELETE FROM sessions WHERE key = ?1", params![key])?;
        Ok(removed > 0)
    }

    pub fn set_status(&self, key: &str, status: RunStatus) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sessions SET status = ?2, updated_at = ?3 WHERE key = ?1",
            params![key, run_status_str(status), now_ms()],
        )?;
        Ok(())
    }

    pub fn append_message(
        &self,
        session_key: &str,
        role: MessageRole,
        content: &str,
    ) -> anyhow::Result<ChatMessage> {
        self.ensure_session(session_key)?;
        let id = Uuid::new_v4().to_string();
        let now = now_ms();
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        let seq: i64 = tx.query_row(
            "SELECT last_seq + 1 FROM sessions WHERE key = ?1",
            params![session_key],
            |row| row.get(0),
        )?;
        tx.execute(
            "INSERT INTO messages (id, session_key, role, content, created_at, seq)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, session_key, role_str(role), content, now, seq],
        )?;
        tx.execute(
            "UPDATE sessions
             SET updated_at = ?2, last_seq = ?3, preview = ?4
             WHERE key = ?1",
            params![session_key, now, seq, preview_for(content)],
        )?;
        tx.commit()?;

        Ok(ChatMessage {
            id,
            session_key: session_key.to_string(),
            role,
            content: content.to_string(),
            created_at: now,
            seq: seq as u64,
        })
    }

    pub fn history(&self, session_key: &str, limit: u32) -> anyhow::Result<Vec<ChatMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, session_key, role, content, created_at, seq
             FROM messages WHERE session_key = ?1
             ORDER BY seq DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![session_key, limit as i64], |row| {
            let role: String = row.get(2)?;
            Ok(ChatMessage {
                id: row.get(0)?,
                session_key: row.get(1)?,
                role: parse_role(&role),
                content: row.get(3)?,
                created_at: row.get(4)?,
                seq: row.get::<_, i64>(5)? as u64,
            })
        })?;
        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }
        messages.reverse();
        Ok(messages)
    }
}

fn map_session_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
    let kind: String = row.get(2)?;
    let status: Option<String> = row.get(7)?;
    Ok(SessionRow {
        key: row.get(0)?,
        session_id: Some(row.get(1)?),
        kind: parse_kind(&kind),
        label: row.get(3)?,
        display_name: None,
        derived_title: None,
        last_message_preview: row.get(9)?,
        agent_id: row.get(4)?,
        created_at: Some(row.get(5)?),
        updated_at: Some(row.get(6)?),
        status: status.as_deref().map(parse_status),
        archived: Some(row.get::<_, i64>(8)? != 0),
    })
}

pub fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

fn preview_for(content: &str) -> String {
    let compact = content.replace('\n', " ");
    compact.chars().take(120).collect()
}

fn kind_str(kind: SessionKind) -> &'static str {
    match kind {
        SessionKind::Direct => "direct",
        SessionKind::Group => "group",
        SessionKind::Global => "global",
        SessionKind::Unknown => "unknown",
    }
}

fn parse_kind(value: &str) -> SessionKind {
    match value {
        "group" => SessionKind::Group,
        "global" => SessionKind::Global,
        "unknown" => SessionKind::Unknown,
        _ => SessionKind::Direct,
    }
}

fn role_str(role: MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

fn parse_role(value: &str) -> MessageRole {
    match value {
        "system" => MessageRole::System,
        "assistant" => MessageRole::Assistant,
        "tool" => MessageRole::Tool,
        _ => MessageRole::User,
    }
}

fn run_status_str(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Queued => "queued",
        RunStatus::Running => "running",
        RunStatus::Done => "done",
        RunStatus::Failed => "failed",
        RunStatus::Interrupted => "interrupted",
        RunStatus::Killed => "killed",
        RunStatus::Timeout => "timeout",
    }
}

fn parse_status(value: &str) -> RunStatus {
    match value {
        "queued" => RunStatus::Queued,
        "running" => RunStatus::Running,
        "failed" => RunStatus::Failed,
        "interrupted" => RunStatus::Interrupted,
        "killed" => RunStatus::Killed,
        "timeout" => RunStatus::Timeout,
        _ => RunStatus::Done,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (Store, tempdir::Guard) {
        let dir = tempdir::Guard::new();
        let store = Store::open(&dir.path().join("state.db")).unwrap();
        (store, dir)
    }

    /// Minimal temp directory helper so the crate needs no extra dev-dependency.
    mod tempdir {
        use std::path::{Path, PathBuf};

        pub struct Guard(PathBuf);

        impl Guard {
            pub fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "openclaw-rust-test-{}",
                    uuid::Uuid::new_v4()
                ));
                std::fs::create_dir_all(&path).unwrap();
                Self(path)
            }

            pub fn path(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for Guard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    #[test]
    fn create_and_list_sessions() {
        let (store, _guard) = temp_store();
        let row = store
            .create_session(Some("agent:main:test"), Some("Test"), SessionKind::Direct, None)
            .unwrap();
        assert_eq!(row.key, "agent:main:test");
        let rows = store.list_sessions(false, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label.as_deref(), Some("Test"));
    }

    #[test]
    fn appends_monotonic_messages() {
        let (store, _guard) = temp_store();
        let first = store
            .append_message("agent:main:test", MessageRole::User, "hello")
            .unwrap();
        let second = store
            .append_message("agent:main:test", MessageRole::Assistant, "hi")
            .unwrap();
        assert_eq!(first.seq, 1);
        assert_eq!(second.seq, 2);
        let history = store.history("agent:main:test", 10).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "hello");
    }

    #[test]
    fn deleting_session_cascades_messages() {
        let (store, _guard) = temp_store();
        store
            .append_message("agent:main:test", MessageRole::User, "hello")
            .unwrap();
        assert!(store.delete_session("agent:main:test").unwrap());
        assert!(store.history("agent:main:test", 10).unwrap().is_empty());
    }
}
