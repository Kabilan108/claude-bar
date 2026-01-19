use anyhow::Result;

const APP_ID: &str = "com.github.kabilan.claude-bar";

pub async fn run() -> Result<()> {
    tracing::info!(app_id = APP_ID, "Initializing GTK application");

    // TODO: Initialize GTK4 + libadwaita application
    // - Set application ID for Hyprland window rules
    // - Handle single-instance via D-Bus activation
    // - Start tray and polling loop

    println!("Daemon mode not yet implemented");
    println!("Application ID: {}", APP_ID);

    Ok(())
}
