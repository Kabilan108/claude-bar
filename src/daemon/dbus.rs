use std::sync::Arc;
use tokio::sync::mpsc;
use zbus::interface;

#[derive(Debug)]
pub enum DbusCommand {
    Refresh,
}

pub struct ClaudeBarService {
    is_refreshing: Arc<std::sync::atomic::AtomicBool>,
    command_tx: mpsc::UnboundedSender<DbusCommand>,
}

impl ClaudeBarService {
    pub fn new(command_tx: mpsc::UnboundedSender<DbusCommand>) -> Self {
        Self {
            is_refreshing: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            command_tx,
        }
    }

    pub fn set_refreshing(&self, refreshing: bool) {
        self.is_refreshing
            .store(refreshing, std::sync::atomic::Ordering::SeqCst);
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

    #[zbus(property)]
    fn is_refreshing(&self) -> bool {
        self.is_refreshing
            .load(std::sync::atomic::Ordering::SeqCst)
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
