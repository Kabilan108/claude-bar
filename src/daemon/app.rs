use crate::core::models::{CostSnapshot, Provider, UsageSnapshot};
use crate::core::retry::RetryState;
use crate::core::settings::SettingsWatcher;
use crate::core::store::UsageStore;
use crate::daemon::dbus::{start_dbus_server, DbusCommand};
use crate::daemon::tray::{run_animation_loop, TrayEvent, TrayManager};
use crate::providers::ProviderRegistry;
use crate::ui::PopupWindow;
use anyhow::Result;
use gtk4::glib;
use gtk4::prelude::*;
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

    let settings_watcher = SettingsWatcher::new()?;
    let settings = settings_watcher.get().await;

    let store = Arc::new(UsageStore::new());
    let tray_manager = Arc::new(TrayManager::new());
    let retry_states = Arc::new(RwLock::new(HashMap::<Provider, RetryState>::new()));

    let registry = Arc::new(ProviderRegistry::new(&settings));

    tray_manager.start(&settings).await?;
    tokio::spawn(run_animation_loop(Arc::clone(&tray_manager)));

    let (ui_tx, ui_rx) = mpsc::unbounded_channel::<UiCommand>();

    let (dbus_cmd_tx, dbus_cmd_rx) = mpsc::unbounded_channel::<DbusCommand>();
    let _dbus_connection = start_dbus_server(dbus_cmd_tx).await?;

    tokio::spawn(handle_dbus_commands(
        dbus_cmd_rx,
        Arc::clone(&registry),
        Arc::clone(&store),
        Arc::clone(&tray_manager),
        ui_tx.clone(),
    ));

    tokio::spawn(run_polling_loop(
        Arc::clone(&registry),
        Arc::clone(&store),
        Arc::clone(&tray_manager),
        Arc::clone(&retry_states),
        ui_tx.clone(),
    ));

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

    run_gtk_main_loop(ui_rx).await
}

async fn handle_dbus_commands(
    mut cmd_rx: mpsc::UnboundedReceiver<DbusCommand>,
    registry: Arc<ProviderRegistry>,
    store: Arc<UsageStore>,
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
        }
    }
}

#[derive(Debug, Clone)]
enum UiCommand {
    ShowPopup {
        provider: Provider,
        snapshot: Option<UsageSnapshot>,
        cost: Option<CostSnapshot>,
        error: Option<(String, String)>,
    },
    UpdateUsage {
        provider: Provider,
        snapshot: UsageSnapshot,
    },
}

async fn run_gtk_main_loop(mut ui_rx: mpsc::UnboundedReceiver<UiCommand>) -> Result<()> {
    gtk4::init().expect("Failed to initialize GTK4");
    adw::init().expect("Failed to initialize libadwaita");

    let app = adw::Application::builder().application_id(APP_ID).build();
    let popup_holder: Rc<RefCell<Option<PopupWindow>>> = Rc::new(RefCell::new(None));

    let popup_holder_activate = popup_holder.clone();
    app.connect_activate(move |app| {
        tracing::info!("GTK application activated");
        let popup = PopupWindow::new(app);
        *popup_holder_activate.borrow_mut() = Some(popup);
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
            }
            popup.show(provider);
        }
        UiCommand::UpdateUsage { provider, snapshot } => {
            popup.update_usage(provider, &snapshot);
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

            let snapshot = store.get_snapshot(provider).await;
            let cost = store.get_cost(provider).await;
            let error = store
                .get_error(provider)
                .await
                .map(|e| (e, provider_error_hint(provider).to_string()));

            let _ = ui_tx.send(UiCommand::ShowPopup {
                provider,
                snapshot,
                cost,
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
        check_interval.tick().await;

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
}

async fn refresh_provider_with_retry(
    registry: &Arc<ProviderRegistry>,
    store: &Arc<UsageStore>,
    tray: &Arc<TrayManager>,
    retry_states: &Arc<RwLock<HashMap<Provider, RetryState>>>,
    ui_tx: &mpsc::UnboundedSender<UiCommand>,
    provider: Provider,
) {
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
    let _ = ui_tx.send(UiCommand::UpdateUsage { provider, snapshot });
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
