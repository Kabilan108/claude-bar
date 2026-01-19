use crate::core::models::{Provider, ProviderIdentity, UsageSnapshot};
use crate::providers::UsageProvider;
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use std::path::PathBuf;

const CREDENTIALS_PATH: &str = ".claude/.credentials.json";
const API_ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeCredentials {
    access_token: String,
    #[allow(dead_code)]
    refresh_token: Option<String>,
    #[allow(dead_code)]
    expires_at: Option<String>,
}

pub struct ClaudeProvider {
    credentials_path: PathBuf,
}

impl ClaudeProvider {
    pub fn new() -> Self {
        let credentials_path = dirs::home_dir()
            .map(|p| p.join(CREDENTIALS_PATH))
            .unwrap_or_else(|| PathBuf::from(CREDENTIALS_PATH));

        Self { credentials_path }
    }

    fn load_credentials(&self) -> Result<ClaudeCredentials> {
        let content = std::fs::read_to_string(&self.credentials_path).with_context(|| {
            format!(
                "Failed to read credentials from {}",
                self.credentials_path.display()
            )
        })?;

        serde_json::from_str(&content).context("Failed to parse Claude credentials")
    }
}

impl Default for ClaudeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl UsageProvider for ClaudeProvider {
    fn name(&self) -> &'static str {
        "Claude Code"
    }

    fn identifier(&self) -> Provider {
        Provider::Claude
    }

    async fn fetch_usage(&self) -> Result<UsageSnapshot> {
        let credentials = self.load_credentials()?;

        let client = reqwest::Client::new();
        let response = client
            .get(API_ENDPOINT)
            .header("Authorization", format!("Bearer {}", credentials.access_token))
            .header("anthropic-beta", "oauth-2025-04-20")
            .send()
            .await
            .context("Failed to fetch Claude usage")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Claude API error: {} - {}", status, body);
        }

        // TODO: Parse actual response into UsageSnapshot
        // For now, return a placeholder
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
        "https://console.anthropic.com/"
    }

    fn has_valid_credentials(&self) -> bool {
        self.credentials_path.exists()
    }

    fn credential_error_hint(&self) -> &'static str {
        "Run `claude` to authenticate"
    }
}
