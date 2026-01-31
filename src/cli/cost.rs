use crate::core::models::{DailyCost, Provider};
use crate::cost::{CostScanResult, CostStore};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize)]
struct CostOutput {
    providers: HashMap<String, CostSummary>,
    #[serde(with = "chrono::serde::ts_seconds")]
    scanned_at: DateTime<Utc>,
    days: u32,
}

#[derive(Serialize)]
struct CostSummary {
    today: f64,
    monthly: f64,
    currency: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    daily_breakdown: Vec<DailyBreakdown>,
}

#[derive(Serialize)]
struct DailyBreakdown {
    date: String,
    model: String,
    cost: f64,
}

pub async fn run(json: bool, days: u32) -> Result<()> {
    let mut cost_store = CostStore::new();

    cost_store.refresh_pricing(false).await?;

    let costs = cost_store.scan_all();

    if json {
        let output = build_json_output(costs, days);
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_text_output(&costs);
    }

    Ok(())
}

fn build_json_output(costs: HashMap<Provider, CostScanResult>, days: u32) -> CostOutput {
    let providers = costs
        .into_iter()
        .map(|(provider, result)| {
            let name = provider.name().to_string();
            let snapshot = result.cost;
            let summary = CostSummary {
                today: snapshot.today_cost,
                monthly: snapshot.monthly_cost,
                currency: snapshot.currency,
                daily_breakdown: snapshot
                    .daily_breakdown
                    .into_iter()
                    .map(|d| DailyBreakdown {
                        date: d.date.to_string(),
                        model: d.model,
                        cost: d.cost,
                    })
                    .collect(),
            };
            (name, summary)
        })
        .collect();

    CostOutput {
        providers,
        scanned_at: Utc::now(),
        days,
    }
}

fn print_text_output(costs: &HashMap<Provider, CostScanResult>) {
    if costs.is_empty() {
        println!("No cost data found.");
        return;
    }

    for (i, (provider, snapshot)) in costs.iter().enumerate() {
        if i > 0 {
            println!();
        }

        let cost = &snapshot.cost;
        println!("{}", provider.name());
        println!("  Today:      ${:.2}", cost.today_cost);
        println!("  This month: ${:.2}", cost.monthly_cost);

        if !cost.daily_breakdown.is_empty() {
            print_daily_summary(&cost.daily_breakdown);
        }
    }
}

fn print_daily_summary(breakdown: &[DailyCost]) {
    let mut daily_totals: HashMap<String, f64> = HashMap::new();

    for entry in breakdown {
        let date = entry.date.to_string();
        *daily_totals.entry(date).or_default() += entry.cost;
    }

    if daily_totals.len() <= 1 {
        return;
    }

    let mut dates: Vec<_> = daily_totals.into_iter().collect();
    dates.sort_by(|a, b| b.0.cmp(&a.0));

    println!();
    println!("  Recent days:");
    for (date, cost) in dates.iter().take(7) {
        println!("    {}: ${:.2}", date, cost);
    }
}
