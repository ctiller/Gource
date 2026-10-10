//! Git-backed histories: all-branches mode, bare repositories, and live
//! following reaching an open stream.

use axum::body::Body;
use axum::http::Request;
use gource_core::StringHasher;
use gource_model::wire::{Decoder, Event};
use gource_serve::repo::{GitSource, follow, is_git_dir};
use gource_serve::{History, ServeOptions, router};
use gource_vcs::VcsOptions;
use gource_vcs::commit::file_colour;
use http_body_util::BodyExt;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use tower::ServiceExt;

fn git(dir: &Path, date: Option<i64>, args: &[&str]) {
    let mut cmd = Command::new("git");
    cmd.current_dir(dir).args(args);
    cmd.env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@x")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@x");
    if let Some(d) = date {
        cmd.env("GIT_AUTHOR_DATE", format!("@{d} +0000"))
            .env("GIT_COMMITTER_DATE", format!("@{d} +0000"));
    }
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

fn commit(dir: &Path, file: &str, date: i64) {
    std::fs::write(dir.join(file), format!("{file} {date}\n")).unwrap();
    git(dir, None, &["add", file]);
    git(dir, Some(date), &["commit", "-q", "-m", file]);
}

/// main: a(100) c(300); feat (from a): b(200). HEAD is main.
fn fixture(dir: &Path) {
    git(dir, None, &["init", "-q", "-b", "main"]);
    commit(dir, "a", 100);
    git(dir, None, &["checkout", "-q", "-b", "feat"]);
    commit(dir, "b", 200);
    git(dir, None, &["checkout", "-q", "main"]);
    commit(dir, "c", 300);
}

fn stamps(commits: &[gource_vcs::Commit]) -> Vec<i64> {
    commits.iter().map(|c| c.timestamp).collect()
}

#[test]
fn head_only_by_default_and_all_branches_on_request() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let path = dir.path().to_str().unwrap();
    assert!(is_git_dir(path));
    assert!(!is_git_dir("/nonexistent"));

    let head = GitSource::new(path, false, VcsOptions::default());
    assert_eq!(stamps(&head.load().unwrap().0), [100, 300]);

    let all = GitSource::new(path, true, VcsOptions::default());
    let (commits, tips) = all.load().unwrap();
    assert_eq!(
        stamps(&commits),
        [100, 200, 300],
        "each once, in time order"
    );
    assert_eq!(tips.len(), 2);

    // Only what is new relative to the old tips.
    git(dir.path(), None, &["checkout", "-q", "feat"]);
    commit(dir.path(), "d", 400);
    let (tips2, _) = (all.tips().unwrap(), ());
    assert_eq!(stamps(&all.commits(&tips2, &tips).unwrap()), [400]);
    assert!(all.commits(&tips2, &tips2).unwrap().is_empty());
}

#[test]
fn empty_repository_has_no_tips() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), None, &["init", "-q", "-b", "main"]);
    let path = dir.path().to_str().unwrap();
    for all in [false, true] {
        let (commits, tips) = GitSource::new(path, all, VcsOptions::default())
            .load()
            .unwrap();
        assert!(commits.is_empty() && tips.is_empty());
    }
}

async fn read_until(
    body: &mut Body,
    d: &mut Decoder<impl Fn(&str) -> [f32; 3]>,
    want: impl Fn(&Event) -> bool,
) -> Vec<Event> {
    let mut seen = Vec::new();
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(20), body.frame())
            .await
            .expect("stream stalled")
            .expect("stream ended")
            .unwrap();
        if let Ok(data) = frame.into_data() {
            d.push(&data);
            while let Some(e) = d.next_event() {
                let e = e.unwrap();
                let done = want(&e);
                seen.push(e);
                if done {
                    return seen;
                }
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_commit_pushed_to_a_bare_repository_reaches_an_open_stream() {
    let work = tempfile::tempdir().unwrap();
    fixture(work.path());
    let bare = tempfile::tempdir().unwrap();
    let bare_path = bare.path().join("r.git");
    git(
        work.path(),
        None,
        &["clone", "-q", "--bare", ".", bare_path.to_str().unwrap()],
    );
    git(
        work.path(),
        None,
        &["remote", "add", "bare", bare_path.to_str().unwrap()],
    );
    let path = bare_path.to_str().unwrap();
    assert!(is_git_dir(path));

    let source = GitSource::new(path, true, VcsOptions::default());
    let (commits, tips) = source.load().unwrap();
    assert_eq!(commits.len(), 3);
    let history = History::new(commits, true, StringHasher::default());
    let follower = tokio::spawn(follow(
        history.clone(),
        source,
        tips,
        Duration::from_millis(50),
    ));
    let app = router(history.clone(), &ServeOptions::default());
    let resp = app
        .oneshot(Request::get("/stream").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let mut body = resp.into_body();
    let hasher = StringHasher::default();
    let mut d = Decoder::new(move |p: &str| file_colour(p, &hasher));
    let events = read_until(&mut body, &mut d, |e| *e == Event::EndOfHistory).await;
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::Commit(_)))
            .count(),
        3
    );

    // New work on a branch other than HEAD, pushed to the bare repository.
    git(work.path(), None, &["checkout", "-q", "feat"]);
    commit(work.path(), "e", 500);
    git(work.path(), None, &["push", "-q", "bare", "feat"]);
    let events = read_until(&mut body, &mut d, |e| matches!(e, Event::Commit(_))).await;
    assert!(matches!(events.last(), Some(Event::Commit(c)) if c.timestamp == 500));
    assert_eq!(history.len(), 4);
    follower.abort();
}
