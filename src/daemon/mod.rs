mod app;
pub mod dbus;
pub mod login;
pub mod tray;

use anyhow::Result;

#[allow(unused_imports)]
pub use dbus::{start_dbus_server, DbusCommand, DBUS_NAME, DBUS_PATH};
#[allow(unused_imports)]
pub use tray::{run_animation_loop, TrayEvent, TrayManager};

pub async fn run() -> Result<()> {
    tracing::info!("Starting claude-bar daemon");
    app::run().await
}
