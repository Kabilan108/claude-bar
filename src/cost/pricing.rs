use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub input_price_per_million: f64,
    pub output_price_per_million: f64,
}

impl ModelPricing {
    pub fn calculate_cost(&self, input_tokens: u64, output_tokens: u64) -> f64 {
        let input_cost = (input_tokens as f64 / 1_000_000.0) * self.input_price_per_million;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * self.output_price_per_million;
        input_cost + output_cost
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PricingStore {
    prices: HashMap<String, ModelPricing>,
    last_fetch: Option<DateTime<Utc>>,
}

impl PricingStore {
    pub fn new() -> Self {
        Self {
            prices: Self::embedded_defaults(),
            last_fetch: None,
        }
    }

    fn cache_path() -> Option<PathBuf> {
        dirs::cache_dir().map(|p| p.join("claude-bar").join("pricing.json"))
    }

    fn embedded_defaults() -> HashMap<String, ModelPricing> {
        let mut prices = HashMap::new();

        // Claude models
        prices.insert(
            "claude-3-5-sonnet-20241022".to_string(),
            ModelPricing {
                input_price_per_million: 3.0,
                output_price_per_million: 15.0,
            },
        );
        prices.insert(
            "claude-3-5-haiku-20241022".to_string(),
            ModelPricing {
                input_price_per_million: 0.80,
                output_price_per_million: 4.0,
            },
        );
        prices.insert(
            "claude-3-opus-20240229".to_string(),
            ModelPricing {
                input_price_per_million: 15.0,
                output_price_per_million: 75.0,
            },
        );
        prices.insert(
            "claude-sonnet-4-20250514".to_string(),
            ModelPricing {
                input_price_per_million: 3.0,
                output_price_per_million: 15.0,
            },
        );
        prices.insert(
            "claude-opus-4-20250514".to_string(),
            ModelPricing {
                input_price_per_million: 15.0,
                output_price_per_million: 75.0,
            },
        );

        // OpenAI models (for Codex)
        prices.insert(
            "gpt-4o".to_string(),
            ModelPricing {
                input_price_per_million: 2.50,
                output_price_per_million: 10.0,
            },
        );
        prices.insert(
            "gpt-4o-mini".to_string(),
            ModelPricing {
                input_price_per_million: 0.15,
                output_price_per_million: 0.60,
            },
        );
        prices.insert(
            "o1".to_string(),
            ModelPricing {
                input_price_per_million: 15.0,
                output_price_per_million: 60.0,
            },
        );

        prices
    }

    pub async fn fetch_from_models_dev() -> Result<Self> {
        // TODO: Implement fetch from models.dev API
        tracing::info!("Fetching pricing from models.dev");
        Ok(Self::new())
    }

    pub fn load_from_cache() -> Option<Self> {
        let path = Self::cache_path()?;
        let content = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn save_to_cache(&self) -> Result<()> {
        let path = Self::cache_path().context("Could not determine cache directory")?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;

        tracing::debug!(?path, "Saved pricing cache");
        Ok(())
    }

    pub fn get_price(&self, model: &str) -> Option<&ModelPricing> {
        self.prices.get(model)
    }

    pub fn last_fetch(&self) -> Option<DateTime<Utc>> {
        self.last_fetch
    }
}

impl Default for PricingStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pricing_calculation() {
        let pricing = ModelPricing {
            input_price_per_million: 3.0,
            output_price_per_million: 15.0,
        };

        let cost = pricing.calculate_cost(1_000_000, 100_000);
        assert!((cost - 4.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_embedded_defaults() {
        let store = PricingStore::new();
        assert!(store.get_price("claude-3-5-sonnet-20241022").is_some());
        assert!(store.get_price("gpt-4o").is_some());
    }
}
