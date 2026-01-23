use crate::core::models::{Provider, ProviderIdentity, RateWindow, UsageSnapshot};
use crate::providers::UsageProvider;
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::path::PathBuf;
use tracing::{debug, warn};

const DEFAULT_CREDENTIALS_PATH: &str = ".codex/auth.json";
const API_ENDPOINT: &str = "https://chatgpt.com/backend-api/wham/usage";

#[derive(Debug, Deserialize)]
struct CredentialsFile {
    tokens: TokenData,
}

#[derive(Debug, Deserialize)]
struct TokenData {
    access_token: String,
    #[allow(dead_code)]
    refresh_token: Option<String>,
    #[allow(dead_code)]
    id_token: Option<String>,
    account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CodexUsageResponse {
    plan_type: Option<String>,
    rate_limit: Option<RateLimitInfo>,
}

#[derive(Debug, Deserialize)]
struct RateLimitInfo {
    primary_window: Option<RateLimitWindow>,
    secondary_window: Option<RateLimitWindow>,
}

#[derive(Debug, Deserialize)]
struct RateLimitWindow {
    used_percent: i32,
    reset_at: Option<i64>,
    limit_window_seconds: Option<i32>,
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

    fn load_credentials(&self) -> Result<TokenData> {
        let content = std::fs::read_to_string(&self.credentials_path).with_context(|| {
            format!(
                "Failed to read credentials from {}",
                self.credentials_path.display()
            )
        })?;

        let file: CredentialsFile =
            serde_json::from_str(&content).context("Failed to parse Codex credentials")?;

        if file.tokens.access_token.is_empty() {
            anyhow::bail!("Codex access token is empty");
        }

        Ok(file.tokens)
    }

    fn parse_reset_time(reset_at: Option<i64>) -> Option<DateTime<Utc>> {
        reset_at.and_then(|ts| {
            DateTime::from_timestamp(ts, 0).or_else(|| {
                warn!("Failed to parse Codex reset timestamp: {}", ts);
                None
            })
        })
    }

    fn window_to_rate_window(window: Option<&RateLimitWindow>, description: &str) -> Option<RateWindow> {
        window.map(|w| {
            let window_minutes = w.limit_window_seconds.map(|s| s / 60);
            RateWindow {
                used_percent: f64::from(w.used_percent) / 100.0,
                window_minutes,
                resets_at: Self::parse_reset_time(w.reset_at),
                reset_description: Some(description.to_string()),
            }
        })
    }

    fn format_plan_type(plan_type: Option<&str>) -> Option<String> {
        plan_type.map(|p| {
            match p.to_lowercase().as_str() {
                "plus" => "ChatGPT Plus".to_string(),
                "pro" => "ChatGPT Pro".to_string(),
                "team" => "ChatGPT Team".to_string(),
                "enterprise" => "ChatGPT Enterprise".to_string(),
                "free" => "ChatGPT Free".to_string(),
                _ => format!("ChatGPT {}", p),
            }
        })
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

        debug!("Fetching Codex usage from {}", API_ENDPOINT);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to build HTTP client")?;

        let mut request = client
            .get(API_ENDPOINT)
            .header("Authorization", format!("Bearer {}", credentials.access_token))
            .header("Accept", "application/json")
            .header("User-Agent", "claude-bar");

        if let Some(account_id) = &credentials.account_id {
            if !account_id.is_empty() {
                request = request.header("ChatGPT-Account-Id", account_id);
            }
        }

        let response = request.send().await.context("Failed to fetch Codex usage")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            if status.as_u16() == 401 || status.as_u16() == 403 {
                anyhow::bail!("Codex authentication failed. Run `codex` to refresh credentials.");
            }
            anyhow::bail!("Codex API error: {} - {}", status, body);
        }

        let body = response.text().await?;
        debug!("Codex API response: {}", body);

        let usage: CodexUsageResponse =
            serde_json::from_str(&body).context("Failed to parse Codex usage response")?;

        let (primary, secondary) = usage.rate_limit.as_ref().map_or((None, None), |rl| {
            (
                Self::window_to_rate_window(rl.primary_window.as_ref(), "Session limit"),
                Self::window_to_rate_window(rl.secondary_window.as_ref(), "Weekly limit"),
            )
        });

        let plan = Self::format_plan_type(usage.plan_type.as_deref());

        Ok(UsageSnapshot {
            primary,
            secondary,
            carveouts: Vec::new(),
            updated_at: Utc::now(),
            identity: ProviderIdentity {
                email: None,
                organization: None,
                plan,
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_parse_credentials() {
        let json = r#"{
            "tokens": {
                "access_token": "test-token-123",
                "refresh_token": "refresh-token-456",
                "id_token": "id-token-789",
                "account_id": "account-abc"
            },
            "last_refresh": "2026-01-19T00:00:00Z"
        }"#;

        let file: CredentialsFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.tokens.access_token, "test-token-123");
        assert_eq!(
            file.tokens.refresh_token,
            Some("refresh-token-456".to_string())
        );
        assert_eq!(file.tokens.account_id, Some("account-abc".to_string()));
    }

    #[test]
    fn test_parse_usage_response() {
        let json = r#"{
            "plan_type": "plus",
            "rate_limit": {
                "primary_window": {
                    "used_percent": 45,
                    "reset_at": 1737298200,
                    "limit_window_seconds": 10800
                },
                "secondary_window": {
                    "used_percent": 25,
                    "reset_at": 1737903000,
                    "limit_window_seconds": 604800
                }
            }
        }"#;

        let usage: CodexUsageResponse = serde_json::from_str(json).unwrap();

        assert_eq!(usage.plan_type, Some("plus".to_string()));
        assert!(usage.rate_limit.is_some());

        let rate_limit = usage.rate_limit.as_ref().unwrap();
        assert!(rate_limit.primary_window.is_some());
        assert!(rate_limit.secondary_window.is_some());

        let primary = rate_limit.primary_window.as_ref().unwrap();
        assert_eq!(primary.used_percent, 45);
        assert_eq!(primary.limit_window_seconds, Some(10800));
    }

    #[test]
    fn test_parse_usage_response_minimal() {
        let json = r#"{
            "plan_type": null,
            "rate_limit": null
        }"#;

        let usage: CodexUsageResponse = serde_json::from_str(json).unwrap();
        assert!(usage.plan_type.is_none());
        assert!(usage.rate_limit.is_none());
    }

    #[test]
    fn test_parse_reset_time() {
        let valid_timestamp = 1737298200i64;
        let parsed = CodexProvider::parse_reset_time(Some(valid_timestamp));
        assert!(parsed.is_some());
        let dt = parsed.unwrap();
        assert_eq!(dt.year(), 2025);

        let none_time = CodexProvider::parse_reset_time(None);
        assert!(none_time.is_none());
    }

    #[test]
    fn test_window_to_rate_window() {
        let window = RateLimitWindow {
            used_percent: 45,
            reset_at: Some(1737298200),
            limit_window_seconds: Some(10800),
        };

        let rate_window = CodexProvider::window_to_rate_window(Some(&window), "Session limit");
        assert!(rate_window.is_some());

        let rw = rate_window.unwrap();
        assert!((rw.used_percent - 0.45).abs() < 0.001);
        assert_eq!(rw.window_minutes, Some(180));
        assert!(rw.resets_at.is_some());
        assert_eq!(rw.reset_description, Some("Session limit".to_string()));
    }

    #[test]
    fn test_format_plan_type() {
        assert_eq!(
            CodexProvider::format_plan_type(Some("plus")),
            Some("ChatGPT Plus".to_string())
        );
        assert_eq!(
            CodexProvider::format_plan_type(Some("pro")),
            Some("ChatGPT Pro".to_string())
        );
        assert_eq!(
            CodexProvider::format_plan_type(Some("team")),
            Some("ChatGPT Team".to_string())
        );
        assert_eq!(
            CodexProvider::format_plan_type(Some("enterprise")),
            Some("ChatGPT Enterprise".to_string())
        );
        assert_eq!(
            CodexProvider::format_plan_type(Some("free")),
            Some("ChatGPT Free".to_string())
        );
        assert_eq!(
            CodexProvider::format_plan_type(Some("custom")),
            Some("ChatGPT custom".to_string())
        );
        assert_eq!(CodexProvider::format_plan_type(None), None);
    }

    #[test]
    fn test_provider_metadata() {
        let provider = CodexProvider::new();
        assert_eq!(provider.name(), "Codex");
        assert_eq!(provider.identifier(), Provider::Codex);
        assert_eq!(provider.dashboard_url(), "https://chatgpt.com/");
        assert_eq!(provider.credential_error_hint(), "Run `codex` to authenticate");
    }
}
