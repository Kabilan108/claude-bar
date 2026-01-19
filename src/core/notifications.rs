use crate::core::models::Provider;
use anyhow::Result;
use notify_rust::Notification;

pub fn send_high_usage_notification(provider: Provider, percent: f64) -> Result<()> {
    let percent_display = (percent * 100.0).round() as u32;

    Notification::new()
        .summary(&format!("{} Usage Warning", provider.name()))
        .body(&format!(
            "You've used {}% of your {} quota.",
            percent_display,
            provider.name()
        ))
        .appname("claude-bar")
        .timeout(notify_rust::Timeout::Milliseconds(5000))
        .show()?;

    tracing::info!(
        provider = ?provider,
        percent = percent_display,
        "Sent high usage notification"
    );

    Ok(())
}
