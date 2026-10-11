//! Git files report their size in bytes after the commit.

use gource_vcs::{Commit, LogMill, VcsOptions};
use std::path::Path;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@x")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@x")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

fn commit_all(dir: &Path, date: i64) {
    git(dir, &["add", "-A"]);
    let out = Command::new("git")
        .current_dir(dir)
        .args(["commit", "-q", "-m", "m"])
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@x")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@x")
        .env("GIT_AUTHOR_DATE", format!("@{date} +0000"))
        .env("GIT_COMMITTER_DATE", format!("@{date} +0000"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
}

fn read(dir: &Path, options: &VcsOptions) -> Vec<Commit> {
    let mut log = LogMill::fetch_blocking(dir.to_str().unwrap(), options).unwrap();
    let mut out = Vec::new();
    while let Some(c) = log.next_commit() {
        out.push(c);
    }
    out
}

fn size_of(c: &Commit, name: &str) -> Option<u64> {
    c.files
        .iter()
        .find(|f| f.filename == name)
        .unwrap()
        .byte_size
}

#[test]
fn sizes_are_the_blob_sizes_after_each_commit() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    git(d, &["init", "-q", "-b", "main"]);
    std::fs::create_dir(d.join("sub")).unwrap();
    std::fs::write(d.join("a.txt"), "0123456789").unwrap();
    std::fs::write(d.join("sub/b.txt"), "x".repeat(1000)).unwrap();
    std::fs::write(d.join("same.txt"), "dup").unwrap();
    std::fs::write(d.join("same2.txt"), "dup").unwrap();
    commit_all(d, 1_600_000_000);
    std::fs::write(d.join("a.txt"), "0123456789".repeat(5)).unwrap();
    std::fs::remove_file(d.join("sub/b.txt")).unwrap();
    commit_all(d, 1_600_000_100);

    let options = VcsOptions {
        include_numstat: true,
        include_blob_sizes: true,
        ..Default::default()
    };
    let commits = read(d, &options);
    assert_eq!(commits.len(), 2);
    assert_eq!(size_of(&commits[0], "/a.txt"), Some(10));
    assert_eq!(size_of(&commits[0], "/sub/b.txt"), Some(1000));
    // Identical blobs are looked up once and both get the size.
    assert_eq!(size_of(&commits[0], "/same.txt"), Some(3));
    assert_eq!(size_of(&commits[0], "/same2.txt"), Some(3));
    assert_eq!(size_of(&commits[1], "/a.txt"), Some(50));
    assert_eq!(size_of(&commits[1], "/sub/b.txt"), None, "deleted");
    // Line counts still come along.
    assert_eq!(commits[1].files[0].lines_added, Some(1));

    // Off by default: no extra git process, no sizes.
    let commits = read(d, &VcsOptions::default());
    assert!(
        commits
            .iter()
            .flat_map(|c| &c.files)
            .all(|f| f.byte_size.is_none())
    );
}
