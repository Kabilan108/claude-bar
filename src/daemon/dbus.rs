use zbus::interface;

pub struct ClaudeBarService {
    is_refreshing: bool,
}

impl ClaudeBarService {
    pub fn new() -> Self {
        Self {
            is_refreshing: false,
        }
    }
}

impl Default for ClaudeBarService {
    fn default() -> Self {
        Self::new()
    }
}

#[interface(name = "com.github.kabilan.ClaudeBar")]
impl ClaudeBarService {
    async fn refresh(&mut self) -> zbus::fdo::Result<()> {
        tracing::info!("D-Bus Refresh called");
        // TODO: Trigger actual refresh of provider data
        Ok(())
    }

    #[zbus(property)]
    fn is_refreshing(&self) -> bool {
        self.is_refreshing
    }

    #[zbus(signal)]
    async fn usage_updated(ctx: &zbus::SignalContext<'_>, provider: &str) -> zbus::Result<()>;
}
