//! Raw Git log format parser (`git log --reverse --raw --pretty=raw`).
//! Port of `src/formats/gitraw.cpp`.

use crate::commit::Commit;
use crate::commit::CommitExt;
use crate::options::VcsOptions;
use regex::Regex;
use std::sync::LazyLock;

static GIT_RAW_COMMIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^commit ([0-9a-z]+)").unwrap());
static GIT_RAW_TREE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^tree ([0-9a-z]+)").unwrap());
static GIT_RAW_PARENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^parent ([0-9a-z]+)").unwrap());
static GIT_RAW_AUTHOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^author (.+) <([^@>]+)@?([^>]*)> (\d+) ([-+]\d+)").unwrap());
static GIT_RAW_COMMITTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^committer (.+) <([^@>]+)@?([^>]*)> (\d+) ([-+]\d+)").unwrap());
static GIT_RAW_FILE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^:[0-9]+ [0-9]+ [0-9a-z]+\.* ([0-9a-z]+)\.* ([A-Z])[ \t]+(.+)").unwrap()
});

pub fn log_command() -> String {
    "git log --reverse --raw --pretty=raw".to_string()
}

pub fn parse_commit<F>(mut get_line: F, commit: &mut Commit, options: &VcsOptions) -> bool
where
    F: FnMut(&mut String) -> bool,
{
    let mut line = String::new();
    commit.username.clear();
    commit.files.clear();

    // 1. read commit ref
    if !get_line(&mut line) {
        return false;
    }
    if !GIT_RAW_COMMIT.is_match(&line) {
        return false;
    }

    // 2. read tree
    if !get_line(&mut line) {
        return false;
    }
    if !GIT_RAW_TREE.is_match(&line) {
        return false;
    }

    // 3. 0 or more parents
    if !get_line(&mut line) {
        return false;
    }
    while GIT_RAW_PARENT.is_match(&line) {
        if !get_line(&mut line) {
            return false;
        }
    }

    // 4. author
    let caps = match GIT_RAW_AUTHOR.captures(&line) {
        Some(c) => c,
        None => return false,
    };
    commit.username = caps.get(1).map_or("", |m| m.as_str()).to_string();

    // 5. committer
    if !get_line(&mut line) {
        return false;
    }
    let committer_caps = match GIT_RAW_COMMITTER.captures(&line) {
        Some(c) => c,
        None => return false,
    };
    commit.timestamp = committer_caps
        .get(4)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);

    // 6. blank line before message
    if !get_line(&mut line) {
        return false;
    }

    // 7. read commit message
    while get_line(&mut line) && !line.is_empty() {}

    // 8. read files
    while get_line(&mut line) && !line.is_empty() {
        if let Some(file_caps) = GIT_RAW_FILE.captures(&line) {
            let action = file_caps.get(2).map_or("M", |m| m.as_str());
            let filename = file_caps.get(3).map_or("", |m| m.as_str());
            commit.add_file(filename, action, options);
        }
    }

    true
}
