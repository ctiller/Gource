//! Reading a git repository (working tree or bare) for the server, and
//! following it for new commits.
//!
//! The history is the commits reachable from a set of *tips*: just `HEAD`,
//! or every local branch. Each poll resolves the tips again; if they moved,
//! `git log <tips> ^<every tip served so far>` yields exactly the commits not
//! served before, each once, however many branches reach them and however
//! `HEAD` moves between them.

use crate::store::History;
use gource_vcs::blobs::blob_sizes;
use gource_vcs::{Commit, CommitLog, VcsOptions};
use std::collections::HashSet;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

/// A git repository to read commits from.
#[derive(Debug, Clone)]
pub struct GitSource {
    dir: String,
    all_branches: bool,
    options: VcsOptions,
}

/// Whether `path` is a directory inside a git repository or a bare one.
pub fn is_git_dir(path: &str) -> bool {
    Path::new(path).is_dir()
        && Command::new("git")
            .args(["rev-parse", "--git-dir"])
            .current_dir(path)
            .output()
            .is_ok_and(|o| o.status.success())
}

impl GitSource {
    /// `all_branches`: the tips are all local branches instead of `HEAD`.
    pub fn new(dir: &str, all_branches: bool, options: VcsOptions) -> Self {
        Self {
            dir: dir.into(),
            all_branches,
            options,
        }
    }

    fn git(&self) -> Command {
        let mut cmd = Command::new("git");
        cmd.current_dir(&self.dir);
        cmd
    }

    /// The commits the history is made of, as sorted object names. Empty for
    /// a repository without commits yet.
    pub fn tips(&self) -> Result<Vec<String>, String> {
        let mut cmd = self.git();
        if self.all_branches {
            cmd.args(["for-each-ref", "--format=%(objectname)", "refs/heads"]);
        } else {
            let rev = if self.options.git_branch.is_empty() {
                "HEAD"
            } else {
                &self.options.git_branch
            };
            if rev.starts_with('-') {
                return Err(format!("invalid branch '{rev}'"));
            }
            cmd.args([
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{rev}^{{commit}}"),
            ]);
        }
        let out = cmd.output().map_err(|e| format!("cannot run git: {e}"))?;
        // `rev-parse --verify --quiet` fails silently for an unborn HEAD.
        if !out.status.success() && !self.all_branches && out.stderr.is_empty() {
            return Ok(Vec::new());
        }
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        let mut tips: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect();
        tips.sort();
        tips.dedup();
        Ok(tips)
    }

    /// `git log` with the arguments every read uses (oldest first).
    fn log_cmd(&self) -> Command {
        let mut cmd = self.git();
        cmd.args([
            "log",
            "--reverse",
            "--raw",
            "--encoding=UTF-8",
            "--no-renames",
            "--no-show-signature",
            // Full names, so the size lookup is never ambiguous.
            "--no-abbrev",
        ]);
        if self.options.include_numstat {
            cmd.arg("--numstat");
        }
        cmd.arg(if self.options.author_time {
            "--pretty=format:user:%aN%n%at"
        } else {
            "--pretty=format:user:%aN%n%ct"
        });
        cmd
    }

    /// The commits reachable from `tips` but not from `seen`, oldest first.
    pub fn commits(&self, tips: &[String], seen: &[String]) -> Result<Vec<Commit>, String> {
        if tips.is_empty() {
            return Ok(Vec::new());
        }
        let mut cmd = self.log_cmd();
        cmd.args(tips);
        cmd.args(seen.iter().map(|s| format!("^{s}")));
        let out = cmd.output().map_err(|e| format!("cannot run git: {e}"))?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        self.parse(&out.stdout)
    }

    /// Exactly the commits named by `ids` (no ancestors), oldest first.
    pub fn commits_of(&self, ids: &[String]) -> Result<Vec<Commit>, String> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut child = self
            .log_cmd()
            .args(["--stdin", "--no-walk=unsorted"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot run git: {e}"))?;
        let mut stdin = child.stdin.take().ok_or("no stdin")?;
        let input: String = ids.iter().map(|s| format!("{s}\n")).collect();
        let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        let _ = writer.join();
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        self.parse(&out.stdout)
    }

    /// The names of the commits reachable from `tips` but not from `seen`
    /// (merges and empty commits included), in no particular order.
    pub fn list(&self, tips: &[String], seen: &[String]) -> Result<Vec<String>, String> {
        if tips.is_empty() {
            return Ok(Vec::new());
        }
        let out = self
            .git()
            .arg("rev-list")
            .args(tips)
            .args(seen.iter().map(|s| format!("^{s}")))
            .output()
            .map_err(|e| format!("cannot run git: {e}"))?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect())
    }

    /// Parse `git log` output like a log file (filters, colours, sizes),
    /// sorted by time: branches interleave, the history is in time order.
    fn parse(&self, stdout: &[u8]) -> Result<Vec<Commit>, String> {
        if stdout.iter().all(u8::is_ascii_whitespace) {
            return Ok(Vec::new());
        }
        let mut options = self.options.clone();
        if options.include_blob_sizes {
            // One `git cat-file` for the blobs this log names.
            match blob_sizes(Path::new(&self.dir), stdout) {
                Ok(sizes) => options.blob_sizes = Some(Arc::new(sizes)),
                Err(e) => log::warn!("file sizes unavailable: {e}"),
            }
        }
        let mut file = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
        file.write_all(stdout).map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;
        let path = file.path().to_str().ok_or("non-UTF-8 temp path")?;
        let mut log =
            CommitLog::open_file(path, "git", &options).ok_or("unreadable git log output")?;
        let mut commits = Vec::new();
        while let Some(c) = log.next_commit() {
            commits.push(c);
        }
        commits.sort_by_key(|c| c.timestamp);
        Ok(commits)
    }

    /// The names in `shas` that are commits present in the repository.
    pub fn existing_commits(&self, shas: &[String]) -> Result<Vec<String>, String> {
        if shas.is_empty() {
            return Ok(Vec::new());
        }
        let mut child = self
            .git()
            .args(["cat-file", "--batch-check"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot run git: {e}"))?;
        let mut stdin = child.stdin.take().ok_or("no stdin")?;
        let input: String = shas.iter().map(|s| format!("{s}\n")).collect();
        let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
        let mut out = String::new();
        std::io::Read::read_to_string(&mut child.stdout.take().ok_or("no stdout")?, &mut out)
            .map_err(|e| e.to_string())?;
        let _ = writer.join();
        let _ = child.wait();
        // "<oid> commit <size>" for present commits, "<name> missing" otherwise.
        Ok(shas
            .iter()
            .zip(out.lines())
            .filter(|(_, l)| l.split(' ').nth(1) == Some("commit"))
            .map(|(s, _)| s.clone())
            .collect())
    }

    /// All the history now, and the tips it is complete up to.
    pub fn load(&self) -> Result<(Vec<Commit>, Vec<String>), String> {
        let tips = self.tips()?;
        let commits = self.commits(&tips, &[])?;
        Ok((commits, tips))
    }
}

/// Which commits have been served: a set of their ids, bounded by the size
/// of the repository.
///
/// Each poll lists the commits reachable from the current tips and serves
/// only those whose id is not in the set, so nothing is served twice however
/// `HEAD` or the branches move: switching branches, force-pushes, and even a
/// force-push plus `gc --prune=now` between two polls (the old tips are gone
/// but their commits are remembered by id). To keep the listing cheap it
/// excludes (`^tip`) the previous poll's tips when git can still find them;
/// that is only an optimisation, never needed for correctness, and a missing
/// object can't break it because it is checked first.
#[derive(Debug, Clone, Default)]
pub struct Served {
    /// The first 128 bits of each commit's object name.
    ids: HashSet<u128>,
    /// The tips of the previous poll.
    tips: Vec<String>,
}

fn id_key(sha: &str) -> u128 {
    u128::from_str_radix(sha.get(..32).unwrap_or(sha), 16).unwrap_or_else(|_| {
        // Not hex (cannot happen with git): hash the text instead.
        sha.bytes()
            .fold(0u128, |h, b| h.wrapping_mul(31).wrapping_add(b as u128))
    })
}

impl Served {
    /// The history is complete up to `tips`.
    pub fn new(source: &GitSource, tips: &[String]) -> Result<Self, String> {
        let ids = source.list(tips, &[])?.iter().map(|s| id_key(s)).collect();
        Ok(Self {
            ids,
            tips: tips.to_vec(),
        })
    }

    /// Number of commits remembered.
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// The commits made visible by the current tips that were not served
    /// before, oldest first; remembers them.
    pub fn poll(&mut self, source: &GitSource) -> Result<Vec<Commit>, String> {
        let tips = source.tips()?;
        if tips == self.tips {
            return Ok(Vec::new());
        }
        let known = source.existing_commits(&self.tips)?;
        let new: Vec<String> = source
            .list(&tips, &known)?
            .into_iter()
            .filter(|s| !self.ids.contains(&id_key(s)))
            .collect();
        let commits = source.commits_of(&new)?;
        self.ids.extend(new.iter().map(|s| id_key(s)));
        self.tips = tips;
        Ok(commits)
    }
}

/// Poll `source` every `interval` and append new commits to `history`.
/// `seen` are the tips `history` is already complete up to. A failed poll
/// (a repository mid-update) is logged as a warning, which `gource-serve`
/// prints to stderr, and retried at the next poll.
pub async fn follow(
    history: Arc<History>,
    source: GitSource,
    seen: Vec<String>,
    interval: Duration,
) {
    let source = Arc::new(source);
    // The ids already served are listed at once, before any tip can vanish.
    let first = {
        let (s, tips) = (source.clone(), seen.clone());
        tokio::task::spawn_blocking(move || Served::new(&s, &tips))
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r)
    };
    let mut served = match first {
        Ok(st) => Some(st),
        Err(e) => {
            log::warn!("polling failed: {e}");
            None
        }
    };
    loop {
        tokio::time::sleep(interval).await;
        let (s, st, tips) = (source.clone(), served.take(), seen.clone());
        let polled = tokio::task::spawn_blocking(move || {
            let mut st = match st {
                Some(st) => st,
                None => Served::new(&s, &tips)?,
            };
            st.poll(&s).map(|c| (c, st))
        })
        .await;
        match polled {
            Ok(Ok((commits, st))) => {
                if !commits.is_empty() {
                    log::info!("{} new commits", commits.len());
                    history.append(commits);
                }
                served = Some(st);
            }
            Ok(Err(e)) => log::warn!("polling failed: {e}"),
            Err(e) => log::warn!("polling failed: {e}"),
        }
    }
}
