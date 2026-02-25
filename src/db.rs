use crate::config::{AppPaths, workspace_slug};
use crate::domain::{EventPayload, StoredEvent};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use rust_decimal::Decimal;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct StoredBudget {
    pub id: Uuid,
    pub name: String,
    pub amount: Decimal,
    pub commodity: String,
    pub month: Option<String>,
    pub category: Option<String>,
    pub account: Option<String>,
    pub provider: Option<String>,
    pub created_at: DateTime<Utc>,
}

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

            CREATE TABLE IF NOT EXISTS rates (
                provider TEXT NOT NULL,
                base TEXT NOT NULL,
                quote TEXT NOT NULL,
                as_of TEXT NOT NULL,
                rate TEXT NOT NULL,
                PRIMARY KEY (provider, base, quote, as_of)
            );

            CREATE INDEX IF NOT EXISTS idx_rates_lookup ON rates(provider, base, quote, as_of);

            CREATE TABLE IF NOT EXISTS budgets (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                amount TEXT NOT NULL,
                commodity TEXT NOT NULL,
                month TEXT,
                category TEXT,
                account TEXT,
                provider TEXT,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_budgets_month ON budgets(month);
            CREATE INDEX IF NOT EXISTS idx_budgets_category ON budgets(category);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_budgets_name ON budgets(name);
            "#,
        )?;
        Ok(())
    }

    pub fn set_rate(
        &self,
        provider: &str,
        base: &str,
        quote: &str,
        as_of: DateTime<Utc>,
        rate: Decimal,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO rates (provider, base, quote, as_of, rate)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(provider, base, quote, as_of) DO UPDATE SET rate = excluded.rate
            "#,
            params![provider, base, quote, as_of.to_rfc3339(), rate.to_string(),],
        )?;
        Ok(())
    }

    /// Returns the latest known rate at or before `as_of`.
    pub fn get_rate_as_of(
        &self,
        provider: &str,
        base: &str,
        quote: &str,
        as_of: DateTime<Utc>,
    ) -> Result<Option<(DateTime<Utc>, Decimal)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT as_of, rate
            FROM rates
            WHERE provider = ?1
              AND base = ?2
              AND quote = ?3
              AND as_of <= ?4
            ORDER BY as_of DESC
            LIMIT 1
            "#,
        )?;

        let mut rows = stmt.query(params![provider, base, quote, as_of.to_rfc3339()])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };

        let as_of_raw: String = row.get(0)?;
        let rate_raw: String = row.get(1)?;

        let as_of = DateTime::parse_from_rfc3339(&as_of_raw)
            .context("Invalid as_of in rates table")?
            .with_timezone(&Utc);
        let rate = rate_raw
            .parse::<Decimal>()
            .context("Invalid decimal rate in rates table")?;

        Ok(Some((as_of, rate)))
    }

    pub fn list_rates(
        &self,
        provider: &str,
        base: &str,
        quote: &str,
        limit: usize,
    ) -> Result<Vec<(DateTime<Utc>, Decimal)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT as_of, rate
            FROM rates
            WHERE provider = ?1
              AND base = ?2
              AND quote = ?3
            ORDER BY as_of DESC
            LIMIT ?4
            "#,
        )?;

        let rows = stmt.query_map(params![provider, base, quote, limit as i64], |row| {
            let as_of_raw: String = row.get(0)?;
            let rate_raw: String = row.get(1)?;
            Ok((as_of_raw, rate_raw))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (as_of_raw, rate_raw) = row?;
            let as_of = DateTime::parse_from_rfc3339(&as_of_raw)
                .context("Invalid as_of in rates table")?
                .with_timezone(&Utc);
            let rate = rate_raw
                .parse::<Decimal>()
                .context("Invalid decimal rate in rates table")?;
            out.push((as_of, rate));
        }
        Ok(out)
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

    pub fn insert_budget(&self, budget: &StoredBudget) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO budgets (id, name, amount, commodity, month, category, account, provider, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                budget.id.to_string(),
                budget.name,
                budget.amount.to_string(),
                budget.commodity,
                budget.month,
                budget.category,
                budget.account,
                budget.provider,
                budget.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn list_budgets(&self) -> Result<Vec<StoredBudget>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, amount, commodity, month, category, account, provider, created_at
            FROM budgets
            ORDER BY created_at ASC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let name: String = row.get(1)?;
            let amount: String = row.get(2)?;
            let commodity: String = row.get(3)?;
            let month: Option<String> = row.get(4)?;
            let category: Option<String> = row.get(5)?;
            let account: Option<String> = row.get(6)?;
            let provider: Option<String> = row.get(7)?;
            let created_at: String = row.get(8)?;
            Ok((
                id,
                name,
                amount,
                commodity,
                month,
                category,
                account,
                provider,
                created_at,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (
                id,
                name,
                amount,
                commodity,
                month,
                category,
                account,
                provider,
                created_at,
            ) = row?;
            let id = Uuid::parse_str(&id).context("Invalid budget UUID")?;
            let amount = amount
                .parse::<Decimal>()
                .context("Invalid decimal amount in budgets table")?;
            let created_at = DateTime::parse_from_rfc3339(&created_at)
                .context("Invalid created_at in budgets table")?
                .with_timezone(&Utc);

            out.push(StoredBudget {
                id,
                name,
                amount,
                commodity,
                month,
                category,
                account,
                provider,
                created_at,
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
