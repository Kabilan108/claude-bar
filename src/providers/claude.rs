use crate::core::models::{
    ModelWindow, Provider, ProviderCostSnapshot, ProviderIdentity, RateWindow, UsageSnapshot,
};
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
    #[serde(rename = "extra_usage")]
    extra_usage: Option<OAuthExtraUsage>,
}

#[derive(Debug, Deserialize)]
struct UsageWindow {
    utilization: Option<f64>,
    resets_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OAuthExtraUsage {
    #[serde(rename = "is_enabled")]
    is_enabled: Option<bool>,
    #[serde(rename = "monthly_limit")]
    monthly_limit: Option<f64>,
    #[serde(rename = "used_credits")]
    used_credits: Option<f64>,
    currency: Option<String>,
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
        window.and_then(|w| {
            let utilization = w.utilization?;
            Some(RateWindow {
                used_percent: utilization / 100.0,
                window_minutes: Some(window_minutes),
                resets_at: Self::parse_reset_time(w.resets_at.as_deref()),
                reset_description: Some(description.to_string()),
            })
        })
    }

    fn infer_plan_from_tier(tier: Option<&str>) -> Option<String> {
        let tier = tier.unwrap_or("").to_lowercase();
        if tier.contains("max") {
            return Some("Claude Max".to_string());
        }
        if tier.contains("enterprise") {
            return Some("Claude Enterprise".to_string());
        }
        if tier.contains("team") {
            return Some("Claude Team".to_string());
        }
        if tier.contains("pro") {
            return Some("Claude Pro".to_string());
        }
        None
    }

    fn map_extra_usage(extra: &Option<OAuthExtraUsage>, plan: Option<&str>) -> Option<ProviderCostSnapshot> {
        let extra = extra.as_ref()?;
        if extra.is_enabled != Some(true) {
            return None;
        }
        let used = extra.used_credits?;
        let limit = extra.monthly_limit?;
        let currency = extra.currency.as_deref().unwrap_or("USD").trim();
        let currency_code = if currency.is_empty() { "USD" } else { currency };

        let normalized = Self::normalize_extra_usage_amounts(used, limit);
        let snapshot = ProviderCostSnapshot {
            used: normalized.0,
            limit: normalized.1,
            currency_code: currency_code.to_string(),
            period: Some("Monthly".to_string()),
            resets_at: None,
            updated_at: Utc::now(),
        };
        Self::rescale_extra_usage_if_needed(snapshot, plan)
    }

    fn normalize_extra_usage_amounts(used: f64, limit: f64) -> (f64, f64) {
        (used / 100.0, limit / 100.0)
    }

    fn rescale_extra_usage_if_needed(
        snapshot: ProviderCostSnapshot,
        plan: Option<&str>,
    ) -> Option<ProviderCostSnapshot> {
        let threshold = Self::extra_usage_rescale_threshold(plan)?;
        if snapshot.limit < threshold {
            return Some(snapshot);
        }
        Some(ProviderCostSnapshot {
            used: snapshot.used / 100.0,
            limit: snapshot.limit / 100.0,
            currency_code: snapshot.currency_code,
            period: snapshot.period,
            resets_at: snapshot.resets_at,
            updated_at: snapshot.updated_at,
        })
    }

    fn extra_usage_rescale_threshold(plan: Option<&str>) -> Option<f64> {
        let normalized = plan.unwrap_or("").trim().to_lowercase();
        if normalized.contains("enterprise") {
            return None;
        }
        Some(1000.0)
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

        let model_specific = usage
            .seven_day_sonnet
            .as_ref()
            .or(usage.seven_day_opus.as_ref());
        let tertiary =
            Self::window_to_rate_window(model_specific, 10080, "Model weekly");

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
        let provider_cost = Self::map_extra_usage(&usage.extra_usage, plan.as_deref());

        Ok(UsageSnapshot {
            primary,
            secondary,
            tertiary,
            provider_cost,
            carveouts,
            updated_at: Utc::now(),
            identity: ProviderIdentity {
                email: None,
                organization: None,
                plan: plan.clone(),
                login_method: plan,
            },
        })
    }

    fn dashboard_url(&self) -> &'static str {
        "https://console.anthropic.com/settings/billing"
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
        assert_eq!(five_hour.utilization, Some(45.5));

        assert!(usage.seven_day.is_some());
        let seven_day = usage.seven_day.as_ref().unwrap();
        assert_eq!(seven_day.utilization, Some(32.0));

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
            utilization: Some(78.5),
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
            None
        );
        assert_eq!(ClaudeProvider::infer_plan_from_tier(None), None);
    }

    #[test]
    fn test_map_extra_usage_normalization() {
        let extra = OAuthExtraUsage {
            is_enabled: Some(true),
            monthly_limit: Some(12345.0),
            used_credits: Some(2345.0),
            currency: Some("USD".to_string()),
        };

        let snapshot =
            ClaudeProvider::map_extra_usage(&Some(extra), Some("Claude Pro")).unwrap();
        assert!((snapshot.used - 23.45).abs() < 0.001);
        assert!((snapshot.limit - 123.45).abs() < 0.001);
        assert_eq!(snapshot.currency_code, "USD");
        assert_eq!(snapshot.period.as_deref(), Some("Monthly"));
    }

    #[test]
    fn test_map_extra_usage_rescale() {
        let extra = OAuthExtraUsage {
            is_enabled: Some(true),
            monthly_limit: Some(250_000.0),
            used_credits: Some(50_000.0),
            currency: Some("USD".to_string()),
        };

        let snapshot =
            ClaudeProvider::map_extra_usage(&Some(extra), Some("Claude Pro")).unwrap();
        assert!((snapshot.used - 5.0).abs() < 0.001);
        assert!((snapshot.limit - 25.0).abs() < 0.001);
    }

    #[test]
    fn test_provider_metadata() {
        let provider = ClaudeProvider::new();
        assert_eq!(provider.name(), "Claude Code");
        assert_eq!(provider.identifier(), Provider::Claude);
        assert_eq!(
            provider.dashboard_url(),
            "https://console.anthropic.com/settings/billing"
        );
        assert_eq!(provider.credential_error_hint(), "Run `claude` to authenticate");
    }
}
