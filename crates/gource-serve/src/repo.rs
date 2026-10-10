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
use std::collections::BTreeSet;
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

    /// The commits reachable from `tips` but not from `seen`, oldest first.
    pub fn commits(&self, tips: &[String], seen: &[String]) -> Result<Vec<Commit>, String> {
        if tips.is_empty() {
            return Ok(Vec::new());
        }
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
        cmd.args(tips);
        cmd.args(seen.iter().map(|s| format!("^{s}")));
        let out = cmd.output().map_err(|e| format!("cannot run git: {e}"))?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        if out.stdout.iter().all(u8::is_ascii_whitespace) {
            return Ok(Vec::new());
        }
        // Parse with the same reader (filters, colours, validation) as a
        // log file.
        let mut options = self.options.clone();
        if options.include_blob_sizes {
            // One `git cat-file` for the blobs this log names.
            match blob_sizes(Path::new(&self.dir), &out.stdout[..]) {
                Ok(sizes) => options.blob_sizes = Some(Arc::new(sizes)),
                Err(e) => log::warn!("file sizes unavailable: {e}"),
            }
        }
        let mut file = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
        file.write_all(&out.stdout).map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;
        let path = file.path().to_str().ok_or("non-UTF-8 temp path")?;
        let mut log =
            CommitLog::open_file(path, "git", &options).ok_or("unreadable git log output")?;
        let mut commits = Vec::new();
        while let Some(c) = log.next_commit() {
            commits.push(c);
        }
        // Branches interleave in time; the history is in time order.
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

    /// The members of `shas` not reachable from another member.
    pub fn independent(&self, shas: &[String]) -> Result<Vec<String>, String> {
        if shas.len() < 2 {
            return Ok(shas.to_vec());
        }
        let out = self
            .git()
            .args(["merge-base", "--independent"])
            .args(shas)
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

    /// All the history now, and the tips it is complete up to.
    pub fn load(&self) -> Result<(Vec<Commit>, Vec<String>), String> {
        let tips = self.tips()?;
        let commits = self.commits(&tips, &[])?;
        Ok((commits, tips))
    }
}

/// The tips whose history has been served, as a bounded set.
///
/// A poll excludes them (`^tip`) so that switching `HEAD` between branches, or
/// a branch moving back, never serves a commit twice. Two rules keep the set
/// safe and small for a server that runs for months:
///
/// * Only tips git can still resolve are passed on. A force-push followed by
///   `gc --prune=now` deletes old tips; `^<missing sha>` would make every
///   later `git log` fail ("bad object").
/// * After each poll the set is reduced to its independent tips
///   (`git merge-base --independent`): a tip reachable from another adds
///   nothing to the exclusion. The set is then at most the number of
///   unrelated heads, not the number of tips ever seen.
///
/// Alternative considered: a set of every served commit id, which needs the
/// whole history listed on each change and grows with the history.
#[derive(Debug, Clone, Default)]
pub struct Served(BTreeSet<String>);

impl Served {
    /// The history is complete up to these tips.
    pub fn new(tips: &[String]) -> Self {
        Self(tips.iter().cloned().collect())
    }

    /// Number of tips remembered.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The commits made visible by the current tips that were not served
    /// before, oldest first; remembers the tips.
    pub fn poll(&mut self, source: &GitSource) -> Result<Vec<Commit>, String> {
        let tips = source.tips()?;
        if tips.iter().all(|t| self.0.contains(t)) {
            return Ok(Vec::new());
        }
        let known: Vec<String> = self.0.iter().cloned().collect();
        let seen = source.existing_commits(&known)?;
        let commits = source.commits(&tips, &seen)?;
        let mut all = seen;
        all.extend(tips);
        all.sort();
        all.dedup();
        self.0 = source.independent(&all)?.into_iter().collect();
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
    let mut served = Served::new(&seen);
    loop {
        tokio::time::sleep(interval).await;
        let (s, mut st) = (source.clone(), served.clone());
        let polled = tokio::task::spawn_blocking(move || st.poll(&s).map(|c| (c, st))).await;
        match polled {
            Ok(Ok((commits, st))) => {
                if !commits.is_empty() {
                    log::info!("{} new commits", commits.len());
                    history.append(commits);
                }
                served = st;
            }
            Ok(Err(e)) => log::warn!("polling failed: {e}"),
            Err(e) => log::warn!("polling failed: {e}"),
        }
    }
}
