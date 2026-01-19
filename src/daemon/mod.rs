mod app;
mod dbus;
mod polling;
pub mod tray;

use anyhow::Result;

pub use tray::{run_animation_loop, TrayEvent, TrayManager};

pub async fn run() -> Result<()> {
    tracing::info!("Starting claude-bar daemon");

    // TODO: Initialize GTK application, tray, polling loop, and D-Bus interface
    app::run().await
}
