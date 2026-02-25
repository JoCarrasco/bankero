use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Posting {
    pub account: String,
    pub commodity: String,
    /// Signed decimal amount encoded as a string in storage.
    pub amount: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateContext {
    pub provider: Option<String>,
    /// If present, the explicit override rate used.
    pub override_rate: Option<Decimal>,
    /// Rate base commodity (e.g., USD).
    pub base: Option<String>,
    /// Rate quote commodity (e.g., VES).
    pub quote: Option<String>,
    /// As-of timestamp used for resolution.
    pub as_of: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BasisContext {
    Fixed { amount: Decimal, commodity: String },
    Provider { provider: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPayload {
    pub schema_version: u32,
    pub device_id: Uuid,
    pub workspace: String,
    pub project: String,

    pub action: String,
    pub created_at: DateTime<Utc>,
    pub effective_at: DateTime<Utc>,

    pub postings: Vec<Posting>,

    #[serde(default)]
    pub tags: Vec<String>,
    pub category: Option<String>,
    pub note: Option<String>,

    pub rate_context: RateContext,
    pub basis: Option<BasisContext>,

    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct StoredEvent {
    pub event_id: Uuid,
    pub action: String,
    pub created_at: DateTime<Utc>,
    pub effective_at: DateTime<Utc>,
    pub payload: EventPayload,
}

pub fn is_provider_token(s: &str) -> bool {
    s.starts_with('@') && s.len() > 1
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderToken {
    pub provider: String,
    pub override_rate: Option<Decimal>,
}

pub fn parse_provider_token(s: &str) -> Option<ProviderToken> {
    if !is_provider_token(s) {
        return None;
    }

    let without_at = &s[1..];
    let (provider, rate_opt) = match without_at.split_once(':') {
        Some((p, r)) => (p, Some(r)),
        None => (without_at, None),
    };

    let provider = provider.trim();
    if provider.is_empty() {
        return None;
    }

    let override_rate = match rate_opt {
        None => None,
        Some(raw) => match raw.trim().parse::<Decimal>() {
            Ok(d) => Some(d),
            Err(_) => return None,
        },
    };

    Some(ProviderToken {
        provider: provider.to_string(),
        override_rate,
    })
}

pub fn parse_basis_arg(raw: &str) -> Option<BasisContext> {
    if let Some(p) = parse_provider_token(raw) {
        return Some(BasisContext::Provider {
            provider: format!("@{}", p.provider),
        });
    }
    None
}
