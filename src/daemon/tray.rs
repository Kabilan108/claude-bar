use crate::core::models::Provider;
use crate::core::settings::ThemeMode;
use crate::core::settings::Settings;
use crate::icons::{IconRenderer, IconState};
use ksni::{self, menu::StandardItem, Handle, MenuItem, Tray, TrayMethods};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};

const ICON_SIZE: i32 = 22;
const ANIMATION_FPS: u64 = 15;
const ANIMATION_INTERVAL: Duration = Duration::from_millis(1000 / ANIMATION_FPS);
const REFRESH_COOLDOWN: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrayEvent {
    LeftClick(Provider),
    RefreshRequested,
    OpenDashboard(Provider),
    Quit,
}

struct ClaudeBarTray {
    provider: Provider,
    primary_percent: f64,
    secondary_percent: f64,
    state: IconState,
    animation_phase: f64,
    has_credentials: bool,
    theme_mode: ThemeMode,
    system_is_dark: bool,
    merged_mode: bool,
    providers: Vec<Provider>,
    event_tx: mpsc::UnboundedSender<TrayEvent>,
}

impl Tray for ClaudeBarTray {
    fn id(&self) -> String {
        match self.provider {
            Provider::Claude => "claude-bar-claude".to_string(),
            Provider::Codex => "claude-bar-codex".to_string(),
        }
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::ApplicationStatus
    }

    fn title(&self) -> String {
        self.provider.name().to_string()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        let renderer = IconRenderer::new();

        let (primary, secondary) = if self.state == IconState::Loading {
            IconRenderer::knight_rider_frame(self.animation_phase)
        } else {
            (self.primary_percent, self.secondary_percent)
        };

        let pixels = renderer.render(
            self.provider,
            primary,
            secondary,
            self.state,
            self.is_dark(),
        );

        vec![ksni::Icon {
            width: ICON_SIZE,
            height: ICON_SIZE,
            data: argb_to_network_order(&pixels, ICON_SIZE as usize),
        }]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let title = self.provider.name().to_string();
        let description = match self.state {
            IconState::Loading => "Loading...".to_string(),
            IconState::Error => "Authentication required".to_string(),
            IconState::Stale => format!(
                "Session: {:.0}% used | Weekly: {:.0}% used (stale data)",
                self.primary_percent * 100.0,
                self.secondary_percent * 100.0
            ),
            IconState::Normal => format!(
                "Session: {:.0}% used | Weekly: {:.0}% used",
                self.primary_percent * 100.0,
                self.secondary_percent * 100.0
            ),
        };

        ksni::ToolTip {
            title,
            description,
            icon_name: String::new(),
            icon_pixmap: Vec::new(),
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items = vec![MenuItem::Standard(StandardItem {
            label: "Refresh Now".to_string(),
            activate: Box::new(|tray: &mut Self| {
                let _ = tray.event_tx.send(TrayEvent::RefreshRequested);
            }),
            ..Default::default()
        })];

        if self.merged_mode {
            for provider in &self.providers {
                let provider = *provider;
                items.push(MenuItem::Standard(StandardItem {
                    label: format!("Open {} Dashboard", provider.name()),
                    activate: Box::new(move |tray: &mut Self| {
                        let _ = tray.event_tx.send(TrayEvent::OpenDashboard(provider));
                    }),
                    ..Default::default()
                }));
            }
        } else if self.has_credentials {
            items.push(MenuItem::Standard(StandardItem {
                label: format!("Open {} Dashboard", self.provider.name()),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.event_tx.send(TrayEvent::OpenDashboard(tray.provider));
                }),
                ..Default::default()
            }));
        }

        items.push(MenuItem::Separator);

        items.push(MenuItem::Standard(StandardItem {
            label: "Quit".to_string(),
            activate: Box::new(|tray: &mut Self| {
                let _ = tray.event_tx.send(TrayEvent::Quit);
            }),
            ..Default::default()
        }));

        items
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.event_tx.send(TrayEvent::LeftClick(self.provider));
    }
}

impl ClaudeBarTray {
    fn is_dark(&self) -> bool {
        match self.theme_mode {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => self.system_is_dark,
        }
    }
}

fn argb_to_network_order(rgba: &[u8], size: usize) -> Vec<u8> {
    let mut argb = Vec::with_capacity(size * size * 4);
    for chunk in rgba.chunks_exact(4) {
        let r = chunk[0];
        let g = chunk[1];
        let b = chunk[2];
        let a = chunk[3];
        argb.push(a);
        argb.push(r);
        argb.push(g);
        argb.push(b);
    }
    argb
}

struct TrayState {
    primary_percent: f64,
    secondary_percent: f64,
    state: IconState,
    animation_phase: f64,
    has_credentials: bool,
    last_refresh: Instant,
    handle: Option<Handle<ClaudeBarTray>>,
}

impl TrayState {
    fn sync_to_tray<F>(&self, updater: F)
    where
        F: FnOnce(&mut ClaudeBarTray) + Send + 'static,
    {
        if let Some(handle) = &self.handle {
            let handle = handle.clone();
            tokio::spawn(async move {
                let _ = handle.update(updater).await;
            });
        }
    }
}

impl Default for TrayState {
    fn default() -> Self {
        Self {
            primary_percent: 0.0,
            secondary_percent: 0.0,
            state: IconState::Loading,
            animation_phase: 0.0,
            has_credentials: false,
            last_refresh: Instant::now() - REFRESH_COOLDOWN,
            handle: None,
        }
    }
}

struct TrayManagerInner {
    states: HashMap<Provider, TrayState>,
    merged_mode: bool,
    theme_mode: ThemeMode,
    system_is_dark: bool,
}

impl Default for TrayManagerInner {
    fn default() -> Self {
        Self {
            states: HashMap::new(),
            merged_mode: false,
            theme_mode: ThemeMode::System,
            system_is_dark: false,
        }
    }
}

pub struct TrayManager {
    inner: Arc<RwLock<TrayManagerInner>>,
    event_tx: mpsc::UnboundedSender<TrayEvent>,
    event_rx: Arc<RwLock<Option<mpsc::UnboundedReceiver<TrayEvent>>>>,
}

impl TrayManager {
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Self {
            inner: Arc::new(RwLock::new(TrayManagerInner::default())),
            event_tx,
            event_rx: Arc::new(RwLock::new(Some(event_rx))),
        }
    }

    pub async fn take_event_receiver(&self) -> Option<mpsc::UnboundedReceiver<TrayEvent>> {
        self.event_rx.write().await.take()
    }

    pub async fn start(&self, settings: &Settings) -> anyhow::Result<()> {
        let mut inner = self.inner.write().await;
        inner.merged_mode = settings.providers.merge_icons;
        inner.theme_mode = settings.theme.mode.clone();
        inner.system_is_dark = matches!(settings.theme.mode, ThemeMode::Dark);

        let mut enabled_providers = Vec::new();
        if settings.providers.claude.enabled {
            enabled_providers.push(Provider::Claude);
        }
        if settings.providers.codex.enabled {
            enabled_providers.push(Provider::Codex);
        }
        if enabled_providers.is_empty() {
            enabled_providers.push(Provider::Claude);
        }

        let providers_to_show = if inner.merged_mode {
            vec![*enabled_providers.first().unwrap_or(&Provider::Claude)]
        } else {
            enabled_providers.clone()
        };

        for provider in providers_to_show {
            let tray = ClaudeBarTray {
                provider,
                primary_percent: 0.0,
                secondary_percent: 0.0,
                state: IconState::Loading,
                animation_phase: 0.0,
                has_credentials: false,
                theme_mode: inner.theme_mode.clone(),
                system_is_dark: inner.system_is_dark,
                merged_mode: inner.merged_mode,
                providers: if inner.merged_mode {
                    enabled_providers.clone()
                } else {
                    vec![provider]
                },
                event_tx: self.event_tx.clone(),
            };

            let handle = tray.spawn().await?;

            inner.states.insert(
                provider,
                TrayState {
                    handle: Some(handle),
                    ..Default::default()
                },
            );

            tracing::info!(provider = ?provider, "Tray icon registered");
        }

        Ok(())
    }

    pub async fn update_icon(&self, provider: Provider, primary: f64, secondary: f64) {
        let mut inner = self.inner.write().await;
        if let Some(state) = inner.states.get_mut(&provider) {
            state.primary_percent = primary;
            state.secondary_percent = secondary;
            state.state = IconState::Normal;
            state.sync_to_tray(move |tray| {
                tray.primary_percent = primary;
                tray.secondary_percent = secondary;
                tray.state = IconState::Normal;
            });
        }
    }

    pub async fn set_loading(&self, provider: Provider) {
        let mut inner = self.inner.write().await;
        if let Some(state) = inner.states.get_mut(&provider) {
            state.state = IconState::Loading;
            state.animation_phase = 0.0;
            state.sync_to_tray(|tray| {
                tray.state = IconState::Loading;
                tray.animation_phase = 0.0;
            });
        }
    }

    pub async fn set_error(&self, provider: Provider) {
        let mut inner = self.inner.write().await;
        if let Some(state) = inner.states.get_mut(&provider) {
            state.state = IconState::Error;
            state.has_credentials = false;
            state.sync_to_tray(|tray| {
                tray.state = IconState::Error;
                tray.has_credentials = false;
            });
        }

        if inner
            .states
            .values()
            .all(|state| state.state == IconState::Error)
        {
            for state in inner.states.values_mut() {
                state.state = IconState::Normal;
                state.sync_to_tray(|tray| {
                    tray.state = IconState::Normal;
                });
            }
        }
    }

    #[allow(dead_code)]
    pub async fn set_stale(&self, provider: Provider) {
        let mut inner = self.inner.write().await;
        if let Some(state) = inner.states.get_mut(&provider) {
            state.state = IconState::Stale;
            state.sync_to_tray(|tray| {
                tray.state = IconState::Stale;
            });
        }
    }

    pub async fn set_credentials_valid(&self, provider: Provider, valid: bool) {
        let mut inner = self.inner.write().await;
        if let Some(state) = inner.states.get_mut(&provider) {
            state.has_credentials = valid;
            state.sync_to_tray(move |tray| {
                tray.has_credentials = valid;
            });
        }
    }

    pub async fn set_system_is_dark(&self, is_dark: bool) {
        let mut inner = self.inner.write().await;
        inner.system_is_dark = is_dark;
        for state in inner.states.values() {
            state.sync_to_tray(move |tray| {
                tray.system_is_dark = is_dark;
            });
        }
    }

    pub async fn tick_animation(&self) {
        let mut inner = self.inner.write().await;
        for state in inner.states.values_mut() {
            if state.state == IconState::Loading {
                state.animation_phase += std::f64::consts::PI / 30.0;
                let phase = state.animation_phase;
                state.sync_to_tray(move |tray| {
                    tray.animation_phase = phase;
                });
            }
        }
    }

    pub async fn should_refresh(&self, provider: Provider) -> bool {
        let inner = self.inner.read().await;
        inner
            .states
            .get(&provider)
            .map(|s| s.last_refresh.elapsed() >= REFRESH_COOLDOWN)
            .unwrap_or(true)
    }

    pub async fn mark_refreshed(&self, provider: Provider) {
        let mut inner = self.inner.write().await;
        if let Some(state) = inner.states.get_mut(&provider) {
            state.last_refresh = Instant::now();
        }
    }

    #[allow(dead_code)]
    pub async fn is_merged_mode(&self) -> bool {
        self.inner.read().await.merged_mode
    }

    pub async fn shutdown(&self) {
        let mut inner = self.inner.write().await;
        inner.states.clear();
        tracing::info!("Tray icons shut down");
    }
}

impl Default for TrayManager {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn run_animation_loop(tray_manager: Arc<TrayManager>) {
    let mut interval = tokio::time::interval(ANIMATION_INTERVAL);

    loop {
        interval.tick().await;
        tray_manager.tick_animation().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_argb_conversion() {
        let rgba = vec![255, 128, 64, 200];
        let argb = argb_to_network_order(&rgba, 1);
        assert_eq!(argb, vec![200, 255, 128, 64]);
    }

    #[tokio::test]
    async fn test_tray_manager_creation() {
        let manager = TrayManager::new();
        assert!(!manager.is_merged_mode().await);
    }
}
