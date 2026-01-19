use crate::core::models::DailyCost;
use crate::cost::scanner::CostScanner;
use anyhow::Result;
use chrono::NaiveDate;
use std::path::PathBuf;

pub struct CodexCostScanner {
    sessions_dir: PathBuf,
}

impl CodexCostScanner {
    pub fn new() -> Self {
        let sessions_dir = std::env::var("CODEX_HOME")
            .map(|home| PathBuf::from(home).join("sessions"))
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .map(|p| p.join(".codex/sessions"))
                    .unwrap_or_else(|| PathBuf::from(".codex/sessions"))
            });

        Self { sessions_dir }
    }
}

impl Default for CodexCostScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl CostScanner for CodexCostScanner {
    fn scan(&self, _since: NaiveDate, _until: NaiveDate) -> Result<Vec<DailyCost>> {
        // TODO: Implement JSONL log scanning
        // - Find all .jsonl files in sessions/YYYY/MM/DD/
        // - Parse each line as JSON
        // - Extract: timestamp, model, input_tokens, output_tokens
        // - Calculate costs using pricing store
        // - Aggregate by date and model

        tracing::debug!(dir = ?self.sessions_dir, "Scanning Codex sessions directory");
        Ok(Vec::new())
    }
}
