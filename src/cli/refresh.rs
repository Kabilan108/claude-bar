use crate::daemon::{DBUS_NAME, DBUS_PATH};
use anyhow::{Context, Result};

pub async fn run() -> Result<()> {
    let connection = zbus::Connection::session()
        .await
        .context("Failed to connect to session D-Bus")?;

    let _reply: () = connection
        .call_method(
            Some(DBUS_NAME),
            DBUS_PATH,
            Some(DBUS_NAME),
            "Refresh",
            &(),
        )
        .await
        .context("Failed to call Refresh method - is the daemon running?")?
        .body()
        .deserialize()
        .context("Failed to deserialize response")?;

    println!("Refresh triggered successfully");
    Ok(())
}
