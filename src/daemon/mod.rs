mod app;
pub mod dbus;
mod polling;
pub mod tray;

use anyhow::Result;

pub use dbus::{start_dbus_server, DbusCommand, DBUS_NAME, DBUS_PATH};
pub use tray::{run_animation_loop, TrayEvent, TrayManager};

pub async fn run() -> Result<()> {
    tracing::info!("Starting claude-bar daemon");
    app::run().await
}
