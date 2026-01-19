use crate::core::models::DailyCost;
use crate::cost::pricing::{PricingStore, TokenUsage};
use crate::cost::scanner::CostScanner;
use anyhow::Result;
use chrono::{Local, NaiveDate};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub struct ClaudeCostScanner {
    project_dirs: Vec<PathBuf>,
    pricing: PricingStore,
}

impl ClaudeCostScanner {
    pub fn new(pricing: PricingStore) -> Self {
        let mut project_dirs = Vec::new();

        if let Some(home) = dirs::home_dir() {
            project_dirs.push(home.join(".claude/projects"));
        }

        if let Some(config) = dirs::config_dir() {
            project_dirs.push(config.join("claude/projects"));
        }

        Self {
            project_dirs,
            pricing,
        }
    }

    fn find_jsonl_files(&self, since: NaiveDate, until: NaiveDate) -> Vec<PathBuf> {
        let mut files = Vec::new();

        for dir in &self.project_dirs {
            if !dir.exists() {
                continue;
            }

            if let Ok(entries) = Self::walk_dir(dir) {
                for entry in entries {
                    if entry.extension().is_some_and(|ext| ext == "jsonl") {
                        if let Some(file_date) = Self::extract_date_from_path(&entry) {
                            if file_date >= since && file_date <= until {
                                files.push(entry);
                            }
                        } else {
                            files.push(entry);
                        }
                    }
                }
            }
        }

        files
    }

    fn walk_dir(dir: &PathBuf) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                files.extend(Self::walk_dir(&path)?);
            } else {
                files.push(path);
            }
        }

        Ok(files)
    }

    fn extract_date_from_path(path: &Path) -> Option<NaiveDate> {
        let file_name = path.file_stem()?.to_str()?;
        NaiveDate::parse_from_str(file_name, "%Y-%m-%d").ok()
    }

    fn parse_file(
        &self,
        path: &PathBuf,
        since: NaiveDate,
        until: NaiveDate,
    ) -> Result<Vec<LogEntry>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();
        let mut seen_ids: HashSet<String> = HashSet::new();

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    tracing::debug!(?path, error = %e, "Failed to read line");
                    continue;
                }
            };

            if line.is_empty() {
                continue;
            }

            let entry: RawLogEntry = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(e) => {
                    tracing::debug!(?path, error = %e, "Failed to parse JSON line");
                    continue;
                }
            };

            if entry.entry_type != "assistant" {
                continue;
            }

            let message = match entry.message {
                Some(m) => m,
                None => continue,
            };

            let usage = match message.usage {
                Some(u) => u,
                None => continue,
            };

            let timestamp = match &entry.timestamp {
                Some(ts) => match chrono::DateTime::parse_from_rfc3339(ts) {
                    Ok(dt) => dt.with_timezone(&Local).date_naive(),
                    Err(_) => continue,
                },
                None => continue,
            };

            if timestamp < since || timestamp > until {
                continue;
            }

            let dedup_key = format!(
                "{}:{}",
                message.id.as_deref().unwrap_or(""),
                entry.request_id.as_deref().unwrap_or("")
            );

            if !dedup_key.is_empty() && dedup_key != ":" {
                if seen_ids.contains(&dedup_key) {
                    continue;
                }
                seen_ids.insert(dedup_key);
            }

            let model = message.model.unwrap_or_else(|| "unknown".to_string());
            let model = PricingStore::normalize_model_name(&model);

            entries.push(LogEntry {
                date: timestamp,
                model,
                input_tokens: usage.input_tokens.unwrap_or(0),
                output_tokens: usage.output_tokens.unwrap_or(0),
                cache_creation_tokens: usage.cache_creation_input_tokens.unwrap_or(0),
                cache_read_tokens: usage.cache_read_input_tokens.unwrap_or(0),
            });
        }

        Ok(entries)
    }
}

impl Default for ClaudeCostScanner {
    fn default() -> Self {
        Self::new(PricingStore::default())
    }
}

impl CostScanner for ClaudeCostScanner {
    fn scan(&self, since: NaiveDate, until: NaiveDate) -> Result<Vec<DailyCost>> {
        tracing::debug!(dirs = ?self.project_dirs, "Scanning Claude project directories");

        let files = self.find_jsonl_files(since, until);
        tracing::debug!(count = files.len(), "Found JSONL files");

        let mut aggregated: HashMap<(NaiveDate, String), TokenUsage> = HashMap::new();

        for file in files {
            match self.parse_file(&file, since, until) {
                Ok(entries) => {
                    for entry in entries {
                        let key = (entry.date, entry.model.clone());
                        let usage = aggregated.entry(key).or_default();
                        usage.input_tokens += entry.input_tokens;
                        usage.output_tokens += entry.output_tokens;
                        usage.cache_creation_tokens += entry.cache_creation_tokens;
                        usage.cache_read_tokens += entry.cache_read_tokens;
                    }
                }
                Err(e) => {
                    tracing::debug!(?file, error = %e, "Failed to parse file");
                }
            }
        }

        let mut costs: Vec<DailyCost> = aggregated
            .into_iter()
            .map(|((date, model), usage)| {
                let cost = self
                    .pricing
                    .get_price(&model)
                    .map(|p| p.calculate_cost(&usage))
                    .unwrap_or_else(|| {
                        tracing::debug!(model = %model, "No pricing found, estimating");
                        let fallback_price = 3.0 / 1_000_000.0;
                        (usage.input_tokens + usage.output_tokens) as f64 * fallback_price
                    });

                DailyCost { date, model, cost }
            })
            .collect();

        costs.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.model.cmp(&b.model)));

        Ok(costs)
    }
}

#[derive(Debug)]
struct LogEntry {
    date: NaiveDate,
    model: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct RawLogEntry {
    #[serde(rename = "type")]
    entry_type: String,
    timestamp: Option<String>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    message: Option<MessageData>,
}

#[derive(Debug, Deserialize)]
struct MessageData {
    id: Option<String>,
    model: Option<String>,
    usage: Option<UsageData>,
}

#[derive(Debug, Deserialize)]
struct UsageData {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_log_entry() {
        let json = r#"{"type":"assistant","timestamp":"2026-01-18T12:00:00Z","requestId":"req_123","message":{"id":"msg_456","model":"claude-sonnet-4-20250514","usage":{"input_tokens":200,"cache_creation_input_tokens":50,"cache_read_input_tokens":25,"output_tokens":80}}}"#;

        let entry: RawLogEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.entry_type, "assistant");
        assert_eq!(entry.request_id, Some("req_123".to_string()));

        let message = entry.message.unwrap();
        assert_eq!(message.id, Some("msg_456".to_string()));
        assert_eq!(message.model, Some("claude-sonnet-4-20250514".to_string()));

        let usage = message.usage.unwrap();
        assert_eq!(usage.input_tokens, Some(200));
        assert_eq!(usage.output_tokens, Some(80));
        assert_eq!(usage.cache_creation_input_tokens, Some(50));
        assert_eq!(usage.cache_read_input_tokens, Some(25));
    }

    #[test]
    fn test_parse_minimal_entry() {
        let json = r#"{"type":"assistant","timestamp":"2026-01-18T12:00:00Z","message":{"model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}"#;

        let entry: RawLogEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.entry_type, "assistant");
        assert!(entry.request_id.is_none());

        let message = entry.message.unwrap();
        let usage = message.usage.unwrap();
        assert_eq!(usage.input_tokens, Some(100));
        assert_eq!(usage.cache_creation_input_tokens, None);
    }

    #[test]
    fn test_skip_non_assistant_entries() {
        let json = r#"{"type":"user","timestamp":"2026-01-18T12:00:00Z","message":{"content":"hello"}}"#;
        let entry: RawLogEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.entry_type, "user");
    }

    #[test]
    fn test_extract_date_from_path() {
        let path = PathBuf::from("/some/dir/2026-01-18.jsonl");
        let date = ClaudeCostScanner::extract_date_from_path(&path);
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2026, 1, 18).unwrap()));

        let path_without_date = PathBuf::from("/some/dir/session.jsonl");
        assert!(ClaudeCostScanner::extract_date_from_path(&path_without_date).is_none());
    }
}
