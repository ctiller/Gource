//! The served commit history.

use gource_core::StringHasher;
use gource_vcs::{Commit, LogMill, VcsOptions};
use std::sync::{Arc, PoisonError, RwLock};
use tokio::sync::watch;

/// The full, unfiltered commit list of one repository, in log order.
pub struct History {
    commits: RwLock<Vec<Commit>>,
    live: bool,
    hasher: StringHasher,
    /// Current length, so streams can wait for live commits.
    len_tx: watch::Sender<usize>,
}

impl History {
    /// `live`: commits may be [appended](Self::append) later and streams
    /// stay open after the history. `hasher` derives file colours and must
    /// match the clients' (the wire format omits derivable colours).
    pub fn new(commits: Vec<Commit>, live: bool, hasher: StringHasher) -> Arc<Self> {
        let (len_tx, _) = watch::channel(commits.len());
        Arc::new(Self {
            commits: RwLock::new(commits),
            live,
            hasher,
            len_tx,
        })
    }

    pub fn len(&self) -> usize {
        self.read(|c| c.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_live(&self) -> bool {
        self.live
    }

    pub fn hasher(&self) -> StringHasher {
        self.hasher
    }

    /// Timestamps of the first and last commit.
    pub fn span(&self) -> Option<(i64, i64)> {
        self.read(|c| Some((c.first()?.timestamp, c.last()?.timestamp)))
    }

    /// Run `f` on the commits.
    pub fn read<R>(&self, f: impl FnOnce(&[Commit]) -> R) -> R {
        f(&self.commits.read().unwrap_or_else(PoisonError::into_inner))
    }

    /// Add new commits (a live repository) and wake waiting streams.
    pub fn append(&self, new: impl IntoIterator<Item = Commit>) {
        let len = {
            let mut commits = self.commits.write().unwrap_or_else(PoisonError::into_inner);
            commits.extend(new);
            commits.len()
        };
        self.len_tx.send_replace(len);
    }

    /// Notified whenever commits are appended.
    pub fn subscribe(&self) -> watch::Receiver<usize> {
        self.len_tx.subscribe()
    }
}

/// Read every commit of a log file or repository (no filters applied).
pub fn load_history(path: &str, options: &VcsOptions) -> Result<Vec<Commit>, String> {
    let mut log = LogMill::fetch_blocking(path, options)?;
    let mut commits = Vec::new();
    while let Some(commit) = log.next_commit() {
        commits.push(commit);
    }
    Ok(commits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gource_vcs::CommitFile;

    fn commit(ts: i64) -> Commit {
        Commit {
            timestamp: ts,
            username: "u".into(),
            files: vec![CommitFile {
                filename: "/f".into(),
                ..Default::default()
            }],
            is_shadow: false,
        }
    }

    #[test]
    fn append_updates_length_span_and_notifies() {
        let h = History::new(vec![], true, StringHasher::default());
        assert!(h.is_empty());
        assert!(h.is_live());
        assert_eq!(h.span(), None);
        let mut rx = h.subscribe();
        h.append([commit(3), commit(7)]);
        assert!(rx.has_changed().unwrap());
        assert_eq!(*rx.borrow_and_update(), 2);
        assert_eq!(h.len(), 2);
        assert_eq!(h.span(), Some((3, 7)));
        assert_eq!(h.hasher(), StringHasher::default());
    }

    #[test]
    fn load_history_reads_a_custom_log() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.log");
        std::fs::write(&path, "1|alice|A|/a.rs\n2|bob|M|/a.rs\n").unwrap();
        let commits = load_history(path.to_str().unwrap(), &VcsOptions::default()).unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[1].username, "bob");
        assert!(load_history("/nonexistent/x.log", &VcsOptions::default()).is_err());
    }
}
