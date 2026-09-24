//! SQLite 持久化：sessions / messages / settings。
//! 迁移用 `PRAGMA user_version` + 有序 SQL 数组，零依赖。

use crate::error::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

const MIGRATIONS: &[&str] = &[
    // v1
    "CREATE TABLE sessions (
        id         TEXT PRIMARY KEY,
        title      TEXT NOT NULL,
        workspace  TEXT NOT NULL,
        created_at INTEGER NOT NULL
     );
     CREATE TABLE messages (
        id           TEXT PRIMARY KEY,
        session_id   TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
        role         TEXT NOT NULL,
        content_json TEXT NOT NULL,
        created_at   INTEGER NOT NULL
     );
     CREATE INDEX messages_session ON messages(session_id, created_at);
     CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
     );",
    // v2 追加在此
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub workspace: String,
    pub created_at: i64,
}

/// `content` 是 OpenAI 消息格式的完整 JSON（含 tool_calls / tool_call_id 等）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: serde_json::Value,
    pub created_at: i64,
}

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        Self::init(Connection::open(path)?)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        let conn = self.conn.lock().unwrap();
        let mut st = conn.prepare(
            "SELECT id, title, workspace, created_at FROM sessions ORDER BY created_at DESC",
        )?;
        let rows = st.query_map([], |r| {
            Ok(Session {
                id: r.get(0)?,
                title: r.get(1)?,
                workspace: r.get(2)?,
                created_at: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn create_session(&self, title: &str, workspace: &str) -> Result<Session> {
        let s = Session {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.to_string(),
            workspace: workspace.to_string(),
            created_at: now(),
        };
        self.conn.lock().unwrap().execute(
            "INSERT INTO sessions (id, title, workspace, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![s.id, s.title, s.workspace, s.created_at],
        )?;
        Ok(s)
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let conn = self.conn.lock().unwrap();
        let mut st =
            conn.prepare("SELECT id, title, workspace, created_at FROM sessions WHERE id = ?1")?;
        let mut rows = st.query_map([id], |r| {
            Ok(Session {
                id: r.get(0)?,
                title: r.get(1)?,
                workspace: r.get(2)?,
                created_at: r.get(3)?,
            })
        })?;
        Ok(rows.next().transpose()?)
    }

    pub fn rename_session(&self, id: &str, title: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE sessions SET title = ?2 WHERE id = ?1",
            params![id, title],
        )?;
        Ok(())
    }

    pub fn delete_session(&self, id: &str) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM sessions WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn get_messages(&self, session_id: &str) -> Result<Vec<Message>> {
        let conn = self.conn.lock().unwrap();
        let mut st = conn.prepare(
            "SELECT id, session_id, role, content_json, created_at FROM messages
             WHERE session_id = ?1 ORDER BY created_at, rowid",
        )?;
        let rows = st.query_map([session_id], |r| {
            let raw: String = r.get(3)?;
            Ok(Message {
                id: r.get(0)?,
                session_id: r.get(1)?,
                role: r.get(2)?,
                content: serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null),
                created_at: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn add_message(
        &self,
        session_id: &str,
        role: &str,
        content: &serde_json::Value,
    ) -> Result<Message> {
        let m = Message {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            role: role.to_string(),
            content: content.clone(),
            created_at: now(),
        };
        self.conn.lock().unwrap().execute(
            "INSERT INTO messages (id, session_id, role, content_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![m.id, m.session_id, m.role, serde_json::to_string(content)?, m.created_at],
        )?;
        Ok(m)
    }

    #[cfg(test)]
    fn user_version(&self) -> i64 {
        self.conn
            .lock()
            .unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap()
    }
}

fn migrate(conn: &Connection) -> Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn migrate_is_idempotent() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(s.user_version(), MIGRATIONS.len() as i64);
        migrate(&s.conn.lock().unwrap()).unwrap();
        assert_eq!(s.user_version(), MIGRATIONS.len() as i64);
    }

    #[test]
    fn message_json_roundtrip_and_cascade() {
        let s = Store::open_in_memory().unwrap();
        let sess = s.create_session("test", "/tmp").unwrap();
        let content = json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{"id": "call_1", "type": "function",
                            "function": {"name": "read_file", "arguments": "{\"path\":\"a.txt\"}"}}]
        });
        s.add_message(&sess.id, "assistant", &content).unwrap();
        s.add_message(
            &sess.id,
            "tool",
            &json!({"role":"tool","tool_call_id":"call_1","content":"hi"}),
        )
        .unwrap();
        let msgs = s.get_messages(&sess.id).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content, content);
        assert_eq!(msgs[1].role, "tool");

        assert_eq!(s.list_sessions().unwrap().len(), 1);
        s.delete_session(&sess.id).unwrap();
        assert!(s.get_messages(&sess.id).unwrap().is_empty());
        assert!(s.get_session(&sess.id).unwrap().is_none());
    }
}
