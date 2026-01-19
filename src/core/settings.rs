use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub providers: ProviderSettings,
    pub display: DisplaySettings,
    pub browser: BrowserSettings,
    pub notifications: NotificationSettings,
    pub debug: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            providers: ProviderSettings::default(),
            display: DisplaySettings::default(),
            browser: BrowserSettings::default(),
            notifications: NotificationSettings::default(),
            debug: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderSettings {
    pub claude: ProviderConfig,
    pub codex: ProviderConfig,
    pub merge_icons: bool,
}

impl Default for ProviderSettings {
    fn default() -> Self {
        Self {
            claude: ProviderConfig { enabled: true },
            codex: ProviderConfig { enabled: true },
            merge_icons: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    pub enabled: bool,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplaySettings {
    pub show_as_remaining: bool,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            show_as_remaining: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserSettings {
    pub preferred: Option<String>,
}

impl Default for BrowserSettings {
    fn default() -> Self {
        Self { preferred: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationSettings {
    pub enabled: bool,
    pub threshold: f64,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 0.9,
        }
    }
}

impl Settings {
    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("claude-bar").join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path().context("Could not determine config directory")?;

        if !path.exists() {
            tracing::info!(?path, "Config file not found, using defaults");
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let settings: Settings = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        tracing::info!(?path, "Loaded config");
        Ok(settings)
    }

    pub fn validate(&self) -> Result<()> {
        if self.notifications.threshold < 0.0 || self.notifications.threshold > 1.0 {
            anyhow::bail!(
                "notifications.threshold must be between 0.0 and 1.0, got {}",
                self.notifications.threshold
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = Settings::default();
        assert!(settings.providers.claude.enabled);
        assert!(settings.providers.codex.enabled);
        assert!(settings.providers.merge_icons);
        assert!(!settings.display.show_as_remaining);
        assert!(settings.notifications.enabled);
        assert!((settings.notifications.threshold - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn test_settings_validation() {
        let mut settings = Settings::default();
        assert!(settings.validate().is_ok());

        settings.notifications.threshold = 1.5;
        assert!(settings.validate().is_err());

        settings.notifications.threshold = -0.1;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn test_parse_toml() {
        let toml = r#"
            debug = true

            [providers]
            merge_icons = false

            [providers.claude]
            enabled = true

            [providers.codex]
            enabled = false

            [display]
            show_as_remaining = true

            [notifications]
            enabled = false
            threshold = 0.85
        "#;

        let settings: Settings = toml::from_str(toml).unwrap();
        assert!(settings.debug);
        assert!(!settings.providers.merge_icons);
        assert!(settings.providers.claude.enabled);
        assert!(!settings.providers.codex.enabled);
        assert!(settings.display.show_as_remaining);
        assert!(!settings.notifications.enabled);
        assert!((settings.notifications.threshold - 0.85).abs() < f64::EPSILON);
    }
}
