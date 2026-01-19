use std::time::Duration;

pub const POLL_INTERVAL: Duration = Duration::from_secs(60);
pub const REFRESH_COOLDOWN: Duration = Duration::from_secs(5);

pub struct PollingLoop {
    // TODO: Background task handles
}

impl PollingLoop {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn start(&mut self) {
        // TODO: Start 60-second polling loop for usage data
        // TODO: Start 60-second polling loop for cost scanning
        tracing::info!("Polling loop started (interval: {:?})", POLL_INTERVAL);
    }

    pub fn trigger_refresh(&mut self) {
        // TODO: Trigger immediate refresh (respecting cooldown)
    }
}

impl Default for PollingLoop {
    fn default() -> Self {
        Self::new()
    }
}
