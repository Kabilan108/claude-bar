use crate::core::models::{Provider, RateWindow};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsagePaceStage {
    OnTrack,
    SlightlyAhead,
    Ahead,
    FarAhead,
    SlightlyBehind,
    Behind,
    FarBehind,
}

#[derive(Debug, Clone)]
pub struct UsagePace {
    pub stage: UsagePaceStage,
    pub delta_percent: f64,
    pub expected_used_percent: f64,
    pub eta_seconds: Option<f64>,
    pub will_last_to_reset: bool,
}

impl UsagePace {
    pub fn weekly(window: &RateWindow, now: DateTime<Utc>, default_window_minutes: i32) -> Option<Self> {
        let resets_at = window.resets_at?;
        let minutes = window.window_minutes.unwrap_or(default_window_minutes);
        if minutes <= 0 {
            return None;
        }

        let duration = minutes as f64 * 60.0;
        let time_until_reset = (resets_at - now).num_seconds() as f64;
        if time_until_reset <= 0.0 || time_until_reset > duration {
            return None;
        }

        let elapsed = clamp(duration - time_until_reset, 0.0, duration);
        let expected = clamp((elapsed / duration) * 100.0, 0.0, 100.0);
        let actual = clamp(window.used_percent * 100.0, 0.0, 100.0);

        if elapsed == 0.0 && actual > 0.0 {
            return None;
        }

        let delta = actual - expected;
        let stage = stage_for_delta(delta);

        let mut eta_seconds = None;
        let mut will_last_to_reset = false;

        if elapsed > 0.0 && actual > 0.0 {
            let rate = actual / elapsed;
            if rate > 0.0 {
                let remaining = (100.0 - actual).max(0.0);
                let candidate = remaining / rate;
                if candidate >= time_until_reset {
                    will_last_to_reset = true;
                } else {
                    eta_seconds = Some(candidate);
                }
            }
        } else if elapsed > 0.0 && actual == 0.0 {
            will_last_to_reset = true;
        }

        Some(Self {
            stage,
            delta_percent: delta,
            expected_used_percent: expected,
            eta_seconds,
            will_last_to_reset,
        })
    }
}

pub struct WeeklyPaceDetail {
    pub left_label: String,
    pub right_label: Option<String>,
    pub expected_used_percent: f64,
    pub stage: UsagePaceStage,
}

pub struct UsagePaceText;

impl UsagePaceText {
    const MINIMUM_EXPECTED_PERCENT: f64 = 3.0;

    pub fn weekly_summary(provider: Provider, window: &RateWindow, now: DateTime<Utc>) -> Option<String> {
        let detail = Self::weekly_detail(provider, window, now)?;
        if let Some(right) = detail.right_label.as_ref() {
            return Some(format!("Pace: {} · {}", detail.left_label, right));
        }
        Some(format!("Pace: {}", detail.left_label))
    }

    pub fn weekly_detail(provider: Provider, window: &RateWindow, now: DateTime<Utc>) -> Option<WeeklyPaceDetail> {
        let pace = Self::weekly_pace(provider, window, now)?;
        Some(WeeklyPaceDetail {
            left_label: Self::detail_left_label(&pace),
            right_label: Self::detail_right_label(&pace, now),
            expected_used_percent: pace.expected_used_percent,
            stage: pace.stage,
        })
    }

    fn weekly_pace(provider: Provider, window: &RateWindow, now: DateTime<Utc>) -> Option<UsagePace> {
        if provider != Provider::Claude && provider != Provider::Codex {
            return None;
        }
        if window.remaining_percent() <= 0.0 {
            return None;
        }
        let pace = UsagePace::weekly(window, now, 10080)?;
        if pace.expected_used_percent < Self::MINIMUM_EXPECTED_PERCENT {
            return None;
        }
        Some(pace)
    }

    fn detail_left_label(pace: &UsagePace) -> String {
        let delta_value = pace.delta_percent.abs().round() as i64;
        match pace.stage {
            UsagePaceStage::OnTrack => "On pace".to_string(),
            UsagePaceStage::SlightlyAhead
            | UsagePaceStage::Ahead
            | UsagePaceStage::FarAhead => format!("{}% in deficit", delta_value),
            UsagePaceStage::SlightlyBehind
            | UsagePaceStage::Behind
            | UsagePaceStage::FarBehind => format!("{}% in reserve", delta_value),
        }
    }

    fn detail_right_label(pace: &UsagePace, now: DateTime<Utc>) -> Option<String> {
        if pace.will_last_to_reset {
            return Some("Lasts until reset".to_string());
        }
        let eta_seconds = pace.eta_seconds?;
        let eta_text = duration_text(eta_seconds, now);
        if eta_text == "now" {
            return Some("Runs out now".to_string());
        }
        Some(format!("Runs out in {}", eta_text))
    }
}

fn duration_text(seconds: f64, now: DateTime<Utc>) -> String {
    let date = now + chrono::Duration::seconds(seconds.round() as i64);
    let countdown = reset_countdown_description(date, now);
    if countdown == "now" {
        return "now".to_string();
    }
    if let Some(stripped) = countdown.strip_prefix("in ") {
        return stripped.to_string();
    }
    countdown
}

fn reset_countdown_description(reset_at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let duration = reset_at.signed_duration_since(now);
    if duration.num_seconds() <= 0 {
        return "now".to_string();
    }
    let total_minutes = duration.num_minutes();
    let days = total_minutes / (24 * 60);
    let hours = (total_minutes % (24 * 60)) / 60;
    let minutes = total_minutes % 60;

    if days > 0 {
        format!("in {}d {}h", days, hours)
    } else if hours > 0 {
        format!("in {}h {}m", hours, minutes)
    } else {
        format!("in {}m", minutes)
    }
}

fn stage_for_delta(delta: f64) -> UsagePaceStage {
    let abs_delta = delta.abs();
    if abs_delta <= 2.0 {
        return UsagePaceStage::OnTrack;
    }
    if abs_delta <= 6.0 {
        return if delta >= 0.0 {
            UsagePaceStage::SlightlyAhead
        } else {
            UsagePaceStage::SlightlyBehind
        };
    }
    if abs_delta <= 12.0 {
        return if delta >= 0.0 {
            UsagePaceStage::Ahead
        } else {
            UsagePaceStage::Behind
        };
    }
    if delta >= 0.0 {
        UsagePaceStage::FarAhead
    } else {
        UsagePaceStage::FarBehind
    }
}

fn clamp(value: f64, lower: f64, upper: f64) -> f64 {
    value.max(lower).min(upper)
}
