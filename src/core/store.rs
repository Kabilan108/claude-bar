use crate::core::models::{CostSnapshot, Provider, UsageSnapshot};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, RwLock};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum StoreUpdate {
    UsageUpdated(Provider),
    CostUpdated(Provider),
    ErrorOccurred(Provider, String),
    ErrorCleared(Provider),
}

#[derive(Default)]
struct StoreInner {
    snapshots: HashMap<Provider, UsageSnapshot>,
    costs: HashMap<Provider, CostSnapshot>,
    errors: HashMap<Provider, String>,
    last_fetch: HashMap<Provider, Instant>,
    #[allow(dead_code)]
    notified_90_percent: HashSet<Provider>,
}

#[derive(Clone)]
pub struct UsageStore {
    inner: Arc<RwLock<StoreInner>>,
    update_tx: broadcast::Sender<StoreUpdate>,
}

impl UsageStore {
    pub fn new() -> Self {
        let (update_tx, _) = broadcast::channel(64);
        Self {
            inner: Arc::new(RwLock::new(StoreInner::default())),
            update_tx,
        }
    }

    #[allow(dead_code)]
    pub fn subscribe(&self) -> broadcast::Receiver<StoreUpdate> {
        self.update_tx.subscribe()
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
        let had_error = {
            let mut inner = self.inner.write().await;
            let had_error = inner.errors.remove(&provider).is_some();
            inner.snapshots.insert(provider, snapshot);
            inner.last_fetch.insert(provider, Instant::now());
            had_error
        };

        if had_error {
            let _ = self.update_tx.send(StoreUpdate::ErrorCleared(provider));
        }
        let _ = self.update_tx.send(StoreUpdate::UsageUpdated(provider));
    }

    #[allow(dead_code)]
    pub async fn update_cost(&self, provider: Provider, cost: CostSnapshot) {
        self.inner.write().await.costs.insert(provider, cost);
        let _ = self.update_tx.send(StoreUpdate::CostUpdated(provider));
    }

    pub async fn set_error(&self, provider: Provider, error: String) {
        {
            let mut inner = self.inner.write().await;
            inner.errors.insert(provider, error.clone());
            inner.snapshots.remove(&provider);
            inner.last_fetch.insert(provider, Instant::now());
        }
        let _ = self.update_tx.send(StoreUpdate::ErrorOccurred(provider, error));
    }

    pub async fn should_refresh(&self, provider: Provider, cooldown: Duration) -> bool {
        self.inner
            .read()
            .await
            .last_fetch
            .get(&provider)
            .map_or(true, |last| last.elapsed() >= cooldown)
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub async fn mark_notified(&self, provider: Provider) {
        self.inner
            .write()
            .await
            .notified_90_percent
            .insert(provider);
    }

    #[allow(dead_code)]
    pub async fn reset_notification(&self, provider: Provider) {
        self.inner
            .write()
            .await
            .notified_90_percent
            .remove(&provider);
    }

    #[allow(dead_code)]
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

impl Default for UsageStore {
    fn default() -> Self {
        Self::new()
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

    #[tokio::test]
    async fn test_store_subscription_receives_updates() {
        let store = UsageStore::new();
        let mut receiver = store.subscribe();

        let snapshot = make_snapshot(0.5);
        store
            .update_snapshot(Provider::Claude, snapshot.clone())
            .await;

        let update = receiver.try_recv().unwrap();
        assert!(matches!(update, StoreUpdate::UsageUpdated(Provider::Claude)));
    }

    #[tokio::test]
    async fn test_store_subscription_receives_errors() {
        let store = UsageStore::new();
        let mut receiver = store.subscribe();

        store
            .set_error(Provider::Codex, "Auth failed".to_string())
            .await;

        let update = receiver.try_recv().unwrap();
        assert!(matches!(
            update,
            StoreUpdate::ErrorOccurred(Provider::Codex, _)
        ));
    }

    #[tokio::test]
    async fn test_store_subscription_error_cleared_on_success() {
        let store = UsageStore::new();

        store
            .set_error(Provider::Claude, "Network error".to_string())
            .await;

        let mut receiver = store.subscribe();

        let snapshot = make_snapshot(0.3);
        store.update_snapshot(Provider::Claude, snapshot).await;

        let update = receiver.try_recv().unwrap();
        assert!(matches!(update, StoreUpdate::ErrorCleared(Provider::Claude)));

        let update = receiver.try_recv().unwrap();
        assert!(matches!(update, StoreUpdate::UsageUpdated(Provider::Claude)));
    }
}
