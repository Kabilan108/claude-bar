use crate::core::models::{ModelWindow, Provider, ProviderIdentity, RateWindow, UsageSnapshot};
use crate::providers::UsageProvider;
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
#[cfg(test)]
use chrono::Datelike;
use serde::Deserialize;
use std::path::PathBuf;
use tracing::{debug, warn};

const DEFAULT_CREDENTIALS_PATH: &str = ".claude/.credentials.json";
const API_ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";

#[derive(Debug, Deserialize)]
struct CredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: ClaudeOAuthCredentials,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeOAuthCredentials {
    access_token: String,
    #[allow(dead_code)]
    refresh_token: Option<String>,
    expires_at: Option<i64>,
    #[allow(dead_code)]
    scopes: Option<Vec<String>>,
    rate_limit_tier: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OAuthUsageResponse {
    five_hour: Option<UsageWindow>,
    seven_day: Option<UsageWindow>,
    #[serde(rename = "seven_day_sonnet")]
    seven_day_sonnet: Option<UsageWindow>,
    #[serde(rename = "seven_day_opus")]
    seven_day_opus: Option<UsageWindow>,
}

#[derive(Debug, Deserialize)]
struct UsageWindow {
    utilization: f64,
    resets_at: Option<String>,
}

pub struct ClaudeProvider {
    credentials_path: PathBuf,
}

impl ClaudeProvider {
    pub fn new() -> Self {
        let credentials_path = dirs::home_dir()
            .map(|p| p.join(DEFAULT_CREDENTIALS_PATH))
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CREDENTIALS_PATH));

        Self { credentials_path }
    }

    fn load_credentials(&self) -> Result<ClaudeOAuthCredentials> {
        let content = std::fs::read_to_string(&self.credentials_path).with_context(|| {
            format!(
                "Failed to read credentials from {}",
                self.credentials_path.display()
            )
        })?;

        let file: CredentialsFile =
            serde_json::from_str(&content).context("Failed to parse Claude credentials")?;

        if file.claude_ai_oauth.access_token.is_empty() {
            anyhow::bail!("Claude access token is empty");
        }

        Ok(file.claude_ai_oauth)
    }

    fn parse_reset_time(resets_at: Option<&str>) -> Option<DateTime<Utc>> {
        resets_at.and_then(|s| {
            DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&Utc))
                .inspect_err(|e| {
                    warn!("Failed to parse reset time '{}': {}", s, e);
                })
                .ok()
        })
    }

    fn window_to_rate_window(
        window: Option<&UsageWindow>,
        window_minutes: i32,
        description: &str,
    ) -> Option<RateWindow> {
        window.map(|w| RateWindow {
            used_percent: w.utilization / 100.0,
            window_minutes: Some(window_minutes),
            resets_at: Self::parse_reset_time(w.resets_at.as_deref()),
            reset_description: Some(description.to_string()),
        })
    }

    fn infer_plan_from_tier(tier: Option<&str>) -> Option<String> {
        tier.map(|t| {
            let lower = t.to_lowercase();
            if lower.contains("max") {
                "Claude Max".to_string()
            } else if lower.contains("enterprise") {
                "Claude Enterprise".to_string()
            } else if lower.contains("team") {
                "Claude Team".to_string()
            } else if lower.contains("pro") {
                "Claude Pro".to_string()
            } else {
                "Claude".to_string()
            }
        })
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

        if let Some(expires_at_ms) = credentials.expires_at {
            let now_ms = chrono::Utc::now().timestamp_millis();
            if now_ms >= expires_at_ms - 60_000 {
                anyhow::bail!(
                    "Claude token expired. Waiting for Claude Code to refresh credentials."
                );
            }
        }

        debug!("Fetching Claude usage from {}", API_ENDPOINT);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to build HTTP client")?;

        let response = client
            .get(API_ENDPOINT)
            .header("Authorization", format!("Bearer {}", credentials.access_token))
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("anthropic-beta", "oauth-2025-04-20")
            .header("User-Agent", "claude-bar")
            .send()
            .await
            .context("Failed to fetch Claude usage")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            if status.as_u16() == 401 {
                anyhow::bail!("Claude authentication failed. Run `claude` to refresh credentials.");
            } else if status.as_u16() == 403 {
                anyhow::bail!(
                    "Claude access forbidden. Credentials may be missing required scope (user:profile)."
                );
            }
            anyhow::bail!("Claude API error: {} - {}", status, body);
        }

        let body = response.text().await?;
        debug!("Claude API response: {}", body);

        let usage: OAuthUsageResponse =
            serde_json::from_str(&body).context("Failed to parse Claude usage response")?;

        let primary = Self::window_to_rate_window(usage.five_hour.as_ref(), 300, "5-hour session");

        let secondary =
            Self::window_to_rate_window(usage.seven_day.as_ref(), 10080, "Weekly quota");

        let mut carveouts = Vec::new();
        if let Some(window) =
            Self::window_to_rate_window(usage.seven_day_sonnet.as_ref(), 10080, "Sonnet weekly")
        {
            carveouts.push(ModelWindow {
                label: "Sonnet Weekly".to_string(),
                window,
            });
        }
        if let Some(window) =
            Self::window_to_rate_window(usage.seven_day_opus.as_ref(), 10080, "Opus weekly")
        {
            carveouts.push(ModelWindow {
                label: "Opus Weekly".to_string(),
                window,
            });
        }

        let plan = Self::infer_plan_from_tier(credentials.rate_limit_tier.as_deref());

        Ok(UsageSnapshot {
            primary,
            secondary,
            carveouts,
            updated_at: Utc::now(),
            identity: ProviderIdentity {
                email: None,
                organization: None,
                plan,
            },
        })
    }

    fn dashboard_url(&self) -> &'static str {
        "https://console.anthropic.com/"
    }

    fn has_valid_credentials(&self) -> bool {
        let Ok(creds) = self.load_credentials() else {
            return false;
        };
        if let Some(expires_at_ms) = creds.expires_at {
            let now_ms = chrono::Utc::now().timestamp_millis();
            if now_ms >= expires_at_ms - 60_000 {
                return false;
            }
        }
        true
    }

    fn credential_error_hint(&self) -> &'static str {
        "Run `claude` to authenticate"
    }

    fn credentials_path(&self) -> Option<PathBuf> {
        Some(self.credentials_path.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_credentials() {
        let json = r#"{
            "claudeAiOauth": {
                "accessToken": "test-token-123",
                "refreshToken": "refresh-token-456",
                "expiresAt": 1737500000000,
                "scopes": ["user:profile"],
                "rateLimitTier": "claude_pro"
            }
        }"#;

        let file: CredentialsFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.claude_ai_oauth.access_token, "test-token-123");
        assert_eq!(
            file.claude_ai_oauth.refresh_token,
            Some("refresh-token-456".to_string())
        );
        assert_eq!(
            file.claude_ai_oauth.rate_limit_tier,
            Some("claude_pro".to_string())
        );
    }

    #[test]
    fn test_parse_usage_response() {
        let json = r#"{
            "five_hour": {
                "utilization": 45.5,
                "resets_at": "2026-01-19T15:30:00Z"
            },
            "seven_day": {
                "utilization": 32.0,
                "resets_at": "2026-01-24T00:00:00Z"
            },
            "seven_day_opus": {
                "utilization": 15.0,
                "resets_at": "2026-01-24T00:00:00Z"
            }
        }"#;

        let usage: OAuthUsageResponse = serde_json::from_str(json).unwrap();

        assert!(usage.five_hour.is_some());
        let five_hour = usage.five_hour.as_ref().unwrap();
        assert!((five_hour.utilization - 45.5).abs() < f64::EPSILON);

        assert!(usage.seven_day.is_some());
        let seven_day = usage.seven_day.as_ref().unwrap();
        assert!((seven_day.utilization - 32.0).abs() < f64::EPSILON);

        assert!(usage.seven_day_opus.is_some());
    }

    #[test]
    fn test_parse_reset_time() {
        let valid_time = "2026-01-19T15:30:00Z";
        let parsed = ClaudeProvider::parse_reset_time(Some(valid_time));
        assert!(parsed.is_some());
        let dt = parsed.unwrap();
        assert_eq!(dt.year(), 2026);
        assert_eq!(dt.month(), 1);
        assert_eq!(dt.day(), 19);

        let invalid_time = "not-a-date";
        let parsed = ClaudeProvider::parse_reset_time(Some(invalid_time));
        assert!(parsed.is_none());

        let none_time = ClaudeProvider::parse_reset_time(None);
        assert!(none_time.is_none());
    }

    #[test]
    fn test_window_to_rate_window() {
        let window = UsageWindow {
            utilization: 78.5,
            resets_at: Some("2026-01-19T15:30:00Z".to_string()),
        };

        let rate_window =
            ClaudeProvider::window_to_rate_window(Some(&window), 300, "5-hour session");
        assert!(rate_window.is_some());

        let rw = rate_window.unwrap();
        assert!((rw.used_percent - 0.785).abs() < 0.001);
        assert_eq!(rw.window_minutes, Some(300));
        assert!(rw.resets_at.is_some());
        assert_eq!(rw.reset_description, Some("5-hour session".to_string()));
    }

    #[test]
    fn test_infer_plan_from_tier() {
        assert_eq!(
            ClaudeProvider::infer_plan_from_tier(Some("default_claude_max_20x")),
            Some("Claude Max".to_string())
        );
        assert_eq!(
            ClaudeProvider::infer_plan_from_tier(Some("claude_pro")),
            Some("Claude Pro".to_string())
        );
        assert_eq!(
            ClaudeProvider::infer_plan_from_tier(Some("claude_team")),
            Some("Claude Team".to_string())
        );
        assert_eq!(
            ClaudeProvider::infer_plan_from_tier(Some("claude_enterprise")),
            Some("Claude Enterprise".to_string())
        );
        assert_eq!(
            ClaudeProvider::infer_plan_from_tier(Some("something_else")),
            Some("Claude".to_string())
        );
        assert_eq!(ClaudeProvider::infer_plan_from_tier(None), None);
    }

    #[test]
    fn test_provider_metadata() {
        let provider = ClaudeProvider::new();
        assert_eq!(provider.name(), "Claude Code");
        assert_eq!(provider.identifier(), Provider::Claude);
        assert_eq!(provider.dashboard_url(), "https://console.anthropic.com/");
        assert_eq!(provider.credential_error_hint(), "Run `claude` to authenticate");
    }
}
