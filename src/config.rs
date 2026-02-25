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
    pub current_workspace: String,
    pub current_project: String,
    pub reference_commodity: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            device_id: Uuid::new_v4(),
            current_workspace: "personal".to_string(),
            current_project: "default".to_string(),
            reference_commodity: "USD".to_string(),
        }
    }
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
    let cfg: AppConfig = serde_json::from_str(&raw)
        .with_context(|| format!("Failed to parse {}", cfg_path.display()))?;

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
