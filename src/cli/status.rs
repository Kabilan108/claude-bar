use crate::core::models::{ProviderIdentity, RateWindow, UsageSnapshot};
use crate::core::settings::Settings;
use crate::providers::{ClaudeProvider, CodexProvider, UsageProvider};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize)]
struct StatusOutput {
    providers: HashMap<String, ProviderStatus>,
    #[serde(with = "chrono::serde::ts_seconds")]
    fetched_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct ProviderStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    session: Option<WindowStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    weekly: Option<WindowStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    opus: Option<WindowStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    identity: Option<ProviderIdentity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct WindowStatus {
    used_percent: f64,
    remaining_percent: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    resets_in: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    window_minutes: Option<i32>,
}

pub async fn run(json: bool, provider_filter: Option<String>) -> Result<()> {
    let settings = Settings::load()?;

    let providers = build_provider_list(&settings, provider_filter.as_deref());

    if providers.is_empty() {
        if let Some(filter) = &provider_filter {
            anyhow::bail!("Unknown provider: {}. Valid providers: claude, codex", filter);
        } else {
            anyhow::bail!("No providers enabled. Check your configuration.");
        }
    }

    let mut results: HashMap<String, ProviderStatus> = HashMap::new();

    for provider in providers {
        let name = provider.name().to_string();
        let status = fetch_provider_status(provider.as_ref()).await;
        results.insert(name, status);
    }

    if json {
        let output = StatusOutput {
            providers: results,
            fetched_at: Utc::now(),
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_text_output(&results);
    }

    Ok(())
}

fn build_provider_list(
    settings: &Settings,
    provider_filter: Option<&str>,
) -> Vec<Box<dyn UsageProvider>> {
    let filter = provider_filter.map(|s| s.to_lowercase());
    let matches_filter = |name: &str| filter.is_none() || filter.as_deref() == Some(name);

    let mut providers: Vec<Box<dyn UsageProvider>> = Vec::new();

    if settings.providers.claude.enabled && matches_filter("claude") {
        providers.push(Box::new(ClaudeProvider::new()));
    }

    if settings.providers.codex.enabled && matches_filter("codex") {
        providers.push(Box::new(CodexProvider::new()));
    }

    providers
}

async fn fetch_provider_status(provider: &dyn UsageProvider) -> ProviderStatus {
    if !provider.has_valid_credentials() {
        return ProviderStatus {
            session: None,
            weekly: None,
            opus: None,
            identity: None,
            error: Some(provider.credential_error_hint().to_string()),
        };
    }

    match provider.fetch_usage().await {
        Ok(snapshot) => snapshot_to_status(snapshot),
        Err(e) => ProviderStatus {
            session: None,
            weekly: None,
            opus: None,
            identity: None,
            error: Some(e.to_string()),
        },
    }
}

fn snapshot_to_status(snapshot: UsageSnapshot) -> ProviderStatus {
    ProviderStatus {
        session: snapshot.primary.map(|w| window_to_status(&w)),
        weekly: snapshot.secondary.map(|w| window_to_status(&w)),
        opus: snapshot.opus.map(|w| window_to_status(&w)),
        identity: Some(snapshot.identity),
        error: None,
    }
}

fn window_to_status(window: &RateWindow) -> WindowStatus {
    WindowStatus {
        used_percent: window.used_percent,
        remaining_percent: window.remaining_percent(),
        resets_in: window.resets_at.map(format_reset_time),
        window_minutes: window.window_minutes,
    }
}

fn format_reset_time(resets_at: DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = resets_at.signed_duration_since(now);

    if duration.num_seconds() <= 0 {
        return "now".to_string();
    }

    let total_minutes = duration.num_minutes();
    let days = total_minutes / (24 * 60);
    let hours = (total_minutes % (24 * 60)) / 60;
    let minutes = total_minutes % 60;

    if days > 0 {
        format!("{}d {}h", days, hours)
    } else if hours > 0 {
        format!("{}h {:02}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

fn print_text_output(results: &HashMap<String, ProviderStatus>) {
    for (i, (name, status)) in results.iter().enumerate() {
        if i > 0 {
            println!();
        }

        println!("{}", name);

        if let Some(error) = &status.error {
            println!("  Error: {}", error);
            continue;
        }

        if let Some(session) = &status.session {
            print_window_line("Session", session);
        }

        if let Some(weekly) = &status.weekly {
            print_window_line("Weekly", weekly);
        }

        if let Some(opus) = &status.opus {
            print_window_line("Opus", opus);
        }
    }
}

fn print_window_line(label: &str, window: &WindowStatus) {
    let reset_info = window
        .resets_in
        .as_ref()
        .map(|r| format!(" (resets in {})", r))
        .unwrap_or_default();

    println!(
        "  {:<8} {:>5.1}% used{}",
        format!("{}:", label),
        window.used_percent * 100.0,
        reset_info
    );
}
