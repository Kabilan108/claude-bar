use anyhow::{Context, Result};

pub async fn run() -> Result<()> {
    tracing::info!("Triggering daemon refresh via D-Bus");

    // TODO: Implement D-Bus call to running daemon
    let connection = zbus::Connection::session()
        .await
        .context("Failed to connect to session D-Bus")?;

    // Check if daemon is running by trying to call the refresh method
    let _reply: () = connection
        .call_method(
            Some("com.github.kabilan.ClaudeBar"),
            "/com/github/kabilan/ClaudeBar",
            Some("com.github.kabilan.ClaudeBar"),
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
