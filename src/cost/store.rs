use crate::core::models::{CostSnapshot, CostUsageTokenSnapshot, DailyCost, DailyTokenUsage, Provider};
use crate::cost::claude::ClaudeCostScanner;
use crate::cost::codex::CodexCostScanner;
use crate::cost::pricing::PricingStore;
use crate::cost::scanner::{aggregate_entries, aggregate_token_usage, CostScanner};
use anyhow::Result;
use chrono::{Datelike, Duration, Local, NaiveDate};
use std::collections::HashMap;

pub struct CostStore {
    claude_scanner: ClaudeCostScanner,
    codex_scanner: CodexCostScanner,
    pricing: PricingStore,
    cached_costs: HashMap<Provider, CostSnapshot>,
    cached_tokens: HashMap<Provider, CostUsageTokenSnapshot>,
    pricing_failed: bool,
    pricing_successful: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PricingRefreshResult {
    Refreshed,
    Skipped,
    Failed,
}

impl CostStore {
    pub fn new() -> Self {
        let pricing = PricingStore::load_from_cache().unwrap_or_default();
        let pricing_successful = pricing.last_fetch().is_some();

        Self {
            claude_scanner: ClaudeCostScanner::new(),
            codex_scanner: CodexCostScanner::new(),
            pricing,
            cached_costs: HashMap::new(),
            cached_tokens: HashMap::new(),
            pricing_failed: !pricing_successful,
            pricing_successful,
        }
    }

    pub async fn refresh_pricing(&mut self, force: bool) -> Result<PricingRefreshResult> {
        if !force && !self.pricing.needs_refresh() {
            tracing::debug!("Pricing cache is fresh, skipping refresh");
            return Ok(PricingRefreshResult::Skipped);
        }

        match PricingStore::fetch_from_models_dev().await {
            Ok(fresh) => {
                self.pricing.merge(fresh);
                self.pricing.save_to_cache()?;

                // Update scanners with new pricing
                self.claude_scanner = ClaudeCostScanner::new();
                self.codex_scanner = CodexCostScanner::new();

                self.pricing_successful = true;
                self.pricing_failed = false;
                tracing::info!("Refreshed pricing from models.dev");
                Ok(PricingRefreshResult::Refreshed)
            }
            Err(e) => {
                if !self.pricing_successful {
                    self.pricing_failed = true;
                }
                tracing::warn!(error = %e, "Failed to refresh pricing, using cached/default");
                Ok(PricingRefreshResult::Failed)
            }
        }
    }

    pub fn scan_all(&mut self) -> HashMap<Provider, CostScanResult> {
        let today = Local::now().date_naive();
        let month_start = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today);
        let since = month_start - Duration::days(30);

        let scanners: [(Provider, &dyn CostScanner); 2] = [
            (Provider::Claude, &self.claude_scanner),
            (Provider::Codex, &self.codex_scanner),
        ];

        let mut results = HashMap::new();
        for (provider, scanner) in scanners {
            match scanner.scan_entries(since, today) {
                Ok(entries) => {
                    let costs = aggregate_entries(&entries, &self.pricing);
                    let tokens = aggregate_token_usage(&entries, &self.pricing);
                    let cost_snapshot =
                        Self::aggregate_costs(&costs, today, month_start, self.pricing_failed);
                    let token_snapshot =
                        Self::aggregate_tokens(&tokens, today, self.pricing_failed);
                    self.cached_costs.insert(provider, cost_snapshot.clone());
                    self.cached_tokens
                        .insert(provider, token_snapshot.clone());
                    results.insert(
                        provider,
                        CostScanResult {
                            cost: cost_snapshot,
                            tokens: token_snapshot,
                        },
                    );
                }
                Err(e) => {
                    tracing::warn!(?provider, error = %e, "Failed to scan costs");
                    let cost_snapshot = self
                        .cached_costs
                        .get(&provider)
                        .cloned()
                        .unwrap_or_else(|| CostSnapshot {
                            pricing_estimate: self.pricing_failed,
                            log_error: true,
                            ..CostSnapshot::default()
                        });
                    let cost_snapshot = mark_log_error(cost_snapshot, self.pricing_failed);
                    let token_snapshot = self
                        .cached_tokens
                        .get(&provider)
                        .cloned()
                        .unwrap_or_else(|| CostUsageTokenSnapshot {
                            session_tokens: None,
                            session_cost_usd: None,
                            last_30_days_tokens: None,
                            last_30_days_cost_usd: None,
                            daily: Vec::new(),
                            updated_at: chrono::Utc::now(),
                        });
                    self.cached_costs.insert(provider, cost_snapshot.clone());
                    self.cached_tokens
                        .insert(provider, token_snapshot.clone());
                    results.insert(
                        provider,
                        CostScanResult {
                            cost: cost_snapshot,
                            tokens: token_snapshot,
                        },
                    );
                }
            };
        }

        results
    }

    #[allow(dead_code)]
    pub fn scan_provider(&mut self, provider: Provider) -> Option<CostScanResult> {
        let today = Local::now().date_naive();
        let month_start = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today);
        let since = month_start - Duration::days(30);

        let scanner: &dyn CostScanner = match provider {
            Provider::Claude => &self.claude_scanner,
            Provider::Codex => &self.codex_scanner,
        };

        match scanner.scan_entries(since, today) {
            Ok(entries) => {
                let costs = aggregate_entries(&entries, &self.pricing);
                let tokens = aggregate_token_usage(&entries, &self.pricing);
                let cost_snapshot =
                    Self::aggregate_costs(&costs, today, month_start, self.pricing_failed);
                let token_snapshot = Self::aggregate_tokens(&tokens, today, self.pricing_failed);
                self.cached_costs.insert(provider, cost_snapshot.clone());
                self.cached_tokens
                    .insert(provider, token_snapshot.clone());
                Some(CostScanResult {
                    cost: cost_snapshot,
                    tokens: token_snapshot,
                })
            }
            Err(e) => {
                tracing::warn!(?provider, error = %e, "Failed to scan costs");
                let cost_snapshot = self
                    .cached_costs
                    .get(&provider)
                    .cloned()
                    .unwrap_or_else(|| CostSnapshot {
                        pricing_estimate: self.pricing_failed,
                        log_error: true,
                        ..CostSnapshot::default()
                    });
                let cost_snapshot = mark_log_error(cost_snapshot, self.pricing_failed);
                let token_snapshot = self
                    .cached_tokens
                    .get(&provider)
                    .cloned()
                    .unwrap_or_else(|| CostUsageTokenSnapshot {
                        session_tokens: None,
                        session_cost_usd: None,
                        last_30_days_tokens: None,
                        last_30_days_cost_usd: None,
                        daily: Vec::new(),
                        updated_at: chrono::Utc::now(),
                    });
                self.cached_costs.insert(provider, cost_snapshot.clone());
                self.cached_tokens
                    .insert(provider, token_snapshot.clone());
                Some(CostScanResult {
                    cost: cost_snapshot,
                    tokens: token_snapshot,
                })
            }
        }
    }

    #[allow(dead_code)]
    pub fn get_cached(&self, provider: Provider) -> Option<&CostSnapshot> {
        self.cached_costs.get(&provider)
    }

    #[allow(dead_code)]
    pub fn get_cached_tokens(&self, provider: Provider) -> Option<&CostUsageTokenSnapshot> {
        self.cached_tokens.get(&provider)
    }

    #[allow(dead_code)]
    pub fn pricing(&self) -> &PricingStore {
        &self.pricing
    }

    fn aggregate_costs(
        costs: &[DailyCost],
        today: NaiveDate,
        month_start: NaiveDate,
        pricing_estimate: bool,
    ) -> CostSnapshot {
        let today_cost: f64 = costs
            .iter()
            .filter(|c| c.date == today)
            .map(|c| c.cost)
            .sum();

        let monthly_cost: f64 = costs
            .iter()
            .filter(|c| c.date >= month_start && c.date <= today)
            .map(|c| c.cost)
            .sum();

        let daily_breakdown: Vec<DailyCost> = costs
            .iter()
            .filter(|c| c.date >= month_start && c.date <= today)
            .cloned()
            .collect();

        CostSnapshot {
            today_cost: normalize_cost(today_cost),
            monthly_cost: normalize_cost(monthly_cost),
            currency: "USD".to_string(),
            daily_breakdown,
            pricing_estimate,
            log_error: false,
        }
    }

    fn aggregate_tokens(
        daily: &[DailyTokenUsage],
        today: NaiveDate,
        _pricing_estimate: bool,
    ) -> CostUsageTokenSnapshot {
        let cutoff = today - chrono::Duration::days(29);
        let filtered: Vec<DailyTokenUsage> = daily
            .iter()
            .filter(|d| d.date >= cutoff && d.date <= today)
            .cloned()
            .collect();

        let current_day = filtered
            .iter()
            .max_by_key(|d| d.date)
            .filter(|d| d.date == today)
            .or_else(|| filtered.iter().max_by_key(|d| d.date));

        let last_30_days_cost_usd = filtered
            .iter()
            .filter_map(|d| d.cost_usd)
            .sum::<f64>();
        let last_30_days_tokens = filtered
            .iter()
            .filter_map(|d| d.total_tokens)
            .sum::<u64>();

        CostUsageTokenSnapshot {
            session_tokens: current_day.and_then(|d| d.total_tokens),
            session_cost_usd: current_day.and_then(|d| d.cost_usd),
            last_30_days_tokens: if last_30_days_tokens > 0 {
                Some(last_30_days_tokens)
            } else {
                None
            },
            last_30_days_cost_usd: if last_30_days_cost_usd > 0.0 {
                Some(normalize_cost(last_30_days_cost_usd))
            } else {
                None
            },
            daily: filtered,
            updated_at: chrono::Utc::now(),
        }
    }
}

fn normalize_cost(value: f64) -> f64 {
    if value.abs() < 0.005 {
        0.0
    } else {
        value
    }
}

fn mark_log_error(mut snapshot: CostSnapshot, pricing_estimate: bool) -> CostSnapshot {
    snapshot.log_error = true;
    snapshot.pricing_estimate = pricing_estimate;
    snapshot
}

impl Default for CostStore {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct CostScanResult {
    pub cost: CostSnapshot,
    pub tokens: CostUsageTokenSnapshot,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aggregate_costs() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 18).unwrap();
        let month_start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();

        let costs = vec![
            DailyCost {
                date: NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                model: "claude-sonnet-4".to_string(),
                cost: 5.0,
            },
            DailyCost {
                date: NaiveDate::from_ymd_opt(2026, 1, 18).unwrap(),
                model: "claude-sonnet-4".to_string(),
                cost: 8.0,
            },
            DailyCost {
                date: NaiveDate::from_ymd_opt(2026, 1, 18).unwrap(),
                model: "claude-opus-4".to_string(),
                cost: 4.0,
            },
        ];

        let snapshot = CostStore::aggregate_costs(&costs, today, month_start, false);

        assert!((snapshot.today_cost - 12.0).abs() < 0.001);
        assert!((snapshot.monthly_cost - 17.0).abs() < 0.001);
        assert_eq!(snapshot.daily_breakdown.len(), 3);
    }

    #[test]
    fn test_aggregate_empty_costs() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 18).unwrap();
        let month_start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();

        let costs: Vec<DailyCost> = vec![];
        let snapshot = CostStore::aggregate_costs(&costs, today, month_start, false);

        assert!((snapshot.today_cost - 0.0).abs() < 0.001);
        assert!((snapshot.monthly_cost - 0.0).abs() < 0.001);
        assert!(snapshot.daily_breakdown.is_empty());
    }

    #[test]
    fn test_cost_store_new() {
        let store = CostStore::new();
        assert!(store.get_cached(Provider::Claude).is_none());
        assert!(store.get_cached(Provider::Codex).is_none());
    }
}
