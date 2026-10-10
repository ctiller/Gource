//! `--file-size-metric size` uses the real blob sizes from git.

use gource_cli::{Outcome, handle_command_line, run_headless};
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
        .env("GIT_AUTHOR_DATE", "@1609459200 +0000")
        .env("GIT_COMMITTER_DATE", "@1609459200 +0000")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// `wide`: one 30000-byte line. `tall`: 300 lines of 2 bytes.
fn fixture(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("wide.txt"), "x".repeat(30_000) + "\n").unwrap();
    std::fs::write(dir.join("tall.txt"), "y\n".repeat(300)).unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "one"]);
}

fn render(repo: &Path, out: &Path, extra: &[&str]) -> Vec<u8> {
    let mut args: Vec<String> = ["-160x120", "-o", out.to_str().unwrap()]
        .iter()
        .map(|s| s.to_string())
        .collect();
    args.extend(extra.iter().map(|s| s.to_string()));
    args.push(repo.to_str().unwrap().to_string());
    let Outcome::Run(config) = handle_command_line(&args) else {
        panic!("expected a run");
    };
    match run_headless(*config, Some(240)) {
        Outcome::Exit { code, stderr, .. } => assert_eq!((code, stderr.as_str()), (0, "")),
        Outcome::Run(_) => panic!("expected exit"),
    }
    std::fs::read(out).unwrap()
}

#[test]
fn size_metric_changes_the_picture_with_real_sizes() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    fixture(&repo);

    let none = render(&repo, &dir.path().join("none.ppm"), &[]);
    let size = render(
        &repo,
        &dir.path().join("size.ppm"),
        &["--file-size-metric", "size"],
    );
    let lines = render(
        &repo,
        &dir.path().join("lines.ppm"),
        &["--file-size-metric", "lines"],
    );
    assert_ne!(none, size, "size metric must change node sizes");
    // 'wide' is tiny by lines but huge by bytes, 'tall' the opposite: with
    // real sizes the picture differs from the lines metric (the old
    // lines*35 estimate would make them equal).
    assert_ne!(lines, size, "size must not be the lines estimate");
}
