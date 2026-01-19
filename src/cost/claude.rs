use crate::core::models::DailyCost;
use crate::cost::scanner::CostScanner;
use anyhow::Result;
use chrono::NaiveDate;
use std::path::PathBuf;

pub struct ClaudeCostScanner {
    project_dirs: Vec<PathBuf>,
}

impl ClaudeCostScanner {
    pub fn new() -> Self {
        let mut project_dirs = Vec::new();

        if let Some(home) = dirs::home_dir() {
            project_dirs.push(home.join(".claude/projects"));
        }

        if let Some(config) = dirs::config_dir() {
            project_dirs.push(config.join("claude/projects"));
        }

        Self { project_dirs }
    }
}

impl Default for ClaudeCostScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl CostScanner for ClaudeCostScanner {
    fn scan(&self, _since: NaiveDate, _until: NaiveDate) -> Result<Vec<DailyCost>> {
        // TODO: Implement JSONL log scanning
        // - Find all .jsonl files in project directories
        // - Parse each line as JSON
        // - Extract: timestamp, model, input_tokens, output_tokens
        // - Calculate costs using pricing store
        // - Aggregate by date and model

        tracing::debug!(dirs = ?self.project_dirs, "Scanning Claude project directories");
        Ok(Vec::new())
    }
}
