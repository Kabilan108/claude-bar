mod claude;
mod codex;

use crate::core::models::{Provider, UsageSnapshot};
use crate::core::settings::Settings;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

pub use claude::ClaudeProvider;
pub use codex::CodexProvider;

#[async_trait]
pub trait UsageProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn identifier(&self) -> Provider;
    async fn fetch_usage(&self) -> Result<UsageSnapshot>;
    fn dashboard_url(&self) -> &'static str;
    fn has_valid_credentials(&self) -> bool;
    fn credential_error_hint(&self) -> &'static str;
}

pub struct ProviderRegistry {
    providers: Vec<Arc<dyn UsageProvider>>,
}

impl ProviderRegistry {
    pub fn new(settings: &Settings) -> Self {
        let mut providers: Vec<Arc<dyn UsageProvider>> = Vec::new();

        if settings.providers.claude.enabled {
            providers.push(Arc::new(ClaudeProvider::new()));
        }

        if settings.providers.codex.enabled {
            providers.push(Arc::new(CodexProvider::new()));
        }

        Self { providers }
    }

    pub fn enabled_providers(&self) -> impl Iterator<Item = &dyn UsageProvider> {
        self.providers.iter().map(|p| p.as_ref())
    }

    pub fn primary_provider(&self) -> Option<&dyn UsageProvider> {
        self.providers.first().map(|p| p.as_ref())
    }

    pub async fn fetch_all(&self) -> HashMap<Provider, Result<UsageSnapshot>> {
        let mut results = HashMap::new();

        for provider in &self.providers {
            let result = provider.fetch_usage().await;
            results.insert(provider.identifier(), result);
        }

        results
    }
}
