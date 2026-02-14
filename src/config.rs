use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub target: String,
    #[serde(default, rename = "source")]
    pub sources: Vec<Source>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Source {
    pub path: String,
    pub name: String,
    pub prefix: Option<String>,
}

impl Source {
    pub fn prefix(&self) -> &str {
        self.prefix.as_deref().unwrap_or(&self.name)
    }
}

pub fn default_config_path() -> anyhow::Result<PathBuf> {
    if let Some(config_dir) = dirs::config_dir() {
        return Ok(config_dir.join("quiver/config.toml"));
    }
    if let Some(home_dir) = dirs::home_dir() {
        return Ok(home_dir.join(".config/quiver/config.toml"));
    }
    bail!("Could not determine config directory: neither XDG_CONFIG_HOME nor HOME is set");
}

pub fn load_config(config_path: &Path) -> anyhow::Result<Config> {
    if !config_path.exists() {
        bail!(
            "Config file not found at {}\nRun `quiver init` to create one.",
            config_path.display()
        );
    }
    let contents = std::fs::read_to_string(config_path).context("Failed to read config file")?;
    let config: Config = toml::from_str(&contents).context("Failed to parse config file")?;
    Ok(config)
}

pub fn save_config(config: &Config, config_path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create config directory")?;
    }
    let contents = toml::to_string_pretty(config).context("Failed to serialize config")?;
    std::fs::write(config_path, contents).context("Failed to write config file")?;
    Ok(())
}

pub fn default_config() -> Config {
    Config {
        target: "~/.claude/skills".into(),
        sources: vec![],
    }
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
        return PathBuf::from(path);
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

pub fn normalize_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    let trimmed = s.trim_end_matches('/');
    if trimmed.is_empty() {
        PathBuf::from("/")
    } else {
        PathBuf::from(trimmed)
    }
}
