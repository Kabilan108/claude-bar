use crate::core::models::DailyCost;
use crate::cost::pricing::{PricingStore, TokenUsage};
use anyhow::Result;
use chrono::NaiveDate;
use std::collections::HashMap;

pub trait CostScanner: Send + Sync {
    fn scan(&self, since: NaiveDate, until: NaiveDate) -> Result<Vec<DailyCost>>;
}

#[derive(Debug)]
pub struct LogEntry {
    pub date: NaiveDate,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
}

pub fn aggregate_entries(entries: Vec<LogEntry>, pricing: &PricingStore) -> Vec<DailyCost> {
    let mut aggregated: HashMap<(NaiveDate, String), TokenUsage> = HashMap::new();

    for entry in entries {
        let key = (entry.date, entry.model);
        let usage = aggregated.entry(key).or_default();
        usage.input_tokens += entry.input_tokens;
        usage.output_tokens += entry.output_tokens;
        usage.cache_creation_tokens += entry.cache_creation_tokens;
        usage.cache_read_tokens += entry.cache_read_tokens;
    }

    let mut costs: Vec<DailyCost> = aggregated
        .into_iter()
        .map(|((date, model), usage)| {
            let cost = pricing
                .get_price(&model)
                .map(|p| p.calculate_cost(&usage))
                .unwrap_or_else(|| estimate_cost(&model, &usage));

            DailyCost { date, model, cost }
        })
        .collect();

    costs.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.model.cmp(&b.model)));
    costs
}

fn estimate_cost(model: &str, usage: &TokenUsage) -> f64 {
    tracing::debug!(model = %model, "No pricing found, estimating");
    let fallback_price = if model.starts_with("claude") {
        3.0 / 1_000_000.0
    } else {
        2.5 / 1_000_000.0
    };
    (usage.input_tokens + usage.output_tokens) as f64 * fallback_price
}
