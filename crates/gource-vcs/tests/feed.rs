//! `CommitFeed` + `CommitLog::from_feed`: the client side of a remote stream.

use gource_vcs::commit::{Commit, CommitFile};
use gource_vcs::{CommitFeed, CommitLog, VcsOptions};

fn commit(ts: i64, user: &str, files: &[&str]) -> Commit {
    Commit {
        timestamp: ts,
        username: user.to_string(),
        files: files
            .iter()
            .map(|f| CommitFile {
                filename: f.to_string(),
                ..Default::default()
            })
            .collect(),
        is_shadow: false,
    }
}

#[test]
fn reads_pushed_commits_in_order_and_finishes_after_end() {
    let feed = CommitFeed::new();
    assert!(feed.is_empty());
    let mut log = CommitLog::from_feed(feed.clone(), VcsOptions::default());
    assert_eq!(log.format_name(), "remote");
    assert!(!log.is_seekable());
    assert_eq!(log.percent(), 0.0);
    assert!(log.check_format());
    assert!(log.commit_at(0.5).is_none());

    // Nothing yet, not finished: like a stream with no input.
    assert!(log.next_commit().is_none());
    assert!(!log.is_finished());

    feed.push(commit(1, "a", &["/x.rs"]));
    feed.push(commit(2, "b", &["/y.rs"]));
    assert_eq!(feed.len(), 2);
    assert_eq!(log.next_commit().unwrap().timestamp, 1);
    feed.end();
    // Ended but not drained.
    assert!(!log.is_finished());
    assert_eq!(log.next_commit().unwrap().username, "b");
    assert!(log.next_commit().is_none());
    assert!(log.is_finished());
}

#[test]
fn validation_skips_commits_without_files_or_filtered_users() {
    let feed = CommitFeed::new();
    let mut options = VcsOptions::default();
    options
        .filters
        .user_filters
        .push(fancy_regex::Regex::new("^bot$").unwrap());
    let mut log = CommitLog::from_feed(feed.clone(), options);
    feed.push(commit(1, "a", &[]));
    feed.push(commit(2, "bot", &["/z"]));
    feed.push(commit(3, "c", &["/z"]));
    assert_eq!(log.next_commit().unwrap().timestamp, 3);
    assert!(log.next_commit().is_none());

    feed.push(commit(4, "a", &[]));
    assert_eq!(log.next_commit_unvalidated().unwrap().timestamp, 4);
}

#[test]
fn buffered_commit_is_returned_first() {
    let feed = CommitFeed::new();
    let mut log = CommitLog::from_feed(feed.clone(), VcsOptions::default());
    feed.push(commit(2, "a", &["/f"]));
    feed.end();
    log.buffer_commit(commit(1, "a", &["/f"]));
    assert!(!log.is_finished());
    assert_eq!(log.next_commit().unwrap().timestamp, 1);
    assert_eq!(log.next_commit().unwrap().timestamp, 2);
    assert!(log.is_finished());
}
