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

#[tokio::test(flavor = "multi_thread")]
async fn served_commits_carry_line_counts() {
    let work = tempfile::tempdir().unwrap();
    git(work.path(), None, &["init", "-q", "-b", "main"]);
    std::fs::write(work.path().join("f.txt"), "1\n2\n3\n").unwrap();
    git(work.path(), None, &["add", "f.txt"]);
    git(work.path(), Some(1_000), &["commit", "-q", "-m", "x"]);
    std::fs::write(work.path().join("f.txt"), "1\n2\n3\n4\n5\n").unwrap();
    git(work.path(), Some(2_000), &["commit", "-qam", "y"]);

    let args = gource_serve::cli::parse_args(
        ["--port", "0", work.path().to_str().unwrap()]
            .iter()
            .map(|s| s.to_string()),
    )
    .unwrap();
    let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(gource_serve::cli::run(
        args,
        move |a| addr_tx.send(a).unwrap(),
        async move {
            stop_rx.await.ok();
        },
    ));
    let addr = addr_rx.await.unwrap();
    let bytes = tokio::task::spawn_blocking(move || {
        use std::io::{Read, Write};
        let mut s = std::net::TcpStream::connect(addr).unwrap();
        s.write_all(b"GET /stream HTTP/1.0\r\n\r\n").unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).unwrap();
        out
    })
    .await
    .unwrap();
    stop_tx.send(()).unwrap();
    server.await.unwrap().unwrap();
    // Skip the HTTP headers.
    let at = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    let hasher = StringHasher::default();
    let mut d = Decoder::new(move |p: &str| file_colour(p, &hasher));
    d.push(&bytes[at..]);
    let commits: Vec<_> = std::iter::from_fn(|| d.next_event().map(|e| e.unwrap()))
        .filter_map(|e| match e {
            Event::Commit(c) => Some(c),
            _ => None,
        })
        .collect();
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].files[0].lines_added, Some(3));
    assert_eq!(commits[1].files[0].lines_added, Some(2));
    assert_eq!(commits[1].files[0].lines_removed, Some(0));
    // Blob sizes after each commit: "1\n2\n3\n" and "1\n2\n3\n4\n5\n".
    assert_eq!(commits[0].files[0].byte_size, Some(6));
    assert_eq!(commits[1].files[0].byte_size, Some(10));
}

#[test]
fn switching_head_between_branches_never_reserves_a_commit() {
    use gource_serve::repo::Served;
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    git(d, None, &["init", "-q", "-b", "trunk"]);
    commit(d, "base", 100);
    git(d, None, &["checkout", "-q", "-b", "side"]);
    commit(d, "s1", 200);
    commit(d, "s2", 300);
    git(d, None, &["checkout", "-q", "trunk"]);
    commit(d, "t1", 400);
    // 4 commits in all. Start on side (base, s1, s2).
    git(d, None, &["checkout", "-q", "side"]);
    let source = GitSource::new(d.to_str().unwrap(), false, VcsOptions::default());
    let (commits, tips) = source.load().unwrap();
    assert_eq!(stamps(&commits), [100, 200, 300]);
    let mut served = Served::new(&tips);
    let mut total = commits.len();
    assert!(served.poll(&source).unwrap().is_empty());
    // Trunk adds exactly t1.
    git(d, None, &["checkout", "-q", "trunk"]);
    let new = served.poll(&source).unwrap();
    assert_eq!(stamps(&new), [400]);
    total += new.len();
    // Back and forth: nothing is served again.
    for branch in ["side", "trunk", "side"] {
        git(d, None, &["checkout", "-q", branch]);
        assert!(served.poll(&source).unwrap().is_empty(), "{branch}");
    }
    assert_eq!(total, 4);
    // A genuinely new commit still arrives, once.
    commit(d, "s3", 500);
    assert_eq!(stamps(&served.poll(&source).unwrap()), [500]);
    assert!(served.poll(&source).unwrap().is_empty());
}
