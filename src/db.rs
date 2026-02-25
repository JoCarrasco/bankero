use crate::config::{workspace_slug, AppPaths};
use crate::domain::{EventPayload, StoredEvent};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(paths: &AppPaths, workspace: &str) -> Result<(Self, PathBuf)> {
        let slug = workspace_slug(workspace);
        let ws_dir = paths.data_dir.join("workspaces").join(slug);
        fs::create_dir_all(&ws_dir)
            .with_context(|| format!("Failed to create workspace dir {}", ws_dir.display()))?;

        let db_path = ws_dir.join("bankero.sqlite3");
        let conn = Connection::open(&db_path)
            .with_context(|| format!("Failed to open DB {}", db_path.display()))?;

        let db = Self { conn };
        db.migrate()?;
        Ok((db, db_path))
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS events (
                id TEXT PRIMARY KEY,
                action TEXT NOT NULL,
                created_at TEXT NOT NULL,
                effective_at TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_events_effective_at ON events(effective_at);
            CREATE INDEX IF NOT EXISTS idx_events_action ON events(action);
            "#,
        )?;
        Ok(())
    }

    pub fn insert_event(&self, id: Uuid, payload: &EventPayload) -> Result<()> {
        let json = serde_json::to_string(payload)?;
        self.conn.execute(
            "INSERT INTO events (id, action, created_at, effective_at, payload_json) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id.to_string(),
                payload.action,
                payload.created_at.to_rfc3339(),
                payload.effective_at.to_rfc3339(),
                json
            ],
        )?;
        Ok(())
    }

    pub fn list_events(&self) -> Result<Vec<StoredEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, action, created_at, effective_at, payload_json FROM events ORDER BY effective_at ASC, created_at ASC",
        )?;

        let mut out = Vec::new();
        let rows = stmt.query_map([], |row| {
            let id_str: String = row.get(0)?;
            let action: String = row.get(1)?;
            let created_at: String = row.get(2)?;
            let effective_at: String = row.get(3)?;
            let payload_json: String = row.get(4)?;

            Ok((id_str, action, created_at, effective_at, payload_json))
        })?;

        for row in rows {
            let (id_str, action, created_at, effective_at, payload_json) = row?;
            let event_id = Uuid::parse_str(&id_str).context("Invalid event UUID in DB")?;
            let created_at = DateTime::parse_from_rfc3339(&created_at)
                .context("Invalid created_at in DB")?
                .with_timezone(&Utc);
            let effective_at = DateTime::parse_from_rfc3339(&effective_at)
                .context("Invalid effective_at in DB")?
                .with_timezone(&Utc);
            let payload: EventPayload =
                serde_json::from_str(&payload_json).context("Invalid payload_json in DB")?;

            out.push(StoredEvent {
                event_id,
                action,
                created_at,
                effective_at,
                payload,
            });
        }

        Ok(out)
    }
}

pub fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create dir {}", parent.display()))?;
    }
    Ok(())
}
