use crate::core::credentials::CredentialsWatcher;
use crate::core::models::{CostSnapshot, CostUsageTokenSnapshot, Provider, UsageSnapshot};
use crate::core::retry::RetryState;
use crate::core::settings::{Settings, SettingsWatcher};
use crate::core::store::UsageStore;
use crate::cost::{CostStore, PricingRefreshResult};
use crate::daemon::dbus::{start_dbus_server, DbusCommand};
use crate::daemon::tray::{run_animation_loop, TrayEvent, TrayManager};
use crate::providers::ProviderRegistry;
use crate::ui::PopupWindow;
use anyhow::Result;
use gtk4::glib;
use gtk4::prelude::*;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager};
use libadwaita as adw;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};

const APP_ID: &str = "com.github.kabilan.claude-bar";

pub async fn run() -> Result<()> {
    tracing::info!(app_id = APP_ID, "Initializing GTK application");

    let mut settings_watcher = SettingsWatcher::new()?;
    let settings = settings_watcher.get().await;
    settings_watcher.start_watching()?;

    let store = Arc::new(UsageStore::new());
    let cost_store = Arc::new(RwLock::new(CostStore::new()));
    let tray_manager = Arc::new(TrayManager::new());
    let retry_states = Arc::new(RwLock::new(HashMap::<Provider, RetryState>::new()));

    let registry = Arc::new(ProviderRegistry::new(&settings));

    let cred_paths = registry.credentials_paths();
    let (_cred_watcher, cred_change_rx) = CredentialsWatcher::start(cred_paths)?;

    tray_manager.start(&settings).await?;
    tokio::spawn(run_animation_loop(Arc::clone(&tray_manager)));

    let (ui_tx, ui_rx) = mpsc::unbounded_channel::<UiCommand>();

    start_global_shortcut(
        &settings,
        Arc::clone(&store),
        ui_tx.clone(),
        Arc::clone(&registry),
    );

    let (dbus_cmd_tx, dbus_cmd_rx) = mpsc::unbounded_channel::<DbusCommand>();
    let _dbus_connection = start_dbus_server(dbus_cmd_tx).await?;

    tokio::spawn(handle_dbus_commands(
        dbus_cmd_rx,
        Arc::clone(&registry),
        Arc::clone(&store),
        Arc::clone(&cost_store),
        Arc::clone(&tray_manager),
        ui_tx.clone(),
    ));

    tokio::spawn(run_polling_loop(
        Arc::clone(&registry),
        Arc::clone(&store),
        Arc::clone(&tray_manager),
        Arc::clone(&retry_states),
        ui_tx.clone(),
        cred_change_rx,
    ));

    tokio::spawn(run_pricing_refresh_loop(Arc::clone(&cost_store)));
    tokio::spawn(run_cost_scan_loop(
        Arc::clone(&cost_store),
        Arc::clone(&store),
        ui_tx.clone(),
    ));

    let mut settings_rx = settings_watcher.subscribe();
    let tray_for_settings = Arc::clone(&tray_manager);
    let ui_tx_settings = ui_tx.clone();
    tokio::spawn(async move {
        while let Ok(new_settings) = settings_rx.recv().await {
            if let Err(e) = tray_for_settings.apply_settings(&new_settings).await {
                tracing::warn!(error = %e, "Failed to apply tray settings");
            }
            let _ = ui_tx_settings.send(UiCommand::ApplySettings {
                show_as_remaining: new_settings.display.show_as_remaining,
                theme_mode: new_settings.theme.mode.clone(),
                popup: new_settings.popup.clone(),
            });
        }
    });

    if let Some(mut event_rx) = tray_manager.take_event_receiver().await {
        let store_clone = Arc::clone(&store);
        let registry_clone = Arc::clone(&registry);
        let tray_clone = Arc::clone(&tray_manager);
        let ui_tx_clone = ui_tx.clone();

        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                handle_tray_event(
                    event,
                    &store_clone,
                    &registry_clone,
                    &tray_clone,
                    &ui_tx_clone,
                )
                .await;
            }
        });
    }

    run_gtk_main_loop(
        ui_rx,
        settings.theme.mode,
        settings.display.show_as_remaining,
        settings.popup.clone(),
        Arc::clone(&tray_manager),
    )
    .await
}

async fn handle_dbus_commands(
    mut cmd_rx: mpsc::UnboundedReceiver<DbusCommand>,
    registry: Arc<ProviderRegistry>,
    store: Arc<UsageStore>,
    cost_store: Arc<RwLock<CostStore>>,
    tray: Arc<TrayManager>,
    ui_tx: mpsc::UnboundedSender<UiCommand>,
) {
    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            DbusCommand::Refresh => {
                tracing::info!("D-Bus refresh command received");
                for provider in [Provider::Claude, Provider::Codex] {
                    tray.set_loading(provider).await;
                    refresh_provider(&registry, &store, &tray, &ui_tx, provider).await;
                }
            }
            DbusCommand::RefreshPricing => {
                tracing::info!("D-Bus refresh pricing command received");
                let refresh_result = {
                    let mut cost_store = cost_store.write().await;
                    cost_store.refresh_pricing(true).await
                };

                match refresh_result {
                    Ok(PricingRefreshResult::Refreshed) => {
                        scan_and_update_costs(&cost_store, &store, &ui_tx).await;
                    }
                    Ok(PricingRefreshResult::Skipped) => {}
                    Ok(PricingRefreshResult::Failed) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "Pricing refresh failed");
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
enum UiCommand {
    ShowPopup {
        provider: Provider,
        snapshot: Option<Box<UsageSnapshot>>,
        cost: Option<Box<CostSnapshot>>,
        tokens: Option<Box<CostUsageTokenSnapshot>>,
        error: Option<(String, String)>,
    },
    ShowProviderMenu {
        providers: Vec<Provider>,
    },
    UpdateUsage {
        provider: Provider,
        snapshot: Box<UsageSnapshot>,
    },
    UpdateCost {
        provider: Provider,
        cost: Box<CostSnapshot>,
    },
    UpdateTokens {
        provider: Provider,
        tokens: Box<CostUsageTokenSnapshot>,
    },
    ApplySettings {
        show_as_remaining: bool,
        theme_mode: crate::core::settings::ThemeMode,
        popup: crate::core::settings::PopupSettings,
    },
}

async fn run_gtk_main_loop(
    mut ui_rx: mpsc::UnboundedReceiver<UiCommand>,
    theme_mode: crate::core::settings::ThemeMode,
    show_as_remaining: bool,
    popup_settings: crate::core::settings::PopupSettings,
    tray_manager: Arc<TrayManager>,
) -> Result<()> {
    // libadwaita manages its own Adwaita-based theming; custom GTK themes
    // (via GTK_THEME or ~/.config/gtk-4.0/gtk.css) are unsupported and cause
    // warnings about missing GResource bundles.
    std::env::remove_var("GTK_THEME");

    gtk4::init().expect("Failed to initialize GTK4");
    adw::init().expect("Failed to initialize libadwaita");

    // The system gsettings color-scheme sets the deprecated GTK3-era
    // gtk-application-prefer-dark-theme property. Reset it so libadwaita's
    // AdwStyleManager is the sole dark-mode handler.
    if let Some(settings) = gtk4::Settings::default() {
        settings.set_gtk_application_prefer_dark_theme(false);
    }

    let app = adw::Application::builder().application_id(APP_ID).build();
    let popup_holder: Rc<RefCell<Option<PopupWindow>>> = Rc::new(RefCell::new(None));

    let popup_holder_activate = popup_holder.clone();
    let theme_mode = theme_mode.clone();
    let tray_manager_theme = Arc::clone(&tray_manager);
    app.connect_activate(move |app| {
        tracing::info!("GTK application activated");
        let popup = PopupWindow::new(app, theme_mode.clone(), &popup_settings);
        popup.set_show_as_remaining(show_as_remaining);
        *popup_holder_activate.borrow_mut() = Some(popup);
        if matches!(theme_mode, crate::core::settings::ThemeMode::System) {
            let is_dark = adw::StyleManager::default().is_dark();
            let tray_manager = Arc::clone(&tray_manager_theme);
            tokio::spawn(async move {
                tray_manager.set_system_is_dark(is_dark).await;
            });
        }
    });

    let _hold_guard = app.hold();
    app.register(None::<&gtk4::gio::Cancellable>)?;
    app.activate();

    let popup_holder_idle = popup_holder.clone();
    glib::idle_add_local(move || {
        if let Ok(cmd) = ui_rx.try_recv() {
            if let Some(popup) = popup_holder_idle.borrow().as_ref() {
                handle_ui_command(popup, cmd);
            }
        }
        glib::ControlFlow::Continue
    });

    let main_context = glib::MainContext::default();
    loop {
        main_context.iteration(true);
    }
}

fn handle_ui_command(popup: &PopupWindow, cmd: UiCommand) {
    match cmd {
        UiCommand::ShowPopup {
            provider,
            snapshot,
            cost,
            tokens,
            error,
        } => {
            if let Some((error_msg, hint)) = error {
                popup.show_error(provider, &error_msg, &hint);
            } else {
                if let Some(snap) = snapshot {
                    popup.update_usage(provider, &snap);
                }
                if let Some(c) = cost {
                    popup.update_cost(provider, &c);
                }
                if let Some(t) = tokens {
                    popup.update_tokens(provider, &t);
                }
            }
            popup.show(provider);
        }
        UiCommand::ShowProviderMenu { providers } => {
            popup.show_provider_menu(&providers);
        }
        UiCommand::UpdateUsage { provider, snapshot } => {
            popup.update_usage(provider, &snapshot);
        }
        UiCommand::UpdateCost { provider, cost } => {
            popup.update_cost(provider, &cost);
        }
        UiCommand::UpdateTokens { provider, tokens } => {
            popup.update_tokens(provider, &tokens);
        }
        UiCommand::ApplySettings {
            show_as_remaining,
            theme_mode,
            popup: popup_settings,
        } => {
            popup.set_show_as_remaining(show_as_remaining);
            popup.set_theme_mode(theme_mode);
            popup.apply_popup_settings(&popup_settings);
        }
    }
}

async fn handle_tray_event(
    event: TrayEvent,
    store: &Arc<UsageStore>,
    registry: &Arc<ProviderRegistry>,
    tray: &Arc<TrayManager>,
    ui_tx: &mpsc::UnboundedSender<UiCommand>,
) {
    match event {
        TrayEvent::LeftClick(provider) => {
            tracing::debug!(?provider, "Tray icon clicked");

            if tray.is_merged_mode().await {
                let mut providers = registry.enabled_provider_ids();
                if providers.is_empty() {
                    providers.push(Provider::Claude);
                }
                let _ = ui_tx.send(UiCommand::ShowProviderMenu { providers });
                return;
            }

            if tray.should_refresh(provider).await {
                tray.mark_refreshed(provider).await;
                tray.set_loading(provider).await;

                let registry_clone = Arc::clone(registry);
                let store_clone = Arc::clone(store);
                let tray_clone = Arc::clone(tray);
                let ui_tx_clone = ui_tx.clone();
                let p = provider;

                tokio::spawn(async move {
                    refresh_provider(&registry_clone, &store_clone, &tray_clone, &ui_tx_clone, p)
                        .await;
                });
            }

            let snapshot = store.get_snapshot(provider).await.map(Box::new);
            let cost = store.get_cost(provider).await.map(Box::new);
            let error = store
                .get_error(provider)
                .await
                .map(|e| (e, provider_error_hint(provider).to_string()));
            let tokens = store.get_token_snapshot(provider).await.map(Box::new);

            let _ = ui_tx.send(UiCommand::ShowPopup {
                provider,
                snapshot,
                cost,
                tokens,
                error,
            });
        }
        TrayEvent::RefreshRequested => {
            tracing::info!("Manual refresh requested");
            for provider in [Provider::Claude, Provider::Codex] {
                tray.set_loading(provider).await;
            }

            let results = registry.fetch_all().await;
            for (provider, result) in results {
                match result {
                    Ok(snapshot) => {
                        apply_successful_fetch(provider, snapshot, store, tray, ui_tx).await;
                    }
                    Err(e) => {
                        apply_failed_fetch(provider, &e, store, tray).await;
                    }
                }
            }
        }
        TrayEvent::OpenDashboard(provider) => {
            let url = provider.dashboard_url();
            tracing::info!(?provider, url, "Opening dashboard");
            if let Err(e) = open::that(url) {
                tracing::error!(error = %e, "Failed to open browser");
            }
        }
        TrayEvent::Quit => {
            tracing::info!("Quit requested");
            tray.shutdown().await;
            std::process::exit(0);
        }
    }
}

async fn run_polling_loop(
    registry: Arc<ProviderRegistry>,
    store: Arc<UsageStore>,
    tray: Arc<TrayManager>,
    retry_states: Arc<RwLock<HashMap<Provider, RetryState>>>,
    ui_tx: mpsc::UnboundedSender<UiCommand>,
    mut cred_change_rx: mpsc::UnboundedReceiver<Provider>,
) {
    for provider in [Provider::Claude, Provider::Codex] {
        retry_states.write().await.insert(provider, RetryState::new());
    }

    for provider in [Provider::Claude, Provider::Codex] {
        refresh_provider_with_retry(
            &registry,
            &store,
            &tray,
            &retry_states,
            &ui_tx,
            provider,
        )
        .await;
    }

    let mut check_interval = tokio::time::interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            _ = check_interval.tick() => {
                for provider in [Provider::Claude, Provider::Codex] {
                    let should_poll = {
                        let states = retry_states.read().await;
                        let state = states.get(&provider).cloned().unwrap_or_default();
                        let delay = state.current_delay();
                        store.should_refresh(provider, delay).await
                    };

                    if should_poll {
                        refresh_provider_with_retry(
                            &registry,
                            &store,
                            &tray,
                            &retry_states,
                            &ui_tx,
                            provider,
                        )
                        .await;
                    }
                }
            }
            Some(provider) = cred_change_rx.recv() => {
                tracing::info!(
                    ?provider,
                    "Credentials changed on disk, resetting retry state"
                );
                {
                    let mut states = retry_states.write().await;
                    if let Some(state) = states.get_mut(&provider) {
                        state.record_success();
                    }
                }
                store.clear_last_fetch(provider).await;
                refresh_provider_with_retry(
                    &registry,
                    &store,
                    &tray,
                    &retry_states,
                    &ui_tx,
                    provider,
                )
                .await;
            }
        }
    }
}

async fn run_pricing_refresh_loop(cost_store: Arc<RwLock<CostStore>>) {
    loop {
        let refresh_result = {
            let mut cost_store = cost_store.write().await;
            cost_store.refresh_pricing(true).await
        };

        match refresh_result {
            Ok(PricingRefreshResult::Refreshed) => {
                break;
            }
            Ok(PricingRefreshResult::Skipped) => {
                break;
            }
            Ok(PricingRefreshResult::Failed) => {
                tracing::warn!("Pricing refresh failed, retrying in 5 minutes");
            }
            Err(e) => {
                tracing::warn!(error = %e, "Pricing refresh failed, retrying in 5 minutes");
            }
        }

        tokio::time::sleep(Duration::from_secs(300)).await;
    }
}

async fn run_cost_scan_loop(
    cost_store: Arc<RwLock<CostStore>>,
    store: Arc<UsageStore>,
    ui_tx: mpsc::UnboundedSender<UiCommand>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(300));

    scan_and_update_costs(&cost_store, &store, &ui_tx).await;

    loop {
        interval.tick().await;
        scan_and_update_costs(&cost_store, &store, &ui_tx).await;
    }
}

async fn scan_and_update_costs(
    cost_store: &Arc<RwLock<CostStore>>,
    store: &Arc<UsageStore>,
    ui_tx: &mpsc::UnboundedSender<UiCommand>,
) {
    let costs = {
        let mut cost_store = cost_store.write().await;
        cost_store.scan_all()
    };

    for (provider, result) in costs {
        store.update_cost(provider, result.cost.clone()).await;
        store
            .update_token_snapshot(provider, result.tokens.clone())
            .await;
        let _ = ui_tx.send(UiCommand::UpdateCost {
            provider,
            cost: Box::new(result.cost),
        });
        let _ = ui_tx.send(UiCommand::UpdateTokens {
            provider,
            tokens: Box::new(result.tokens),
        });
    }
}

async fn refresh_provider_with_retry(
    registry: &Arc<ProviderRegistry>,
    store: &Arc<UsageStore>,
    tray: &Arc<TrayManager>,
    retry_states: &Arc<RwLock<HashMap<Provider, RetryState>>>,
    ui_tx: &mpsc::UnboundedSender<UiCommand>,
    provider: Provider,
) {
    let has_creds = registry
        .get_provider(provider)
        .is_some_and(|p| p.has_valid_credentials());

    if !has_creds {
        let hint = registry
            .get_provider(provider)
            .map(|p| p.credential_error_hint())
            .unwrap_or("Check credentials");
        tracing::debug!(?provider, "Skipping fetch: credentials missing or expired");
        store
            .set_error(provider, format!("Token expired or missing. {hint}"))
            .await;
        tray.set_error(provider).await;
        return;
    }

    match registry.fetch_provider(provider).await {
        Ok(snapshot) => {
            {
                let mut states = retry_states.write().await;
                if let Some(state) = states.get_mut(&provider) {
                    if state.is_in_backoff() {
                        tracing::info!(
                            ?provider,
                            failures = state.consecutive_failures(),
                            "Provider recovered from error state"
                        );
                    }
                    state.record_success();
                }
            }
            apply_successful_fetch(provider, snapshot, store, tray, ui_tx).await;
        }
        Err(e) => {
            let (next_delay, failures) = {
                let mut states = retry_states.write().await;
                let state = states.entry(provider).or_default();
                state.record_failure();
                (state.current_delay(), state.consecutive_failures())
            };

            let error_msg = e.to_string();
            tracing::warn!(
                ?provider,
                error = %error_msg,
                consecutive_failures = failures,
                next_retry_secs = next_delay.as_secs(),
                "Failed to fetch usage, backing off"
            );
            store.set_error(provider, error_msg).await;
            tray.set_error(provider).await;
        }
    }
}

async fn refresh_provider(
    registry: &Arc<ProviderRegistry>,
    store: &Arc<UsageStore>,
    tray: &Arc<TrayManager>,
    ui_tx: &mpsc::UnboundedSender<UiCommand>,
    provider: Provider,
) {
    match registry.fetch_provider(provider).await {
        Ok(snapshot) => {
            apply_successful_fetch(provider, snapshot, store, tray, ui_tx).await;
        }
        Err(e) => {
            apply_failed_fetch(provider, &e, store, tray).await;
        }
    }
}

fn provider_error_hint(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "Run `claude` to authenticate",
        Provider::Codex => "Run `codex` to authenticate",
    }
}

fn extract_percentages(snapshot: &UsageSnapshot) -> (f64, f64) {
    let primary = snapshot.primary.as_ref().map_or(0.0, |r| r.used_percent);
    let secondary = snapshot.secondary.as_ref().map_or(0.0, |r| r.used_percent);
    (primary, secondary)
}

async fn apply_successful_fetch(
    provider: Provider,
    snapshot: UsageSnapshot,
    store: &Arc<UsageStore>,
    tray: &Arc<TrayManager>,
    ui_tx: &mpsc::UnboundedSender<UiCommand>,
) {
    let (primary, secondary) = extract_percentages(&snapshot);
    store.update_snapshot(provider, snapshot.clone()).await;
    tray.update_icon(provider, primary, secondary).await;
    tray.set_credentials_valid(provider, true).await;
    let _ = ui_tx.send(UiCommand::UpdateUsage {
        provider,
        snapshot: Box::new(snapshot),
    });
}

async fn apply_failed_fetch(
    provider: Provider,
    error: &anyhow::Error,
    store: &Arc<UsageStore>,
    tray: &Arc<TrayManager>,
) {
    let error_msg = error.to_string();
    tracing::warn!(?provider, error = %error_msg, "Failed to fetch usage");
    store.set_error(provider, error_msg).await;
    tray.set_error(provider).await;
}

fn start_global_shortcut(
    settings: &Settings,
    store: Arc<UsageStore>,
    ui_tx: mpsc::UnboundedSender<UiCommand>,
    registry: Arc<ProviderRegistry>,
) {
    if !settings.shortcuts.enabled {
        return;
    }

    let Some(hotkey) = parse_hotkey(&settings.shortcuts.popup) else {
        tracing::warn!("Failed to parse shortcut; global hotkey disabled");
        return;
    };

    let manager = match GlobalHotKeyManager::new() {
        Ok(manager) => manager,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to create hotkey manager");
            return;
        }
    };

    if let Err(e) = manager.register(hotkey) {
        tracing::warn!(error = %e, "Failed to register global hotkey");
        return;
    }

    let provider = registry
        .enabled_provider_ids()
        .first()
        .copied()
        .unwrap_or(Provider::Claude);

    let receiver = GlobalHotKeyEvent::receiver();
    std::thread::spawn(move || {
        let _manager = manager;
        while let Ok(event) = receiver.recv() {
            if event.id == hotkey.id() {
                let store = Arc::clone(&store);
                let ui_tx = ui_tx.clone();
                tokio::spawn(async move {
                    let snapshot = store.get_snapshot(provider).await.map(Box::new);
                    let cost = store.get_cost(provider).await.map(Box::new);
                    let tokens = store.get_token_snapshot(provider).await.map(Box::new);
                    let error = store
                        .get_error(provider)
                        .await
                        .map(|e| (e, provider_error_hint(provider).to_string()));
                    let _ = ui_tx.send(UiCommand::ShowPopup {
                        provider,
                        snapshot,
                        cost,
                        tokens,
                        error,
                    });
                });
            }
        }
    });
}

fn parse_hotkey(input: &str) -> Option<HotKey> {
    let mut modifiers = Modifiers::empty();
    let mut key = None;

    for raw in input.split('+') {
        let part = raw.trim().to_lowercase();
        if part.is_empty() {
            continue;
        }
        match part.as_str() {
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "shift" => modifiers |= Modifiers::SHIFT,
            "alt" | "option" => modifiers |= Modifiers::ALT,
            "super" | "cmd" | "meta" => modifiers |= Modifiers::SUPER,
            _ => {
                key = key_code_for(&part);
            }
        }
    }

    let key = key?;
    Some(HotKey::new(Some(modifiers), key))
}

fn key_code_for(input: &str) -> Option<Code> {
    if input.len() == 1 {
        let ch = input.chars().next()?.to_ascii_uppercase();
        return match ch {
            'A' => Some(Code::KeyA),
            'B' => Some(Code::KeyB),
            'C' => Some(Code::KeyC),
            'D' => Some(Code::KeyD),
            'E' => Some(Code::KeyE),
            'F' => Some(Code::KeyF),
            'G' => Some(Code::KeyG),
            'H' => Some(Code::KeyH),
            'I' => Some(Code::KeyI),
            'J' => Some(Code::KeyJ),
            'K' => Some(Code::KeyK),
            'L' => Some(Code::KeyL),
            'M' => Some(Code::KeyM),
            'N' => Some(Code::KeyN),
            'O' => Some(Code::KeyO),
            'P' => Some(Code::KeyP),
            'Q' => Some(Code::KeyQ),
            'R' => Some(Code::KeyR),
            'S' => Some(Code::KeyS),
            'T' => Some(Code::KeyT),
            'U' => Some(Code::KeyU),
            'V' => Some(Code::KeyV),
            'W' => Some(Code::KeyW),
            'X' => Some(Code::KeyX),
            'Y' => Some(Code::KeyY),
            'Z' => Some(Code::KeyZ),
            '0' => Some(Code::Digit0),
            '1' => Some(Code::Digit1),
            '2' => Some(Code::Digit2),
            '3' => Some(Code::Digit3),
            '4' => Some(Code::Digit4),
            '5' => Some(Code::Digit5),
            '6' => Some(Code::Digit6),
            '7' => Some(Code::Digit7),
            '8' => Some(Code::Digit8),
            '9' => Some(Code::Digit9),
            _ => None,
        };
    }

    None
}
