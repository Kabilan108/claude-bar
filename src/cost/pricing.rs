use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub input_price_per_million: f64,
    pub output_price_per_million: f64,
    #[serde(default)]
    pub cache_creation_price_per_million: Option<f64>,
    #[serde(default)]
    pub cache_read_price_per_million: Option<f64>,
    #[serde(default)]
    pub threshold_tokens: Option<u64>,
    #[serde(default)]
    pub input_price_above_threshold: Option<f64>,
    #[serde(default)]
    pub output_price_above_threshold: Option<f64>,
    #[serde(default)]
    pub cache_creation_price_above_threshold: Option<f64>,
    #[serde(default)]
    pub cache_read_price_above_threshold: Option<f64>,
}

impl ModelPricing {
    fn tiered_cost(&self, tokens: u64, base_price: f64, above_price: Option<f64>) -> f64 {
        let price_per_token = base_price / 1_000_000.0;

        match (self.threshold_tokens, above_price) {
            (Some(threshold), Some(above)) if tokens > threshold => {
                let below = threshold as f64 * price_per_token;
                let over = (tokens - threshold) as f64 * (above / 1_000_000.0);
                below + over
            }
            _ => tokens as f64 * price_per_token,
        }
    }

    pub fn calculate_cost(&self, usage: &TokenUsage) -> f64 {
        let input_cost = self.tiered_cost(
            usage.input_tokens,
            self.input_price_per_million,
            self.input_price_above_threshold,
        );

        let output_cost = self.tiered_cost(
            usage.output_tokens,
            self.output_price_per_million,
            self.output_price_above_threshold,
        );

        let cache_creation_cost = match self.cache_creation_price_per_million {
            Some(price) => self.tiered_cost(
                usage.cache_creation_tokens,
                price,
                self.cache_creation_price_above_threshold,
            ),
            None => 0.0,
        };

        let cache_read_cost = match self.cache_read_price_per_million {
            Some(price) => self.tiered_cost(
                usage.cache_read_tokens,
                price,
                self.cache_read_price_above_threshold,
            ),
            None => 0.0,
        };

        input_cost + output_cost + cache_creation_cost + cache_read_cost
    }
}

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
}

impl TokenUsage {
    pub fn new(input: u64, output: u64) -> Self {
        Self {
            input_tokens: input,
            output_tokens: output,
            ..Default::default()
        }
    }

    pub fn with_cache(mut self, creation: u64, read: u64) -> Self {
        self.cache_creation_tokens = creation;
        self.cache_read_tokens = read;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

        // Claude Opus 4.5 (latest)
        prices.insert(
            "claude-opus-4-5-20251101".to_string(),
            ModelPricing {
                input_price_per_million: 5.0,
                output_price_per_million: 25.0,
                cache_creation_price_per_million: Some(6.25),
                cache_read_price_per_million: Some(0.5),
                threshold_tokens: None,
                input_price_above_threshold: None,
                output_price_above_threshold: None,
                cache_creation_price_above_threshold: None,
                cache_read_price_above_threshold: None,
            },
        );

        // Claude Sonnet 4 (with tiered pricing above 200k tokens)
        prices.insert(
            "claude-sonnet-4-20250514".to_string(),
            ModelPricing {
                input_price_per_million: 3.0,
                output_price_per_million: 15.0,
                cache_creation_price_per_million: Some(3.75),
                cache_read_price_per_million: Some(0.3),
                threshold_tokens: Some(200_000),
                input_price_above_threshold: Some(6.0),
                output_price_above_threshold: Some(22.5),
                cache_creation_price_above_threshold: Some(7.5),
                cache_read_price_above_threshold: Some(0.6),
            },
        );

        // Claude 3.5 Sonnet
        prices.insert(
            "claude-3-5-sonnet-20241022".to_string(),
            ModelPricing {
                input_price_per_million: 3.0,
                output_price_per_million: 15.0,
                cache_creation_price_per_million: Some(3.75),
                cache_read_price_per_million: Some(0.3),
                threshold_tokens: None,
                input_price_above_threshold: None,
                output_price_above_threshold: None,
                cache_creation_price_above_threshold: None,
                cache_read_price_above_threshold: None,
            },
        );

        // Claude 3.5 Haiku
        prices.insert(
            "claude-3-5-haiku-20241022".to_string(),
            ModelPricing {
                input_price_per_million: 0.80,
                output_price_per_million: 4.0,
                cache_creation_price_per_million: Some(1.0),
                cache_read_price_per_million: Some(0.08),
                threshold_tokens: None,
                input_price_above_threshold: None,
                output_price_above_threshold: None,
                cache_creation_price_above_threshold: None,
                cache_read_price_above_threshold: None,
            },
        );

        // Claude 3 Opus
        prices.insert(
            "claude-3-opus-20240229".to_string(),
            ModelPricing {
                input_price_per_million: 15.0,
                output_price_per_million: 75.0,
                cache_creation_price_per_million: Some(18.75),
                cache_read_price_per_million: Some(1.5),
                threshold_tokens: None,
                input_price_above_threshold: None,
                output_price_above_threshold: None,
                cache_creation_price_above_threshold: None,
                cache_read_price_above_threshold: None,
            },
        );

        // Claude Opus 4
        prices.insert(
            "claude-opus-4-20250514".to_string(),
            ModelPricing {
                input_price_per_million: 15.0,
                output_price_per_million: 75.0,
                cache_creation_price_per_million: Some(18.75),
                cache_read_price_per_million: Some(1.5),
                threshold_tokens: None,
                input_price_above_threshold: None,
                output_price_above_threshold: None,
                cache_creation_price_above_threshold: None,
                cache_read_price_above_threshold: None,
            },
        );

        // GPT-5 (Codex)
        prices.insert(
            "gpt-5".to_string(),
            ModelPricing {
                input_price_per_million: 1.25,
                output_price_per_million: 10.0,
                cache_creation_price_per_million: None,
                cache_read_price_per_million: Some(0.125),
                threshold_tokens: None,
                input_price_above_threshold: None,
                output_price_above_threshold: None,
                cache_creation_price_above_threshold: None,
                cache_read_price_above_threshold: None,
            },
        );

        // GPT-4o
        prices.insert(
            "gpt-4o".to_string(),
            ModelPricing {
                input_price_per_million: 2.50,
                output_price_per_million: 10.0,
                cache_creation_price_per_million: None,
                cache_read_price_per_million: Some(1.25),
                threshold_tokens: None,
                input_price_above_threshold: None,
                output_price_above_threshold: None,
                cache_creation_price_above_threshold: None,
                cache_read_price_above_threshold: None,
            },
        );

        // GPT-4o-mini
        prices.insert(
            "gpt-4o-mini".to_string(),
            ModelPricing {
                input_price_per_million: 0.15,
                output_price_per_million: 0.60,
                cache_creation_price_per_million: None,
                cache_read_price_per_million: Some(0.075),
                threshold_tokens: None,
                input_price_above_threshold: None,
                output_price_above_threshold: None,
                cache_creation_price_above_threshold: None,
                cache_read_price_above_threshold: None,
            },
        );

        // o1
        prices.insert(
            "o1".to_string(),
            ModelPricing {
                input_price_per_million: 15.0,
                output_price_per_million: 60.0,
                cache_creation_price_per_million: None,
                cache_read_price_per_million: Some(7.5),
                threshold_tokens: None,
                input_price_above_threshold: None,
                output_price_above_threshold: None,
                cache_creation_price_above_threshold: None,
                cache_read_price_above_threshold: None,
            },
        );

        // o3
        prices.insert(
            "o3".to_string(),
            ModelPricing {
                input_price_per_million: 10.0,
                output_price_per_million: 40.0,
                cache_creation_price_per_million: None,
                cache_read_price_per_million: Some(2.5),
                threshold_tokens: None,
                input_price_above_threshold: None,
                output_price_above_threshold: None,
                cache_creation_price_above_threshold: None,
                cache_read_price_above_threshold: None,
            },
        );

        // o3-mini
        prices.insert(
            "o3-mini".to_string(),
            ModelPricing {
                input_price_per_million: 1.10,
                output_price_per_million: 4.40,
                cache_creation_price_per_million: None,
                cache_read_price_per_million: Some(0.55),
                threshold_tokens: None,
                input_price_above_threshold: None,
                output_price_above_threshold: None,
                cache_creation_price_above_threshold: None,
                cache_read_price_above_threshold: None,
            },
        );

        prices
    }

    pub fn normalize_model_name(model: &str) -> String {
        let model = model.to_lowercase();

        // Strip "anthropic." prefix
        let model = model.strip_prefix("anthropic.").unwrap_or(&model);

        // Strip "openai/" prefix
        let model = model.strip_prefix("openai/").unwrap_or(model);

        // Strip "-codex" suffix for Codex models
        let model = model.strip_suffix("-codex").unwrap_or(model);

        // Strip version suffixes like "-v1:0" (Vertex AI)
        let model = if let Some(pos) = model.find("-v1:") {
            &model[..pos]
        } else {
            model
        };

        model.to_string()
    }

    pub async fn fetch_from_models_dev() -> Result<Self> {
        tracing::info!("Fetching pricing from models.dev");

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        let response = client
            .get("https://models.dev/api/models")
            .header("Accept", "application/json")
            .send()
            .await
            .context("Failed to fetch pricing from models.dev")?;

        if !response.status().is_success() {
            anyhow::bail!(
                "models.dev returned status {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            );
        }

        let models: Vec<ModelsDevModel> = response
            .json()
            .await
            .context("Failed to parse models.dev response")?;

        let mut prices = Self::embedded_defaults();

        for model in models {
            if let Some(pricing) = model.to_pricing() {
                prices.insert(model.id, pricing);
            }
        }

        Ok(Self {
            prices,
            last_fetch: Some(Utc::now()),
        })
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
        let normalized = Self::normalize_model_name(model);

        // Try exact match first
        if let Some(price) = self.prices.get(&normalized) {
            return Some(price);
        }

        // Try stripping date suffix for Claude models (e.g., claude-sonnet-4-20250514 -> claude-sonnet-4)
        if let Some(base) = normalized.strip_suffix(|c: char| c == '-' || c.is_ascii_digit()) {
            let base = base.trim_end_matches(|c: char| c == '-' || c.is_ascii_digit());
            for (key, price) in &self.prices {
                if key.starts_with(base) {
                    return Some(price);
                }
            }
        }

        // Fallback: look for partial match
        for (key, price) in &self.prices {
            if normalized.contains(key) || key.contains(&normalized) {
                return Some(price);
            }
        }

        None
    }

    pub fn last_fetch(&self) -> Option<DateTime<Utc>> {
        self.last_fetch
    }

    pub fn needs_refresh(&self) -> bool {
        match self.last_fetch {
            None => true,
            Some(last) => Utc::now() - last > Duration::hours(24),
        }
    }

    pub fn merge(&mut self, other: Self) {
        for (key, value) in other.prices {
            self.prices.insert(key, value);
        }
        self.last_fetch = other.last_fetch.or(self.last_fetch);
    }
}

impl Default for PricingStore {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct ModelsDevModel {
    id: String,
    #[serde(default)]
    pricing: Option<ModelsDevPricing>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevPricing {
    #[serde(default)]
    input: Option<f64>,
    #[serde(default)]
    output: Option<f64>,
    #[serde(default)]
    cache_read: Option<f64>,
    #[serde(default)]
    cache_write: Option<f64>,
}

impl ModelsDevModel {
    fn to_pricing(&self) -> Option<ModelPricing> {
        let pricing = self.pricing.as_ref()?;
        let input = pricing.input?;
        let output = pricing.output?;

        Some(ModelPricing {
            input_price_per_million: input * 1_000_000.0,
            output_price_per_million: output * 1_000_000.0,
            cache_creation_price_per_million: pricing.cache_write.map(|p| p * 1_000_000.0),
            cache_read_price_per_million: pricing.cache_read.map(|p| p * 1_000_000.0),
            threshold_tokens: None,
            input_price_above_threshold: None,
            output_price_above_threshold: None,
            cache_creation_price_above_threshold: None,
            cache_read_price_above_threshold: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_cost_calculation() {
        let pricing = ModelPricing {
            input_price_per_million: 3.0,
            output_price_per_million: 15.0,
            cache_creation_price_per_million: None,
            cache_read_price_per_million: None,
            threshold_tokens: None,
            input_price_above_threshold: None,
            output_price_above_threshold: None,
            cache_creation_price_above_threshold: None,
            cache_read_price_above_threshold: None,
        };

        let usage = TokenUsage::new(1_000_000, 100_000);
        let cost = pricing.calculate_cost(&usage);
        assert!((cost - 4.5).abs() < 0.001);
    }

    #[test]
    fn test_cost_with_cache() {
        let pricing = ModelPricing {
            input_price_per_million: 3.0,
            output_price_per_million: 15.0,
            cache_creation_price_per_million: Some(3.75),
            cache_read_price_per_million: Some(0.3),
            threshold_tokens: None,
            input_price_above_threshold: None,
            output_price_above_threshold: None,
            cache_creation_price_above_threshold: None,
            cache_read_price_above_threshold: None,
        };

        let usage = TokenUsage::new(1_000_000, 100_000).with_cache(50_000, 200_000);
        let cost = pricing.calculate_cost(&usage);

        // input: 1M * 3/1M = $3.00
        // output: 100k * 15/1M = $1.50
        // cache_creation: 50k * 3.75/1M = $0.1875
        // cache_read: 200k * 0.3/1M = $0.06
        // total: $4.7475
        assert!((cost - 4.7475).abs() < 0.001);
    }

    #[test]
    fn test_tiered_pricing() {
        let pricing = ModelPricing {
            input_price_per_million: 3.0,
            output_price_per_million: 15.0,
            cache_creation_price_per_million: None,
            cache_read_price_per_million: None,
            threshold_tokens: Some(200_000),
            input_price_above_threshold: Some(6.0),
            output_price_above_threshold: Some(22.5),
            cache_creation_price_above_threshold: None,
            cache_read_price_above_threshold: None,
        };

        // 300k tokens: 200k at base rate, 100k at above rate
        let usage = TokenUsage::new(300_000, 0);
        let cost = pricing.calculate_cost(&usage);

        // 200k * 3/1M + 100k * 6/1M = 0.6 + 0.6 = $1.2
        assert!((cost - 1.2).abs() < 0.001);
    }

    #[test]
    fn test_embedded_defaults() {
        let store = PricingStore::new();
        assert!(store.get_price("claude-3-5-sonnet-20241022").is_some());
        assert!(store.get_price("gpt-4o").is_some());
        assert!(store.get_price("claude-opus-4-5-20251101").is_some());
    }

    #[test]
    fn test_normalize_model_name() {
        assert_eq!(
            PricingStore::normalize_model_name("anthropic.claude-3-5-sonnet"),
            "claude-3-5-sonnet"
        );
        assert_eq!(
            PricingStore::normalize_model_name("openai/gpt-4o-codex"),
            "gpt-4o"
        );
        assert_eq!(
            PricingStore::normalize_model_name("claude-sonnet-4-v1:0"),
            "claude-sonnet-4"
        );
    }

    #[test]
    fn test_get_price_partial_match() {
        let store = PricingStore::new();

        // Should find claude-sonnet-4-20250514 when searching for claude-sonnet-4
        let price = store.get_price("claude-sonnet-4");
        assert!(price.is_some());
    }

    #[test]
    fn test_needs_refresh() {
        let store = PricingStore::new();
        assert!(store.needs_refresh());

        let store_with_fetch = PricingStore {
            prices: HashMap::new(),
            last_fetch: Some(Utc::now()),
        };
        assert!(!store_with_fetch.needs_refresh());
    }
}
