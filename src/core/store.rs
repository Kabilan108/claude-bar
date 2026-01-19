use crate::core::models::{CostSnapshot, Provider, UsageSnapshot};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Default)]
struct StoreInner {
    snapshots: HashMap<Provider, UsageSnapshot>,
    costs: HashMap<Provider, CostSnapshot>,
    errors: HashMap<Provider, String>,
    last_fetch: HashMap<Provider, Instant>,
    notified_90_percent: HashSet<Provider>,
}

#[derive(Clone, Default)]
pub struct UsageStore {
    inner: Arc<RwLock<StoreInner>>,
}

impl UsageStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(StoreInner::default())),
        }
    }

    pub async fn get_snapshot(&self, provider: Provider) -> Option<UsageSnapshot> {
        self.inner.read().await.snapshots.get(&provider).cloned()
    }

    pub async fn get_cost(&self, provider: Provider) -> Option<CostSnapshot> {
        self.inner.read().await.costs.get(&provider).cloned()
    }

    pub async fn get_error(&self, provider: Provider) -> Option<String> {
        self.inner.read().await.errors.get(&provider).cloned()
    }

    pub async fn update_snapshot(&self, provider: Provider, snapshot: UsageSnapshot) {
        let mut inner = self.inner.write().await;
        inner.snapshots.insert(provider, snapshot);
        inner.errors.remove(&provider);
        inner.last_fetch.insert(provider, Instant::now());
    }

    pub async fn update_cost(&self, provider: Provider, cost: CostSnapshot) {
        self.inner.write().await.costs.insert(provider, cost);
    }

    pub async fn set_error(&self, provider: Provider, error: String) {
        let mut inner = self.inner.write().await;
        inner.errors.insert(provider, error);
        inner.snapshots.remove(&provider);
    }

    pub async fn should_refresh(&self, provider: Provider, cooldown: Duration) -> bool {
        let inner = self.inner.read().await;
        match inner.last_fetch.get(&provider) {
            Some(last) => last.elapsed() >= cooldown,
            None => true,
        }
    }

    pub async fn should_notify(&self, provider: Provider, threshold: f64) -> bool {
        let inner = self.inner.read().await;

        if inner.notified_90_percent.contains(&provider) {
            return false;
        }

        let Some(snapshot) = inner.snapshots.get(&provider) else {
            return false;
        };

        snapshot.max_usage() >= threshold
    }

    pub async fn mark_notified(&self, provider: Provider) {
        self.inner
            .write()
            .await
            .notified_90_percent
            .insert(provider);
    }

    pub async fn reset_notification(&self, provider: Provider) {
        self.inner
            .write()
            .await
            .notified_90_percent
            .remove(&provider);
    }

    pub async fn all_providers_with_snapshots(&self) -> Vec<(Provider, UsageSnapshot)> {
        self.inner
            .read()
            .await
            .snapshots
            .iter()
            .map(|(p, s)| (*p, s.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{ProviderIdentity, RateWindow};
    use chrono::Utc;

    fn make_snapshot(used_percent: f64) -> UsageSnapshot {
        UsageSnapshot {
            primary: Some(RateWindow {
                used_percent,
                window_minutes: Some(300),
                resets_at: None,
                reset_description: None,
            }),
            secondary: None,
            opus: None,
            updated_at: Utc::now(),
            identity: ProviderIdentity {
                email: None,
                organization: None,
                plan: None,
            },
        }
    }

    #[tokio::test]
    async fn test_store_update_and_get() {
        let store = UsageStore::new();
        let snapshot = make_snapshot(0.5);

        store
            .update_snapshot(Provider::Claude, snapshot.clone())
            .await;

        let retrieved = store.get_snapshot(Provider::Claude).await;
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert!((retrieved.primary.unwrap().used_percent - 0.5).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_store_error_clears_snapshot() {
        let store = UsageStore::new();
        let snapshot = make_snapshot(0.5);

        store
            .update_snapshot(Provider::Claude, snapshot.clone())
            .await;
        assert!(store.get_snapshot(Provider::Claude).await.is_some());

        store
            .set_error(Provider::Claude, "Token expired".to_string())
            .await;
        assert!(store.get_snapshot(Provider::Claude).await.is_none());
        assert!(store.get_error(Provider::Claude).await.is_some());
    }

    #[tokio::test]
    async fn test_notification_once_per_reset() {
        let store = UsageStore::new();
        let snapshot = make_snapshot(0.95);

        store
            .update_snapshot(Provider::Claude, snapshot.clone())
            .await;

        assert!(store.should_notify(Provider::Claude, 0.9).await);

        store.mark_notified(Provider::Claude).await;

        assert!(!store.should_notify(Provider::Claude, 0.9).await);

        store.reset_notification(Provider::Claude).await;

        assert!(store.should_notify(Provider::Claude, 0.9).await);
    }
}
