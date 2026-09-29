//! Background history indexing and cache I/O worker thread.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::theseus::{ChurnDecayModel, CohortMode};
use crate::{CommitInput, History, HistoryBuilder};

/// Configuration for the background history indexing and caching worker.
#[derive(Debug, Clone)]
pub struct HistoryWorkerConfig {
    /// Optional on-disk cache file path.
    pub cache_path: Option<PathBuf>,
    /// Unique cache key for repository and settings validation.
    pub cache_key: String,
    /// Cohort classification mode.
    pub cohort_mode: CohortMode,
    /// Line churn decay model.
    pub decay_model: ChurnDecayModel,
    /// Interval in commits at which intermediate [`History`] snapshots are published (default 128).
    pub publish_every: usize,
}

impl Default for HistoryWorkerConfig {
    fn default() -> Self {
        Self {
            cache_path: None,
            cache_key: String::new(),
            cohort_mode: CohortMode::default(),
            decay_model: ChurnDecayModel::default(),
            publish_every: 128,
        }
    }
}

/// Lifecycle status of the background history indexing worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerStatus {
    /// Currently ingesting and indexing commits.
    Indexing {
        /// Number of commits indexed so far.
        commits_indexed: usize,
    },
    /// Indexing is finished or history was loaded from disk cache.
    Complete {
        /// True if history was populated from the disk cache.
        from_cache: bool,
        /// Total number of commits in the history.
        commits_indexed: usize,
    },
    /// Worker thread failed or encountered an error.
    Failed(String),
}

struct SharedWorkerState {
    latest: RwLock<Option<Arc<History>>>,
    status: RwLock<WorkerStatus>,
    cancelled: AtomicBool,
}

/// Background worker managing asynchronous history construction and on-disk caching.
pub struct HistoryWorker {
    shared: Arc<SharedWorkerState>,
    handle: Option<JoinHandle<()>>,
}

impl HistoryWorker {
    /// Spawns a background worker thread reading commits from the returned sender channel.
    pub fn spawn(config: HistoryWorkerConfig) -> (Self, Sender<CommitInput>) {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(SharedWorkerState {
            latest: RwLock::new(None),
            status: RwLock::new(WorkerStatus::Indexing { commits_indexed: 0 }),
            cancelled: AtomicBool::new(false),
        });

        let worker_shared = Arc::clone(&shared);
        let handle = thread::spawn(move || {
            worker_thread_main(config, rx, worker_shared);
        });

        let worker = Self {
            shared,
            handle: Some(handle),
        };

        (worker, tx)
    }

    /// Returns the most recently published snapshot of [`History`], if any.
    pub fn latest(&self) -> Option<Arc<History>> {
        self.shared.latest.read().unwrap().clone()
    }

    /// Returns the current lifecycle status of the worker.
    pub fn status(&self) -> WorkerStatus {
        self.shared.status.read().unwrap().clone()
    }

    /// Requests that the worker stop indexing early.
    pub fn cancel(&self) {
        self.shared.cancelled.store(true, Ordering::SeqCst);
    }

    /// Waits for the worker thread to finish execution and returns the final [`History`].
    pub fn wait(mut self) -> Option<Arc<History>> {
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        self.latest()
    }
}

impl Drop for HistoryWorker {
    fn drop(&mut self) {
        self.cancel();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn worker_thread_main(
    config: HistoryWorkerConfig,
    rx: Receiver<CommitInput>,
    shared: Arc<SharedWorkerState>,
) {
    // 1. Try loading from cache if cache_path exists
    if let Some(ref path) = config.cache_path
        && path.exists()
        && let Ok(cached_history) = History::load_from_path(path, &config.cache_key)
    {
        let commits = cached_history.commit_count();
        let arc = Arc::new(cached_history);
        *shared.latest.write().unwrap() = Some(arc);
        *shared.status.write().unwrap() = WorkerStatus::Complete {
            from_cache: true,
            commits_indexed: commits,
        };
        return;
    }

    // 2. Build history from streaming commits
    let mut builder = HistoryBuilder::new(config.cohort_mode, config.decay_model);
    let mut commits_indexed = 0;

    loop {
        if shared.cancelled.load(Ordering::Relaxed) {
            break;
        }

        match rx.recv_timeout(Duration::from_millis(10)) {
            Ok(commit) => {
                builder.add_commit(commit);
                commits_indexed += 1;
                *shared.status.write().unwrap() = WorkerStatus::Indexing { commits_indexed };

                if config.publish_every > 0 && commits_indexed % config.publish_every == 0 {
                    *shared.latest.write().unwrap() = Some(Arc::new(builder.snapshot()));
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }

    if shared.cancelled.load(Ordering::Relaxed) {
        return;
    }

    let history = builder.finish();

    // 3. Persist to cache if configured
    if let Some(ref path) = config.cache_path {
        let _ = history.save_to_path(path, &config.cache_key);
    }

    let arc = Arc::new(history);
    *shared.latest.write().unwrap() = Some(Arc::clone(&arc));
    *shared.status.write().unwrap() = WorkerStatus::Complete {
        from_cache: false,
        commits_indexed,
    };
}
