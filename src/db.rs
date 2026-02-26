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
pub struct StoredRate {
    pub provider: String,
    pub base: String,
    pub quote: String,
    pub as_of: DateTime<Utc>,
    pub rate: Decimal,
}

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
    pub auto_reserve_from: Option<String>,
    pub auto_reserve_until_amount: Option<Decimal>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct StoredPiggy {
    pub id: Uuid,
    pub name: String,
    pub target_amount: Decimal,
    pub commodity: String,
    pub from_account: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct StoredPiggyFund {
    pub id: Uuid,
    pub piggy_id: Uuid,
    pub amount: Decimal,
    pub effective_at: DateTime<Utc>,
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

            CREATE TABLE IF NOT EXISTS piggies (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                target_amount TEXT NOT NULL,
                commodity TEXT NOT NULL,
                from_account TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_piggies_name ON piggies(name);
            CREATE INDEX IF NOT EXISTS idx_piggies_from_account ON piggies(from_account);

            CREATE TABLE IF NOT EXISTS piggy_funds (
                id TEXT PRIMARY KEY,
                piggy_id TEXT NOT NULL,
                amount TEXT NOT NULL,
                effective_at TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY(piggy_id) REFERENCES piggies(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_piggy_funds_piggy_id ON piggy_funds(piggy_id);
            CREATE INDEX IF NOT EXISTS idx_piggy_funds_effective_at ON piggy_funds(effective_at);
            "#,
        )?;

        // Additive migrations for budgets table.
        // SQLite doesn't support IF NOT EXISTS for columns, so ignore duplicate-column errors.
        add_column_if_missing(&self.conn, "budgets", "auto_reserve_from", "TEXT")?;
        add_column_if_missing(&self.conn, "budgets", "auto_reserve_until_amount", "TEXT")?;
        Ok(())
    }

    pub fn insert_piggy(&self, piggy: &StoredPiggy) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO piggies (id, name, target_amount, commodity, from_account, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                piggy.id.to_string(),
                piggy.name,
                piggy.target_amount.to_string(),
                piggy.commodity,
                piggy.from_account,
                piggy.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_piggy_by_name(&self, name: &str) -> Result<Option<StoredPiggy>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, target_amount, commodity, from_account, created_at
            FROM piggies
            WHERE name = ?1
            LIMIT 1
            "#,
        )?;

        let mut rows = stmt.query(params![name])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };

        let id: String = row.get(0)?;
        let name: String = row.get(1)?;
        let target_amount: String = row.get(2)?;
        let commodity: String = row.get(3)?;
        let from_account: String = row.get(4)?;
        let created_at: String = row.get(5)?;

        let id = Uuid::parse_str(&id).context("Invalid piggy UUID")?;
        let target_amount = target_amount
            .parse::<Decimal>()
            .context("Invalid decimal target_amount in piggies table")?;
        let created_at = DateTime::parse_from_rfc3339(&created_at)
            .context("Invalid created_at in piggies table")?
            .with_timezone(&Utc);

        Ok(Some(StoredPiggy {
            id,
            name,
            target_amount,
            commodity,
            from_account,
            created_at,
        }))
    }

    pub fn list_piggies(&self) -> Result<Vec<StoredPiggy>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, target_amount, commodity, from_account, created_at
            FROM piggies
            ORDER BY created_at ASC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let name: String = row.get(1)?;
            let target_amount: String = row.get(2)?;
            let commodity: String = row.get(3)?;
            let from_account: String = row.get(4)?;
            let created_at: String = row.get(5)?;
            Ok((id, name, target_amount, commodity, from_account, created_at))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (id, name, target_amount, commodity, from_account, created_at) = row?;
            let id = Uuid::parse_str(&id).context("Invalid piggy UUID")?;
            let target_amount = target_amount
                .parse::<Decimal>()
                .context("Invalid decimal target_amount in piggies table")?;
            let created_at = DateTime::parse_from_rfc3339(&created_at)
                .context("Invalid created_at in piggies table")?
                .with_timezone(&Utc);

            out.push(StoredPiggy {
                id,
                name,
                target_amount,
                commodity,
                from_account,
                created_at,
            });
        }
        Ok(out)
    }

    pub fn insert_piggy_fund(&self, fund: &StoredPiggyFund) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO piggy_funds (id, piggy_id, amount, effective_at, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                fund.id.to_string(),
                fund.piggy_id.to_string(),
                fund.amount.to_string(),
                fund.effective_at.to_rfc3339(),
                fund.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn piggy_funded_total(&self, piggy_id: Uuid) -> Result<Decimal> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT amount
            FROM piggy_funds
            WHERE piggy_id = ?1
            ORDER BY effective_at ASC, created_at ASC
            "#,
        )?;

        let rows = stmt.query_map(params![piggy_id.to_string()], |row| {
            let amount: String = row.get(0)?;
            Ok(amount)
        })?;

        let mut total = Decimal::ZERO;
        for row in rows {
            let amount = row?
                .parse::<Decimal>()
                .context("Invalid decimal amount in piggy_funds table")?;
            total += amount;
        }
        Ok(total)
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

    pub fn list_latest_rates_for_provider(
        &self,
        provider: &str,
        limit: usize,
    ) -> Result<Vec<(String, String, DateTime<Utc>, Decimal)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT r.base, r.quote, r.as_of, r.rate
            FROM rates r
            WHERE r.provider = ?1
              AND r.as_of = (
                SELECT MAX(r2.as_of)
                FROM rates r2
                WHERE r2.provider = r.provider
                  AND r2.base = r.base
                  AND r2.quote = r.quote
              )
            ORDER BY r.base ASC, r.quote ASC
            LIMIT ?2
            "#,
        )?;

        let rows = stmt.query_map(params![provider, limit as i64], |row| {
            let base: String = row.get(0)?;
            let quote: String = row.get(1)?;
            let as_of_raw: String = row.get(2)?;
            let rate_raw: String = row.get(3)?;
            Ok((base, quote, as_of_raw, rate_raw))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (base, quote, as_of_raw, rate_raw) = row?;
            let as_of = DateTime::parse_from_rfc3339(&as_of_raw)
                .context("Invalid as_of in rates table")?
                .with_timezone(&Utc);
            let rate = rate_raw
                .parse::<Decimal>()
                .context("Invalid decimal rate in rates table")?;
            out.push((base, quote, as_of, rate));
        }
        Ok(out)
    }

    pub fn list_latest_rates_for_base(
        &self,
        provider: &str,
        base: &str,
        limit: usize,
    ) -> Result<Vec<(String, String, DateTime<Utc>, Decimal)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT r.base, r.quote, r.as_of, r.rate
            FROM rates r
            WHERE r.provider = ?1
              AND r.base = ?2
              AND r.as_of = (
                SELECT MAX(r2.as_of)
                FROM rates r2
                WHERE r2.provider = r.provider
                  AND r2.base = r.base
                  AND r2.quote = r.quote
              )
            ORDER BY r.quote ASC
            LIMIT ?3
            "#,
        )?;

        let rows = stmt.query_map(params![provider, base, limit as i64], |row| {
            let base: String = row.get(0)?;
            let quote: String = row.get(1)?;
            let as_of_raw: String = row.get(2)?;
            let rate_raw: String = row.get(3)?;
            Ok((base, quote, as_of_raw, rate_raw))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (base, quote, as_of_raw, rate_raw) = row?;
            let as_of = DateTime::parse_from_rfc3339(&as_of_raw)
                .context("Invalid as_of in rates table")?
                .with_timezone(&Utc);
            let rate = rate_raw
                .parse::<Decimal>()
                .context("Invalid decimal rate in rates table")?;
            out.push((base, quote, as_of, rate));
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

    /// Inserts an event if it does not exist yet.
    /// Returns true if inserted, false if it already existed.
    pub fn insert_event_ignore(&self, id: Uuid, payload: &EventPayload) -> Result<bool> {
        let json = serde_json::to_string(payload)?;
        let affected = self.conn.execute(
            "INSERT OR IGNORE INTO events (id, action, created_at, effective_at, payload_json) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id.to_string(),
                payload.action,
                payload.created_at.to_rfc3339(),
                payload.effective_at.to_rfc3339(),
                json
            ],
        )?;
        Ok(affected > 0)
    }

    pub fn count_events(&self) -> Result<i64> {
        let mut stmt = self.conn.prepare("SELECT COUNT(*) FROM events")?;
        let count: i64 = stmt.query_row([], |row| row.get(0))?;
        Ok(count)
    }

    pub fn count_rates(&self) -> Result<i64> {
        let mut stmt = self.conn.prepare("SELECT COUNT(*) FROM rates")?;
        let count: i64 = stmt.query_row([], |row| row.get(0))?;
        Ok(count)
    }

    pub fn list_all_rates(&self) -> Result<Vec<StoredRate>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT provider, base, quote, as_of, rate
            FROM rates
            ORDER BY provider ASC, base ASC, quote ASC, as_of ASC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            let provider: String = row.get(0)?;
            let base: String = row.get(1)?;
            let quote: String = row.get(2)?;
            let as_of_raw: String = row.get(3)?;
            let rate_raw: String = row.get(4)?;
            Ok((provider, base, quote, as_of_raw, rate_raw))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (provider, base, quote, as_of_raw, rate_raw) = row?;
            let as_of = DateTime::parse_from_rfc3339(&as_of_raw)
                .context("Invalid as_of in rates table")?
                .with_timezone(&Utc);
            let rate = rate_raw
                .parse::<Decimal>()
                .context("Invalid decimal rate in rates table")?;
            out.push(StoredRate {
                provider,
                base,
                quote,
                as_of,
                rate,
            });
        }
        Ok(out)
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
            INSERT INTO budgets (id, name, amount, commodity, month, category, account, provider, auto_reserve_from, auto_reserve_until_amount, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
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
                budget.auto_reserve_from,
                budget.auto_reserve_until_amount.map(|d| d.to_string()),
                budget.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_budget_by_name(&self, name: &str) -> Result<Option<StoredBudget>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, amount, commodity, month, category, account, provider, auto_reserve_from, auto_reserve_until_amount, created_at
            FROM budgets
            WHERE name = ?1
            LIMIT 1
            "#,
        )?;

        let mut rows = stmt.query(params![name])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };

        let id: String = row.get(0)?;
        let name: String = row.get(1)?;
        let amount: String = row.get(2)?;
        let commodity: String = row.get(3)?;
        let month: Option<String> = row.get(4)?;
        let category: Option<String> = row.get(5)?;
        let account: Option<String> = row.get(6)?;
        let provider: Option<String> = row.get(7)?;
        let auto_reserve_from: Option<String> = row.get(8)?;
        let auto_reserve_until_amount: Option<String> = row.get(9)?;
        let created_at: String = row.get(10)?;

        let id = Uuid::parse_str(&id).context("Invalid budget UUID")?;
        let amount = amount
            .parse::<Decimal>()
            .context("Invalid decimal amount in budgets table")?;
        let auto_reserve_until_amount = auto_reserve_until_amount
            .map(|s| s.parse::<Decimal>())
            .transpose()
            .context("Invalid decimal auto_reserve_until_amount in budgets table")?;
        let created_at = DateTime::parse_from_rfc3339(&created_at)
            .context("Invalid created_at in budgets table")?
            .with_timezone(&Utc);

        Ok(Some(StoredBudget {
            id,
            name,
            amount,
            commodity,
            month,
            category,
            account,
            provider,
            auto_reserve_from,
            auto_reserve_until_amount,
            created_at,
        }))
    }

    pub fn set_budget_auto_reserve(
        &self,
        name: &str,
        from_prefix: Option<&str>,
        until_amount: Option<Decimal>,
    ) -> Result<usize> {
        let changed = self.conn.execute(
            r#"
            UPDATE budgets
            SET auto_reserve_from = ?2,
                auto_reserve_until_amount = ?3
            WHERE name = ?1
            "#,
            params![name, from_prefix, until_amount.map(|d| d.to_string()),],
        )?;
        Ok(changed)
    }

    pub fn list_budgets(&self) -> Result<Vec<StoredBudget>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, amount, commodity, month, category, account, provider, auto_reserve_from, auto_reserve_until_amount, created_at
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
            let auto_reserve_from: Option<String> = row.get(8)?;
            let auto_reserve_until_amount: Option<String> = row.get(9)?;
            let created_at: String = row.get(10)?;
            Ok((
                id,
                name,
                amount,
                commodity,
                month,
                category,
                account,
                provider,
                auto_reserve_from,
                auto_reserve_until_amount,
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
                auto_reserve_from,
                auto_reserve_until_amount,
                created_at,
            ) = row?;
            let id = Uuid::parse_str(&id).context("Invalid budget UUID")?;
            let amount = amount
                .parse::<Decimal>()
                .context("Invalid decimal amount in budgets table")?;
            let auto_reserve_until_amount = auto_reserve_until_amount
                .map(|s| s.parse::<Decimal>())
                .transpose()
                .context("Invalid decimal auto_reserve_until_amount in budgets table")?;
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
                auto_reserve_from,
                auto_reserve_until_amount,
                created_at,
            });
        }

        Ok(out)
    }
}

fn add_column_if_missing(conn: &Connection, table: &str, column: &str, ty: &str) -> Result<()> {
    let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {ty}");
    match conn.execute(&sql, []) {
        Ok(_) => Ok(()),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("duplicate column name") {
                Ok(())
            } else {
                Err(e).with_context(|| format!("Failed to add column {table}.{column}"))
            }
        }
    }
}

pub fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create dir {}", parent.display()))?;
    }
    Ok(())
}
