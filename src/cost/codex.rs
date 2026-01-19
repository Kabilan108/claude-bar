use crate::core::models::DailyCost;
use crate::cost::pricing::{PricingStore, TokenUsage};
use crate::cost::scanner::CostScanner;
use anyhow::Result;
use chrono::NaiveDate;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub struct CodexCostScanner {
    sessions_dir: PathBuf,
    pricing: PricingStore,
}

impl CodexCostScanner {
    pub fn new(pricing: PricingStore) -> Self {
        let sessions_dir = std::env::var("CODEX_HOME")
            .map(|home| PathBuf::from(home).join("sessions"))
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .map(|p| p.join(".codex/sessions"))
                    .unwrap_or_else(|| PathBuf::from(".codex/sessions"))
            });

        Self {
            sessions_dir,
            pricing,
        }
    }

    fn find_jsonl_files(&self, since: NaiveDate, until: NaiveDate) -> Vec<PathBuf> {
        let mut files = Vec::new();

        if !self.sessions_dir.exists() {
            return files;
        }

        // Codex sessions are organized: sessions/YYYY/MM/DD/*.jsonl
        for year_entry in std::fs::read_dir(&self.sessions_dir).into_iter().flatten() {
            let year_entry = match year_entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let year_path = year_entry.path();
            if !year_path.is_dir() {
                continue;
            }

            let year: i32 = match year_path.file_name().and_then(|n| n.to_str()) {
                Some(s) => match s.parse() {
                    Ok(y) => y,
                    Err(_) => continue,
                },
                None => continue,
            };

            for month_entry in std::fs::read_dir(&year_path).into_iter().flatten() {
                let month_entry = match month_entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };

                let month_path = month_entry.path();
                if !month_path.is_dir() {
                    continue;
                }

                let month: u32 = match month_path.file_name().and_then(|n| n.to_str()) {
                    Some(s) => match s.parse() {
                        Ok(m) => m,
                        Err(_) => continue,
                    },
                    None => continue,
                };

                for day_entry in std::fs::read_dir(&month_path).into_iter().flatten() {
                    let day_entry = match day_entry {
                        Ok(e) => e,
                        Err(_) => continue,
                    };

                    let day_path = day_entry.path();
                    if !day_path.is_dir() {
                        continue;
                    }

                    let day: u32 = match day_path.file_name().and_then(|n| n.to_str()) {
                        Some(s) => match s.parse() {
                            Ok(d) => d,
                            Err(_) => continue,
                        },
                        None => continue,
                    };

                    let date = match NaiveDate::from_ymd_opt(year, month, day) {
                        Some(d) => d,
                        None => continue,
                    };

                    if date < since || date > until {
                        continue;
                    }

                    for file_entry in std::fs::read_dir(&day_path).into_iter().flatten() {
                        let file_entry = match file_entry {
                            Ok(e) => e,
                            Err(_) => continue,
                        };

                        let file_path = file_entry.path();
                        if file_path.extension().is_some_and(|ext| ext == "jsonl") {
                            files.push(file_path);
                        }
                    }
                }
            }
        }

        files
    }

    fn parse_file(&self, path: &PathBuf, date: NaiveDate) -> Result<Vec<LogEntry>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();
        let mut current_model: Option<String> = None;
        let mut last_totals = CodexTotals::default();

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

            let entry: RawCodexEntry = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(e) => {
                    tracing::debug!(?path, error = %e, "Failed to parse JSON line");
                    continue;
                }
            };

            match entry.entry_type.as_str() {
                "turn_context" => {
                    if let Some(payload) = entry.payload {
                        if let Some(model) = payload.model {
                            current_model = Some(PricingStore::normalize_model_name(&model));
                        }
                    }
                }
                "event_msg" => {
                    if let Some(payload) = entry.payload {
                        if payload.payload_type.as_deref() != Some("token_count") {
                            continue;
                        }

                        let info = match payload.info {
                            Some(i) => i,
                            None => continue,
                        };

                        let model = info
                            .model
                            .or(info.model_name)
                            .map(|m| PricingStore::normalize_model_name(&m))
                            .or_else(|| current_model.clone())
                            .unwrap_or_else(|| "unknown".to_string());

                        let totals = match info.total_token_usage {
                            Some(t) => t,
                            None => continue,
                        };

                        let input = totals.input_tokens.unwrap_or(0);
                        let cached = totals
                            .cached_input_tokens
                            .or(totals.cache_read_input_tokens)
                            .unwrap_or(0);
                        let output = totals.output_tokens.unwrap_or(0);

                        // Calculate delta from last totals
                        let delta_input = input.saturating_sub(last_totals.input);
                        let delta_cached =
                            cached.min(delta_input).saturating_sub(last_totals.cached);
                        let delta_output = output.saturating_sub(last_totals.output);

                        last_totals = CodexTotals {
                            input,
                            cached,
                            output,
                        };

                        if delta_input > 0 || delta_output > 0 {
                            entries.push(LogEntry {
                                date,
                                model,
                                input_tokens: delta_input.saturating_sub(delta_cached),
                                output_tokens: delta_output,
                                cache_read_tokens: delta_cached,
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(entries)
    }
}

impl Default for CodexCostScanner {
    fn default() -> Self {
        Self::new(PricingStore::default())
    }
}

impl CostScanner for CodexCostScanner {
    fn scan(&self, since: NaiveDate, until: NaiveDate) -> Result<Vec<DailyCost>> {
        tracing::debug!(dir = ?self.sessions_dir, "Scanning Codex sessions directory");

        let files = self.find_jsonl_files(since, until);
        tracing::debug!(count = files.len(), "Found JSONL files");

        let mut aggregated: HashMap<(NaiveDate, String), TokenUsage> = HashMap::new();

        for file in files {
            let date = Self::extract_date_from_path(&file).unwrap_or(since);

            match self.parse_file(&file, date) {
                Ok(entries) => {
                    for entry in entries {
                        let key = (entry.date, entry.model.clone());
                        let usage = aggregated.entry(key).or_default();
                        usage.input_tokens += entry.input_tokens;
                        usage.output_tokens += entry.output_tokens;
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
                        let fallback_price = 2.5 / 1_000_000.0;
                        (usage.input_tokens + usage.output_tokens) as f64 * fallback_price
                    });

                DailyCost { date, model, cost }
            })
            .collect();

        costs.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.model.cmp(&b.model)));

        Ok(costs)
    }
}

impl CodexCostScanner {
    fn extract_date_from_path(path: &Path) -> Option<NaiveDate> {
        // Path structure: .../sessions/YYYY/MM/DD/session.jsonl
        let components: Vec<_> = path.components().collect();
        if components.len() < 4 {
            return None;
        }

        let len = components.len();
        let day: u32 = components[len - 2].as_os_str().to_str()?.parse().ok()?;
        let month: u32 = components[len - 3].as_os_str().to_str()?.parse().ok()?;
        let year: i32 = components[len - 4].as_os_str().to_str()?.parse().ok()?;

        NaiveDate::from_ymd_opt(year, month, day)
    }
}

#[derive(Debug, Default)]
struct CodexTotals {
    input: u64,
    cached: u64,
    output: u64,
}

#[derive(Debug)]
struct LogEntry {
    date: NaiveDate,
    model: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct RawCodexEntry {
    #[serde(rename = "type")]
    entry_type: String,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    payload: Option<CodexPayload>,
}

#[derive(Debug, Deserialize)]
struct CodexPayload {
    #[serde(rename = "type")]
    payload_type: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    info: Option<CodexInfo>,
}

#[derive(Debug, Deserialize)]
struct CodexInfo {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    model_name: Option<String>,
    #[serde(default)]
    total_token_usage: Option<CodexTokenUsage>,
}

#[derive(Debug, Deserialize)]
struct CodexTokenUsage {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    cached_input_tokens: Option<u64>,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_turn_context() {
        let json = r#"{"type":"turn_context","timestamp":"2026-01-18T12:00:00Z","payload":{"model":"openai/gpt-5.2-codex"}}"#;

        let entry: RawCodexEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.entry_type, "turn_context");

        let payload = entry.payload.unwrap();
        assert_eq!(payload.model, Some("openai/gpt-5.2-codex".to_string()));
    }

    #[test]
    fn test_parse_event_msg_token_count() {
        let json = r#"{"type":"event_msg","timestamp":"2026-01-18T12:00:00Z","payload":{"type":"token_count","info":{"model":"openai/gpt-5.2-codex","total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":10}}}}"#;

        let entry: RawCodexEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.entry_type, "event_msg");

        let payload = entry.payload.unwrap();
        assert_eq!(payload.payload_type, Some("token_count".to_string()));

        let info = payload.info.unwrap();
        let usage = info.total_token_usage.unwrap();
        assert_eq!(usage.input_tokens, Some(100));
        assert_eq!(usage.cached_input_tokens, Some(20));
        assert_eq!(usage.output_tokens, Some(10));
    }

    #[test]
    fn test_extract_date_from_path() {
        let path = PathBuf::from("/home/user/.codex/sessions/2026/01/18/session.jsonl");
        let date = CodexCostScanner::extract_date_from_path(&path);
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2026, 1, 18).unwrap()));
    }

    #[test]
    fn test_delta_calculation() {
        // Simulate cumulative totals
        let totals1 = CodexTotals {
            input: 100,
            cached: 20,
            output: 50,
        };
        let totals2 = CodexTotals {
            input: 250,
            cached: 60,
            output: 100,
        };

        let delta_input = totals2.input.saturating_sub(totals1.input);
        let delta_cached = totals2.cached.saturating_sub(totals1.cached);
        let delta_output = totals2.output.saturating_sub(totals1.output);

        assert_eq!(delta_input, 150);
        assert_eq!(delta_cached, 40);
        assert_eq!(delta_output, 50);
    }
}
