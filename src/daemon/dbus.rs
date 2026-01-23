use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use zbus::interface;

#[derive(Debug)]
pub enum DbusCommand {
    Refresh,
    RefreshPricing,
}

pub struct ClaudeBarService {
    is_refreshing: Arc<AtomicBool>,
    command_tx: mpsc::UnboundedSender<DbusCommand>,
}

impl ClaudeBarService {
    fn new(command_tx: mpsc::UnboundedSender<DbusCommand>) -> Self {
        Self {
            is_refreshing: Arc::new(AtomicBool::new(false)),
            command_tx,
        }
    }

    #[allow(dead_code)]
    pub fn set_refreshing(&self, refreshing: bool) {
        self.is_refreshing.store(refreshing, Ordering::SeqCst);
    }
}

#[interface(name = "com.github.kabilan.ClaudeBar")]
impl ClaudeBarService {
    async fn refresh(&self) -> zbus::fdo::Result<()> {
        tracing::info!("D-Bus Refresh called");
        self.command_tx
            .send(DbusCommand::Refresh)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    #[zbus(name = "RefreshPricing")]
    async fn refresh_pricing(&self) -> zbus::fdo::Result<()> {
        tracing::info!("D-Bus RefreshPricing called");
        self.command_tx
            .send(DbusCommand::RefreshPricing)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    #[zbus(property)]
    fn is_refreshing(&self) -> bool {
        self.is_refreshing.load(Ordering::SeqCst)
    }

    #[zbus(signal)]
    async fn usage_updated(ctx: &zbus::SignalContext<'_>, provider: &str) -> zbus::Result<()>;
}

pub const DBUS_NAME: &str = "com.github.kabilan.ClaudeBar";
pub const DBUS_PATH: &str = "/com/github/kabilan/ClaudeBar";

pub async fn start_dbus_server(
    command_tx: mpsc::UnboundedSender<DbusCommand>,
) -> anyhow::Result<zbus::Connection> {
    let service = ClaudeBarService::new(command_tx);

    let connection = zbus::connection::Builder::session()?
        .name(DBUS_NAME)?
        .serve_at(DBUS_PATH, service)?
        .build()
        .await?;

    tracing::info!("D-Bus server started at {}", DBUS_NAME);

    Ok(connection)
}
