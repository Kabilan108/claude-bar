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
    use chrono::TimeZone;

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

    #[test]
    fn test_provider_serialization_roundtrip() {
        for provider in [Provider::Claude, Provider::Codex] {
            let json = serde_json::to_string(&provider).unwrap();
            let deserialized: Provider = serde_json::from_str(&json).unwrap();
            assert_eq!(provider, deserialized);
        }
    }

    #[test]
    fn test_rate_window_serialization_roundtrip() {
        let window = RateWindow {
            used_percent: 0.78,
            window_minutes: Some(300),
            resets_at: Some(Utc.with_ymd_and_hms(2026, 1, 18, 15, 30, 0).unwrap()),
            reset_description: Some("Resets in 2h 14m".to_string()),
        };

        let json = serde_json::to_string(&window).unwrap();
        let deserialized: RateWindow = serde_json::from_str(&json).unwrap();

        assert!((deserialized.used_percent - 0.78).abs() < f64::EPSILON);
        assert_eq!(deserialized.window_minutes, Some(300));
        assert!(deserialized.resets_at.is_some());
        assert_eq!(
            deserialized.reset_description,
            Some("Resets in 2h 14m".to_string())
        );
    }

    #[test]
    fn test_usage_snapshot_serialization_roundtrip() {
        let snapshot = UsageSnapshot {
            primary: Some(RateWindow {
                used_percent: 0.65,
                window_minutes: Some(300),
                resets_at: None,
                reset_description: None,
            }),
            secondary: Some(RateWindow {
                used_percent: 0.32,
                window_minutes: Some(10080),
                resets_at: None,
                reset_description: Some("Weekly quota".to_string()),
            }),
            opus: None,
            updated_at: Utc::now(),
            identity: ProviderIdentity {
                email: Some("user@example.com".to_string()),
                organization: Some("Acme Corp".to_string()),
                plan: Some("Pro".to_string()),
            },
        };

        let json = serde_json::to_string(&snapshot).unwrap();
        let deserialized: UsageSnapshot = serde_json::from_str(&json).unwrap();

        assert!(deserialized.primary.is_some());
        assert!(deserialized.secondary.is_some());
        assert!(deserialized.opus.is_none());
        assert_eq!(
            deserialized.identity.email,
            Some("user@example.com".to_string())
        );
    }

    #[test]
    fn test_cost_snapshot_serialization_roundtrip() {
        let cost = CostSnapshot {
            today_cost: 12.45,
            monthly_cost: 234.56,
            currency: "USD".to_string(),
            daily_breakdown: vec![
                DailyCost {
                    date: NaiveDate::from_ymd_opt(2026, 1, 18).unwrap(),
                    model: "claude-3-5-sonnet".to_string(),
                    cost: 8.50,
                },
                DailyCost {
                    date: NaiveDate::from_ymd_opt(2026, 1, 18).unwrap(),
                    model: "claude-3-opus".to_string(),
                    cost: 3.95,
                },
            ],
        };

        let json = serde_json::to_string(&cost).unwrap();
        let deserialized: CostSnapshot = serde_json::from_str(&json).unwrap();

        assert!((deserialized.today_cost - 12.45).abs() < f64::EPSILON);
        assert!((deserialized.monthly_cost - 234.56).abs() < f64::EPSILON);
        assert_eq!(deserialized.currency, "USD");
        assert_eq!(deserialized.daily_breakdown.len(), 2);
    }

    #[test]
    fn test_usage_snapshot_max_usage() {
        let snapshot = UsageSnapshot {
            primary: Some(RateWindow {
                used_percent: 0.50,
                window_minutes: None,
                resets_at: None,
                reset_description: None,
            }),
            secondary: Some(RateWindow {
                used_percent: 0.80,
                window_minutes: None,
                resets_at: None,
                reset_description: None,
            }),
            opus: Some(RateWindow {
                used_percent: 0.45,
                window_minutes: None,
                resets_at: None,
                reset_description: None,
            }),
            updated_at: Utc::now(),
            identity: ProviderIdentity {
                email: None,
                organization: None,
                plan: None,
            },
        };

        assert!((snapshot.max_usage() - 0.80).abs() < f64::EPSILON);
    }
}
