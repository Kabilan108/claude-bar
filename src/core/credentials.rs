use crate::core::models::Provider;
use anyhow::{Context, Result};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::mpsc;

pub struct CredentialsWatcher {
    _watcher: RecommendedWatcher,
}

impl CredentialsWatcher {
    pub fn start(
        watch_paths: Vec<(Provider, PathBuf)>,
    ) -> Result<(Self, mpsc::UnboundedReceiver<Provider>)> {
        let (async_tx, async_rx) = mpsc::unbounded_channel::<Provider>();

        let mut dir_to_files: HashMap<PathBuf, Vec<(String, Provider)>> = HashMap::new();
        for (provider, path) in &watch_paths {
            if let (Some(parent), Some(filename)) =
                (path.parent().map(|p| p.to_path_buf()), path.file_name())
            {
                dir_to_files
                    .entry(parent)
                    .or_default()
                    .push((filename.to_string_lossy().to_string(), *provider));
            }
        }

        let dir_to_files_clone = dir_to_files.clone();
        let (notify_tx, mut notify_rx) = mpsc::unbounded_channel::<Provider>();

        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    if event.kind.is_modify() || event.kind.is_create() {
                        for path in &event.paths {
                            if let (Some(parent), Some(filename)) =
                                (path.parent(), path.file_name())
                            {
                                if let Some(files) =
                                    dir_to_files_clone.get(&parent.to_path_buf())
                                {
                                    let fname = filename.to_string_lossy();
                                    for (expected_name, provider) in files {
                                        if *fname == **expected_name {
                                            let _ = notify_tx.send(*provider);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            Config::default(),
        )?;

        for parent_dir in dir_to_files.keys() {
            if parent_dir.exists() {
                watcher
                    .watch(parent_dir, RecursiveMode::NonRecursive)
                    .with_context(|| {
                        format!("Failed to watch directory: {}", parent_dir.display())
                    })?;
                tracing::info!(?parent_dir, "Watching credentials directory");
            } else {
                tracing::warn!(
                    ?parent_dir,
                    "Credentials directory does not exist, skipping watch"
                );
            }
        }

        tokio::spawn(async move {
            use std::collections::HashSet;

            while let Some(first_provider) = notify_rx.recv().await {
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

                let mut changed: HashSet<Provider> = HashSet::new();
                changed.insert(first_provider);
                while let Ok(provider) = notify_rx.try_recv() {
                    changed.insert(provider);
                }

                for provider in changed {
                    tracing::info!(?provider, "Credentials file changed on disk");
                    let _ = async_tx.send(provider);
                }
            }
        });

        Ok((Self { _watcher: watcher }, async_rx))
    }
}
