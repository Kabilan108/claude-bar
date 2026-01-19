mod app;
mod dbus;
mod polling;
mod tray;

use anyhow::Result;

pub async fn run() -> Result<()> {
    tracing::info!("Starting claude-bar daemon");

    // TODO: Initialize GTK application, tray, polling loop, and D-Bus interface
    app::run().await
}
