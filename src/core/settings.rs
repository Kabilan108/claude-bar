use anyhow::{Context, Result};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub providers: ProviderSettings,
    pub display: DisplaySettings,
    pub browser: BrowserSettings,
    pub notifications: NotificationSettings,
    pub debug: bool,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplaySettings {
    pub show_as_remaining: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserSettings {
    pub preferred: Option<String>,
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

pub struct SettingsWatcher {
    settings: Arc<RwLock<Settings>>,
    #[allow(dead_code)]
    update_tx: broadcast::Sender<Settings>,
    _watcher: Option<RecommendedWatcher>,
}

impl SettingsWatcher {
    pub fn new() -> Result<Self> {
        let settings = Settings::load()?;
        settings.validate()?;

        let (update_tx, _) = broadcast::channel(16);
        let settings = Arc::new(RwLock::new(settings));

        Ok(Self {
            settings,
            update_tx,
            _watcher: None,
        })
    }

    #[allow(dead_code)]
    pub fn start_watching(&mut self) -> Result<()> {
        let Some(config_path) = Settings::config_path() else {
            tracing::warn!("Could not determine config path, hot-reload disabled");
            return Ok(());
        };

        if !config_path.exists() {
            tracing::info!(?config_path, "Config file does not exist, hot-reload waiting");
            if let Some(parent) = config_path.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent)?;
                }
            }
        }

        let settings_clone = Arc::clone(&self.settings);
        let update_tx_clone = self.update_tx.clone();
        let config_path_clone = config_path.clone();

        let (tx, rx) = std::sync::mpsc::channel();

        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    if event.kind.is_modify() || event.kind.is_create() {
                        let _ = tx.send(());
                    }
                }
            },
            Config::default(),
        )?;

        let watch_path = config_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        watcher.watch(watch_path, RecursiveMode::NonRecursive)?;

        tracing::info!(?watch_path, "Started watching config directory");

        tokio::spawn(async move {
            while rx.recv().is_ok() {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                match Settings::load() {
                    Ok(new_settings) => {
                        if let Err(e) = new_settings.validate() {
                            tracing::error!(?e, "Config validation failed, keeping old settings");
                            continue;
                        }

                        tracing::info!(?config_path_clone, "Config reloaded");

                        {
                            let mut current = settings_clone.write().await;
                            *current = new_settings.clone();
                        }

                        let _ = update_tx_clone.send(new_settings);
                    }
                    Err(e) => {
                        tracing::error!(?e, "Failed to reload config");
                    }
                }
            }
        });

        self._watcher = Some(watcher);
        Ok(())
    }

    #[allow(dead_code)]
    pub fn subscribe(&self) -> broadcast::Receiver<Settings> {
        self.update_tx.subscribe()
    }

    pub async fn get(&self) -> Settings {
        self.settings.read().await.clone()
    }

    #[allow(dead_code)]
    pub fn get_blocking(&self) -> Settings {
        self.settings.blocking_read().clone()
    }
}

impl Default for SettingsWatcher {
    fn default() -> Self {
        Self::new().expect("Failed to create default SettingsWatcher")
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
