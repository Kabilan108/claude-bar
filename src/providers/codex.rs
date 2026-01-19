use crate::core::models::{Provider, ProviderIdentity, UsageSnapshot};
use crate::providers::UsageProvider;
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use std::path::PathBuf;

const DEFAULT_CREDENTIALS_PATH: &str = ".codex/auth.json";
const API_ENDPOINT: &str = "https://chatgpt.com/backend-api/wham/usage";

#[derive(Debug, Deserialize)]
struct CodexCredentials {
    access_token: String,
}

pub struct CodexProvider {
    credentials_path: PathBuf,
}

impl CodexProvider {
    pub fn new() -> Self {
        let credentials_path = std::env::var("CODEX_HOME")
            .map(|home| PathBuf::from(home).join("auth.json"))
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .map(|p| p.join(DEFAULT_CREDENTIALS_PATH))
                    .unwrap_or_else(|| PathBuf::from(DEFAULT_CREDENTIALS_PATH))
            });

        Self { credentials_path }
    }

    fn load_credentials(&self) -> Result<CodexCredentials> {
        let content = std::fs::read_to_string(&self.credentials_path).with_context(|| {
            format!(
                "Failed to read credentials from {}",
                self.credentials_path.display()
            )
        })?;

        serde_json::from_str(&content).context("Failed to parse Codex credentials")
    }
}

impl Default for CodexProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl UsageProvider for CodexProvider {
    fn name(&self) -> &'static str {
        "Codex"
    }

    fn identifier(&self) -> Provider {
        Provider::Codex
    }

    async fn fetch_usage(&self) -> Result<UsageSnapshot> {
        let credentials = self.load_credentials()?;

        let client = reqwest::Client::new();
        let response = client
            .get(API_ENDPOINT)
            .header("Authorization", format!("Bearer {}", credentials.access_token))
            .send()
            .await
            .context("Failed to fetch Codex usage")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Codex API error: {} - {}", status, body);
        }

        // TODO: Parse actual response into UsageSnapshot
        let _body = response.text().await?;

        Ok(UsageSnapshot {
            primary: None,
            secondary: None,
            opus: None,
            updated_at: Utc::now(),
            identity: ProviderIdentity {
                email: None,
                organization: None,
                plan: None,
            },
        })
    }

    fn dashboard_url(&self) -> &'static str {
        "https://chatgpt.com/"
    }

    fn has_valid_credentials(&self) -> bool {
        self.credentials_path.exists()
    }

    fn credential_error_hint(&self) -> &'static str {
        "Run `codex` to authenticate"
    }
}
