use crate::core::models::{DailyCost, DailyTokenUsage};
use crate::cost::pricing::{PricingStore, TokenUsage};
use anyhow::Result;
use chrono::NaiveDate;
use std::collections::HashMap;

pub trait CostScanner: Send + Sync {
    fn scan_entries(&self, since: NaiveDate, until: NaiveDate) -> Result<Vec<LogEntry>>;
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

pub fn aggregate_entries(entries: &[LogEntry], pricing: &PricingStore) -> Vec<DailyCost> {
    let mut aggregated: HashMap<(NaiveDate, String), TokenUsage> = HashMap::new();

    for entry in entries {
        let key = (entry.date, entry.model.clone());
        let usage = aggregated.entry(key).or_default();
        usage.input_tokens += entry.input_tokens;
        usage.output_tokens += entry.output_tokens;
        usage.cache_creation_tokens += entry.cache_creation_tokens;
        usage.cache_read_tokens += entry.cache_read_tokens;
    }

    let mut costs: Vec<DailyCost> = aggregated
        .into_iter()
        .map(|((date, model), usage)| {
            let cost = cost_for_usage(&model, &usage, pricing);
            DailyCost { date, model, cost }
        })
        .collect();

    costs.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.model.cmp(&b.model)));
    costs
}

pub fn aggregate_token_usage(entries: &[LogEntry], pricing: &PricingStore) -> Vec<DailyTokenUsage> {
    let mut tokens_by_day: HashMap<NaiveDate, u64> = HashMap::new();
    let mut usage_by_model: HashMap<(NaiveDate, String), TokenUsage> = HashMap::new();

    for entry in entries {
        let total_tokens = entry.input_tokens
            + entry.output_tokens
            + entry.cache_creation_tokens
            + entry.cache_read_tokens;
        *tokens_by_day.entry(entry.date).or_insert(0) += total_tokens;

        let usage = usage_by_model
            .entry((entry.date, entry.model.clone()))
            .or_default();
        usage.input_tokens += entry.input_tokens;
        usage.output_tokens += entry.output_tokens;
        usage.cache_creation_tokens += entry.cache_creation_tokens;
        usage.cache_read_tokens += entry.cache_read_tokens;
    }

    let mut cost_by_day: HashMap<NaiveDate, f64> = HashMap::new();
    for ((date, model), usage) in usage_by_model {
        let cost = cost_for_usage(&model, &usage, pricing);
        *cost_by_day.entry(date).or_insert(0.0) += cost;
    }

    let mut daily: Vec<DailyTokenUsage> = tokens_by_day
        .into_iter()
        .map(|(date, tokens)| {
            let cost = cost_by_day.get(&date).copied();
            DailyTokenUsage {
                date,
                total_tokens: if tokens > 0 { Some(tokens) } else { None },
                cost_usd: cost.filter(|c| *c > 0.0),
            }
        })
        .collect();

    daily.sort_by(|a, b| a.date.cmp(&b.date));
    daily
}

fn cost_for_usage(model: &str, usage: &TokenUsage, pricing: &PricingStore) -> f64 {
    pricing
        .get_price(model)
        .map(|p| p.calculate_cost(usage))
        .unwrap_or_else(|| estimate_cost(model, usage))
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
