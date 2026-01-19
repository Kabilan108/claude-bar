use crate::core::models::{CostSnapshot, DailyCost, Provider};
use crate::cost::claude::ClaudeCostScanner;
use crate::cost::codex::CodexCostScanner;
use crate::cost::pricing::PricingStore;
use crate::cost::scanner::CostScanner;
use anyhow::Result;
use chrono::{Datelike, Duration, Local, NaiveDate};
use std::collections::HashMap;

pub struct CostStore {
    claude_scanner: ClaudeCostScanner,
    codex_scanner: CodexCostScanner,
    pricing: PricingStore,
    cached_costs: HashMap<Provider, CostSnapshot>,
}

impl CostStore {
    pub fn new() -> Self {
        let pricing = PricingStore::load_from_cache().unwrap_or_default();

        Self {
            claude_scanner: ClaudeCostScanner::new(pricing.clone()),
            codex_scanner: CodexCostScanner::new(pricing.clone()),
            pricing,
            cached_costs: HashMap::new(),
        }
    }

    pub async fn refresh_pricing(&mut self) -> Result<()> {
        if !self.pricing.needs_refresh() {
            tracing::debug!("Pricing cache is fresh, skipping refresh");
            return Ok(());
        }

        match PricingStore::fetch_from_models_dev().await {
            Ok(fresh) => {
                self.pricing.merge(fresh);
                self.pricing.save_to_cache()?;

                // Update scanners with new pricing
                self.claude_scanner = ClaudeCostScanner::new(self.pricing.clone());
                self.codex_scanner = CodexCostScanner::new(self.pricing.clone());

                tracing::info!("Refreshed pricing from models.dev");
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to refresh pricing, using cached/default");
            }
        }

        Ok(())
    }

    pub fn scan_all(&mut self) -> HashMap<Provider, CostSnapshot> {
        let today = Local::now().date_naive();
        let month_start = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today);
        let since = month_start - Duration::days(30);

        let mut results = HashMap::new();

        match self.claude_scanner.scan(since, today) {
            Ok(costs) => {
                let snapshot = Self::aggregate_costs(&costs, today, month_start);
                self.cached_costs.insert(Provider::Claude, snapshot.clone());
                results.insert(Provider::Claude, snapshot);
            }
            Err(e) => {
                tracing::warn!(provider = "Claude", error = %e, "Failed to scan costs");
                if let Some(cached) = self.cached_costs.get(&Provider::Claude) {
                    results.insert(Provider::Claude, cached.clone());
                }
            }
        }

        match self.codex_scanner.scan(since, today) {
            Ok(costs) => {
                let snapshot = Self::aggregate_costs(&costs, today, month_start);
                self.cached_costs.insert(Provider::Codex, snapshot.clone());
                results.insert(Provider::Codex, snapshot);
            }
            Err(e) => {
                tracing::warn!(provider = "Codex", error = %e, "Failed to scan costs");
                if let Some(cached) = self.cached_costs.get(&Provider::Codex) {
                    results.insert(Provider::Codex, cached.clone());
                }
            }
        }

        results
    }

    pub fn scan_provider(&mut self, provider: Provider) -> Option<CostSnapshot> {
        let today = Local::now().date_naive();
        let month_start = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today);
        let since = month_start - Duration::days(30);

        let scanner: &dyn CostScanner = match provider {
            Provider::Claude => &self.claude_scanner,
            Provider::Codex => &self.codex_scanner,
        };

        match scanner.scan(since, today) {
            Ok(costs) => {
                let snapshot = Self::aggregate_costs(&costs, today, month_start);
                self.cached_costs.insert(provider, snapshot.clone());
                Some(snapshot)
            }
            Err(e) => {
                tracing::warn!(?provider, error = %e, "Failed to scan costs");
                self.cached_costs.get(&provider).cloned()
            }
        }
    }

    pub fn get_cached(&self, provider: Provider) -> Option<&CostSnapshot> {
        self.cached_costs.get(&provider)
    }

    pub fn pricing(&self) -> &PricingStore {
        &self.pricing
    }

    fn aggregate_costs(
        costs: &[DailyCost],
        today: NaiveDate,
        month_start: NaiveDate,
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
            today_cost,
            monthly_cost,
            currency: "USD".to_string(),
            daily_breakdown,
        }
    }
}

impl Default for CostStore {
    fn default() -> Self {
        Self::new()
    }
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

        let snapshot = CostStore::aggregate_costs(&costs, today, month_start);

        assert!((snapshot.today_cost - 12.0).abs() < 0.001);
        assert!((snapshot.monthly_cost - 17.0).abs() < 0.001);
        assert_eq!(snapshot.daily_breakdown.len(), 3);
    }

    #[test]
    fn test_aggregate_empty_costs() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 18).unwrap();
        let month_start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();

        let costs: Vec<DailyCost> = vec![];
        let snapshot = CostStore::aggregate_costs(&costs, today, month_start);

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
