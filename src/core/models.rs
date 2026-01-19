use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider {
    Claude,
    Codex,
}

impl Provider {
    pub fn name(&self) -> &'static str {
        match self {
            Provider::Claude => "Claude Code",
            Provider::Codex => "Codex",
        }
    }

    pub fn dashboard_url(&self) -> &'static str {
        match self {
            Provider::Claude => "https://console.anthropic.com/",
            Provider::Codex => "https://chatgpt.com/",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateWindow {
    pub used_percent: f64,
    pub window_minutes: Option<i32>,
    pub resets_at: Option<DateTime<Utc>>,
    pub reset_description: Option<String>,
}

impl RateWindow {
    pub fn remaining_percent(&self) -> f64 {
        1.0 - self.used_percent
    }

    pub fn is_high_usage(&self, threshold: f64) -> bool {
        self.used_percent >= threshold
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderIdentity {
    pub email: Option<String>,
    pub organization: Option<String>,
    pub plan: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub primary: Option<RateWindow>,
    pub secondary: Option<RateWindow>,
    pub opus: Option<RateWindow>,
    pub updated_at: DateTime<Utc>,
    pub identity: ProviderIdentity,
}

impl UsageSnapshot {
    pub fn max_usage(&self) -> f64 {
        [
            self.primary.as_ref().map(|r| r.used_percent),
            self.secondary.as_ref().map(|r| r.used_percent),
            self.opus.as_ref().map(|r| r.used_percent),
        ]
        .into_iter()
        .flatten()
        .fold(0.0, f64::max)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyCost {
    pub date: NaiveDate,
    pub model: String,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSnapshot {
    pub today_cost: f64,
    pub monthly_cost: f64,
    pub currency: String,
    pub daily_breakdown: Vec<DailyCost>,
}

impl Default for CostSnapshot {
    fn default() -> Self {
        Self {
            today_cost: 0.0,
            monthly_cost: 0.0,
            currency: "USD".to_string(),
            daily_breakdown: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_window_remaining() {
        let window = RateWindow {
            used_percent: 0.75,
            window_minutes: Some(300),
            resets_at: None,
            reset_description: None,
        };
        assert!((window.remaining_percent() - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn test_rate_window_high_usage() {
        let window = RateWindow {
            used_percent: 0.92,
            window_minutes: None,
            resets_at: None,
            reset_description: None,
        };
        assert!(window.is_high_usage(0.9));
        assert!(!window.is_high_usage(0.95));
    }

    #[test]
    fn test_provider_names() {
        assert_eq!(Provider::Claude.name(), "Claude Code");
        assert_eq!(Provider::Codex.name(), "Codex");
    }
}
