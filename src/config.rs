use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub device_id: Uuid,

    /// Friendly device name used for human-facing identification (e.g. in sync discovery).
    ///
    /// If missing (older configs), it is auto-filled from `device_id`.
    #[serde(default)]
    pub device_name: Option<String>,
    pub current_workspace: String,
    pub current_project: String,
    pub reference_commodity: String,

    /// Shared folder path used for file-based multi-device sync (MVP).
    #[serde(default)]
    pub sync_dir: Option<String>,

    /// Timestamp of the last successful sync.
    #[serde(default)]
    pub last_sync_at: Option<DateTime<Utc>>,
}

impl Default for AppConfig {
    fn default() -> Self {
        let device_id = Uuid::new_v4();
        Self {
            device_id,
            device_name: Some(funny_name_from_uuid(device_id)),
            current_workspace: "personal".to_string(),
            current_project: "default".to_string(),
            reference_commodity: "USD".to_string(),
            sync_dir: None,
            last_sync_at: None,
        }
    }
}

pub fn funny_name_from_uuid(id: Uuid) -> String {
    // Deterministic, dependency-free name generation.
    // Keep the list small and inoffensive; output is stable per device_id.
    const ADJ: &[&str] = &[
        "juicy", "zesty", "bouncy", "cosmic", "witty", "sparkly", "sleepy", "brave", "sneaky",
        "happy", "mellow", "curious", "tiny", "giant", "swift", "cuddly", "crispy", "gentle",
        "spicy", "funky",
    ];
    const NOUN: &[&str] = &[
        "strawberry",
        "pineapple",
        "mango",
        "blueberry",
        "kiwi",
        "peach",
        "avocado",
        "lemon",
        "tangerine",
        "panda",
        "otter",
        "penguin",
        "alpaca",
        "badger",
        "fox",
        "koala",
        "gecko",
        "hamster",
        "turtle",
        "narwhal",
    ];

    let b = id.as_bytes();
    let a = u16::from_le_bytes([b[0], b[1]]) as usize;
    let n = u16::from_le_bytes([b[2], b[3]]) as usize;

    format!("{}_{}", ADJ[a % ADJ.len()], NOUN[n % NOUN.len()])
}

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

pub fn app_paths(override_home: Option<PathBuf>) -> Result<AppPaths> {
    if let Some(home) = override_home {
        return Ok(AppPaths {
            config_dir: home.join("config"),
            data_dir: home.join("data"),
        });
    }

    let proj = ProjectDirs::from("com", "bankero", "bankero")
        .context("Failed to resolve platform directories")?;

    Ok(AppPaths {
        config_dir: proj.config_dir().to_path_buf(),
        data_dir: proj.data_dir().to_path_buf(),
    })
}

pub fn load_or_init_config(paths: &AppPaths) -> Result<(AppConfig, PathBuf)> {
    fs::create_dir_all(&paths.config_dir)
        .with_context(|| format!("Failed to create config dir {}", paths.config_dir.display()))?;

    let cfg_path = paths.config_dir.join("config.json");
    if !cfg_path.exists() {
        let cfg = AppConfig::default();
        write_config(&cfg_path, &cfg)?;
        return Ok((cfg, cfg_path));
    }

    let raw = fs::read_to_string(&cfg_path)
        .with_context(|| format!("Failed to read {}", cfg_path.display()))?;
    let mut cfg: AppConfig = serde_json::from_str(&raw)
        .with_context(|| format!("Failed to parse {}", cfg_path.display()))?;

    // Auto-migrate older config versions.
    let mut changed = false;
    if cfg.device_name.is_none() {
        cfg.device_name = Some(funny_name_from_uuid(cfg.device_id));
        changed = true;
    }
    if changed {
        write_config(&cfg_path, &cfg)?;
    }

    Ok((cfg, cfg_path))
}

pub fn write_config(path: &Path, cfg: &AppConfig) -> Result<()> {
    let json = serde_json::to_string_pretty(cfg)?;
    fs::write(path, json).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

pub fn workspace_slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        let mapped = match ch {
            'a'..='z' | '0'..='9' | '-' | '_' => Some(ch),
            'A'..='Z' => Some(ch.to_ascii_lowercase()),
            ' ' | ':' | '/' | '\\' => Some('-'),
            _ => None,
        };
        if let Some(c) = mapped {
            if !(c == '-' && out.ends_with('-')) {
                out.push(c);
            }
        }
    }

    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "workspace".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn now_utc() -> DateTime<Utc> {
    Utc::now()
}
