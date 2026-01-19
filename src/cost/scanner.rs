use crate::core::models::DailyCost;
use anyhow::Result;
use chrono::NaiveDate;

pub trait CostScanner: Send + Sync {
    fn scan(&self, since: NaiveDate, until: NaiveDate) -> Result<Vec<DailyCost>>;
}
