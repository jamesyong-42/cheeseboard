use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Application configuration persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Unique device identifier (generated on first run, never changes).
    pub device_id: String,
    /// User-editable friendly device name.
    pub device_name: String,
}

impl AppConfig {
    /// Load config from the default path, or create a new one if it doesn't exist.
    pub fn load_or_create() -> Result<Self, ConfigError> {
        let path = Self::config_path()?;

        if path.exists() {
            let contents =
                std::fs::read_to_string(&path).map_err(|e| ConfigError::Io(e, path.clone()))?;
            let config: Self =
                serde_json::from_str(&contents).map_err(|e| ConfigError::Parse(e, path.clone()))?;
            tracing::info!("Loaded config from {}", path.display());
            Ok(config)
        } else {
            let config = Self::generate();
            config.save()?;
            tracing::info!("Created new config at {}", path.display());
            Ok(config)
        }
    }

    /// Save config to disk.
    pub fn save(&self) -> Result<(), ConfigError> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ConfigError::Io(e, parent.to_path_buf()))?;
        }
        let contents =
            serde_json::to_string_pretty(self).map_err(|e| ConfigError::Parse(e, path.clone()))?;
        std::fs::write(&path, contents).map_err(|e| ConfigError::Io(e, path.clone()))?;
        Ok(())
    }

    /// Generate a new config with a random device ID and hostname.
    fn generate() -> Self {
        let device_id = uuid::Uuid::new_v4().to_string();
        let device_name = hostname_or_default();

        Self {
            device_id,
            device_name,
        }
    }

    /// Path to the config file.
    pub fn config_path() -> Result<PathBuf, ConfigError> {
        let proj_dirs = directories::ProjectDirs::from("com", "cheeseboard", "Cheeseboard")
            .ok_or(ConfigError::NoHomeDir)?;
        Ok(proj_dirs.config_dir().join("config.json"))
    }

    /// Path to the state directory (for tsnet state).
    pub fn state_dir() -> Result<PathBuf, ConfigError> {
        let proj_dirs = directories::ProjectDirs::from("com", "cheeseboard", "Cheeseboard")
            .ok_or(ConfigError::NoHomeDir)?;
        Ok(proj_dirs.data_dir().join("tsnet-state"))
    }
}

/// Get the system hostname, or fall back to a generated name.
fn hostname_or_default() -> String {
    // Try common env vars first
    if let Ok(name) = std::env::var("HOSTNAME") {
        if !name.is_empty() {
            return name;
        }
    }
    if let Ok(name) = std::env::var("COMPUTERNAME") {
        if !name.is_empty() {
            return name;
        }
    }

    // Try reading /etc/hostname on Linux
    #[cfg(target_os = "linux")]
    if let Ok(name) = std::fs::read_to_string("/etc/hostname") {
        let name = name.trim().to_string();
        if !name.is_empty() {
            return name;
        }
    }

    // Try `scutil --get ComputerName` on macOS via file check
    #[cfg(target_os = "macos")]
    if let Ok(output) = std::process::Command::new("scutil")
        .args(["--get", "ComputerName"])
        .output()
    {
        if output.status.success() {
            let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !name.is_empty() {
                return name;
            }
        }
    }

    format!("cheeseboard-{}", &uuid::Uuid::new_v4().to_string()[..8])
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error at {1}: {0}")]
    Io(std::io::Error, PathBuf),

    #[error("Parse error at {1}: {0}")]
    Parse(serde_json::Error, PathBuf),

    #[error("Could not determine home directory")]
    NoHomeDir,
}
