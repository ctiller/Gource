//! Background log acquisition (port of `logmill.cpp`).
//!
//! Given a path (repository directory, log file, or `-`), a background thread
//! figures out the format (forced via `VcsOptions::log_format` or
//! auto-detected in the C++ order: git, hg, bzr, gitraw, cvs-exp, svn, cvs2cl,
//! custom, apache), generating the log with the VCS command when the path is
//! a repository (walking up parent directories looking for `.git`, `.hg`,
//! `.bzr`, `.svn`), and produces a [`CommitLog`]. Error messages must match
//! logmill.cpp.

use crate::formats;
use crate::log::{CommitLog, SeekableLog};
use crate::options::VcsOptions;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use tempfile::NamedTempFile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogMillStatus {
    Fetching,
    Success,
    Failure,
}

/// Handle to the background log fetch.
pub struct LogMill {
    status: Arc<Mutex<LogMillStatus>>,
    result_rx: Receiver<Result<CommitLog, String>>,
    cached_result: Option<Result<CommitLog, String>>,
    abort_flag: Arc<AtomicBool>,
    child_process: Arc<Mutex<Option<Child>>>,
    thread_handle: Option<JoinHandle<()>>,
}

impl LogMill {
    /// Start fetching in a background thread.
    pub fn spawn(path: &str, options: VcsOptions) -> LogMill {
        let status = Arc::new(Mutex::new(LogMillStatus::Fetching));
        let abort_flag = Arc::new(AtomicBool::new(false));
        let child_process = Arc::new(Mutex::new(None));
        let (tx, rx) = channel();

        let thread_status = Arc::clone(&status);
        let thread_abort = Arc::clone(&abort_flag);
        let thread_child = Arc::clone(&child_process);
        let path_str = path.to_string();

        let handle = thread::Builder::new()
            .name("logmill".to_string())
            .spawn(move || {
                let res = fetch_internal(
                    &path_str,
                    &options,
                    Some(&thread_abort),
                    Some(&thread_child),
                );
                let new_status = if res.is_ok() {
                    LogMillStatus::Success
                } else {
                    LogMillStatus::Failure
                };
                *thread_status.lock().unwrap() = new_status;
                let _ = tx.send(res);
            })
            .expect("failed to spawn logmill thread");

        LogMill {
            status,
            result_rx: rx,
            cached_result: None,
            abort_flag,
            child_process,
            thread_handle: Some(handle),
        }
    }

    /// Fetch synchronously on the calling thread (used by
    /// `--output-custom-log` and tests).
    pub fn fetch_blocking(path: &str, options: &VcsOptions) -> Result<CommitLog, String> {
        fetch_internal(path, options, None, None)
    }

    pub fn status(&self) -> LogMillStatus {
        *self.status.lock().unwrap()
    }

    pub fn is_finished(&self) -> bool {
        self.status() != LogMillStatus::Fetching
    }

    /// Take the result once finished (`getLog` / `getError`). Returns `None`
    /// while still fetching or if already taken. An empty error string means
    /// "no error message" (the C++ code quits silently in that case).
    pub fn take_result(&mut self) -> Option<Result<CommitLog, String>> {
        if let Some(res) = self.cached_result.take() {
            return Some(res);
        }
        if let Ok(res) = self.result_rx.try_recv() {
            if let Some(handle) = self.thread_handle.take() {
                let _ = handle.join();
            }
            return Some(res);
        }
        None
    }

    /// Block until the fetch has finished. The result stays available to
    /// [`LogMill::take_result`].
    pub fn wait(&mut self) {
        if self.cached_result.is_none()
            && let Ok(res) = self.result_rx.recv()
        {
            self.cached_result = Some(res);
            if let Some(handle) = self.thread_handle.take() {
                let _ = handle.join();
            }
        }
    }

    /// Request cancellation (e.g. when the user quits while a large log is
    /// being generated). Kills a running VCS command if possible.
    pub fn abort(&mut self) {
        self.abort_flag.store(true, Ordering::SeqCst);
        if let Ok(mut child_opt) = self.child_process.lock()
            && let Some(mut child) = child_opt.take()
        {
            let _ = child.kill();
        }
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

/// Walk up parent directories looking for .git (file or dir), .hg (dir), .bzr (dir), .svn (dir).
/// Matches `RLogMill::findRepository` in `src/logmill.cpp`.
pub fn find_repository(dir: &Path) -> Option<(PathBuf, String)> {
    let canonical = dir.canonicalize().ok()?;
    let mut current = canonical.as_path();

    while current.is_dir() {
        let git_path = current.join(".git");
        let hg_path = current.join(".hg");
        let bzr_path = current.join(".bzr");
        let svn_path = current.join(".svn");

        let log_format = if git_path.exists() {
            Some("git")
        } else if hg_path.is_dir() {
            Some("hg")
        } else if bzr_path.is_dir() {
            Some("bzr")
        } else if svn_path.is_dir() {
            Some("svn")
        } else {
            None
        };

        if let Some(fmt) = log_format {
            return Some((current.to_path_buf(), fmt.to_string()));
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }

    None
}

/// Run a VCS command in `repo_dir` and write output into a temp file.
fn generate_log(
    vcs: &str,
    repo_dir: &Path,
    options: &VcsOptions,
    abort_flag: Option<&Arc<AtomicBool>>,
    child_process: Option<&Arc<Mutex<Option<Child>>>>,
) -> Result<(NamedTempFile, String), String> {
    let command_str = match formats::log_command_with_options(vcs, options) {
        Some(cmd) => cmd,
        None => return Err("failed to generate log file".to_string()),
    };

    let temp_file = NamedTempFile::new().map_err(|e| e.to_string())?;
    let temp_file_out = temp_file.reopen().map_err(|e| e.to_string())?;

    let parts = match shlex_split(&command_str) {
        Some(p) if !p.is_empty() => p,
        _ => return Err("failed to generate log file".to_string()),
    };

    let program = &parts[0];
    let args = &parts[1..];

    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd.current_dir(repo_dir);
    cmd.stdout(temp_file_out);

    let child = cmd
        .spawn()
        .map_err(|_| "failed to generate log file".to_string())?;

    let local_child = Arc::new(Mutex::new(Some(child)));
    if let Some(cp) = child_process
        && let Ok(mut lock) = cp.lock()
        && let Ok(mut local_lock) = local_child.lock()
    {
        *lock = local_lock.take();
    }

    let active_child = if let Some(cp) = child_process {
        cp
    } else {
        &local_child
    };

    loop {
        if let Some(af) = abort_flag
            && af.load(Ordering::SeqCst)
        {
            if let Ok(mut lock) = active_child.lock()
                && let Some(mut ch) = lock.take()
            {
                let _ = ch.kill();
            }
            return Err("aborted".to_string());
        }

        let is_done = if let Ok(mut lock) = active_child.lock() {
            if let Some(ref mut ch) = *lock {
                match ch.try_wait() {
                    Ok(Some(status)) => {
                        if !status.success() {
                            return Err("failed to generate log file".to_string());
                        }
                        true
                    }
                    Ok(None) => false,
                    Err(_) => return Err("failed to generate log file".to_string()),
                }
            } else {
                return Err("aborted".to_string());
            }
        } else {
            false
        };

        if is_done {
            break;
        }

        thread::sleep(std::time::Duration::from_millis(50));
    }

    Ok((temp_file, command_str))
}

pub(crate) fn shlex_split(s: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let chars = s.chars().peekable();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;

    for c in chars {
        match c {
            '\'' if !in_double => {
                in_single = !in_single;
            }
            '"' if !in_single => {
                in_double = !in_double;
            }
            c if c.is_whitespace() && !in_single && !in_double => {
                if !current.is_empty() {
                    words.push(current);
                    current = String::new();
                }
            }
            c => {
                current.push(c);
            }
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    if in_single || in_double {
        None
    } else {
        Some(words)
    }
}

fn fetch_internal(
    path: &str,
    options: &VcsOptions,
    abort_flag: Option<&Arc<AtomicBool>>,
    child_process: Option<&Arc<Mutex<Option<Child>>>>,
) -> Result<CommitLog, String> {
    let abort_clone = abort_flag
        .cloned()
        .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));

    if !options.github.is_empty() {
        let target = crate::github::GitHubTarget::parse(&options.github)?;
        return crate::github::GitHubWatcher::spawn(target, options.clone(), abort_clone);
    } else if crate::github::GitHubTarget::looks_like_github_path(path) {
        let target = crate::github::GitHubTarget::parse(path)?;
        return crate::github::GitHubWatcher::spawn(target, options.clone(), abort_clone);
    } else if options.live {
        let path_obj = Path::new(path);
        if path_obj.is_dir()
            && let Some((repo_dir, fmt)) = find_repository(path_obj)
            && fmt == "git"
        {
            return crate::live::LiveGitWatcher::spawn(repo_dir, options.clone(), abort_clone);
        }
    }

    let mut log_format = options.log_format.clone();
    let mut logfile = path.to_string();

    let path_obj = Path::new(path);
    let is_dir = path_obj.is_dir();

    if log_format.is_empty()
        && logfile != "-"
        && is_dir
        && let Some((repo_path, fmt)) = find_repository(path_obj)
    {
        logfile = repo_path.to_string_lossy().to_string();
        log_format = fmt;
    }

    let mut clog = if !log_format.is_empty() {
        let fetch = |fmt: &str| try_fetch_format(&logfile, fmt, options, abort_flag, child_process);
        match log_format.as_str() {
            // The deprecated raw git format has no log-format value of its
            // own: `git` falls back to it.
            "git" => fetch("git").or_else(|| fetch("gitraw")),
            "cvs" => fetch("cvs-exp"),
            fmt => fetch(fmt),
        }
    } else {
        // Auto-detect order: git, hg, bzr, gitraw, cvs-exp, svn, cvs2cl, custom, apache
        let formats = [
            "git", "hg", "bzr", "gitraw", "cvs-exp", "svn", "cvs2cl", "custom", "apache",
        ];
        let mut found = None;
        for fmt in formats {
            if let Some(af) = abort_flag
                && af.load(Ordering::SeqCst)
            {
                break;
            }
            if let Some(log) = try_fetch_format(&logfile, fmt, options, abort_flag, child_process) {
                found = Some(log);
                break;
            }
        }
        found
    };

    if let Some(ref mut l) = clog {
        // Find the first commit at or after start_timestamp, as
        // `RLogMill::run` does. A stream also has to start with a valid
        // commit: the commit buffered by the format check has not been
        // validated (C++ passes it to the app anyway). Streams wait for input
        // where C++ polls.
        let start = options.start_timestamp;
        if start != 0 || !l.is_seekable() {
            let aborted = || abort_flag.is_some_and(|af| af.load(Ordering::SeqCst));
            l.wait_for_input(true);
            while !aborted() && !l.at_end() {
                if let Some(commit) = l.next_commit()
                    && (start == 0 || commit.timestamp >= start)
                {
                    l.buffer_commit(commit);
                    break;
                }
            }
            l.wait_for_input(false);
        }
        return Ok(clog.unwrap());
    }

    // Match logmill.cpp error strings:
    if is_dir {
        if !log_format.is_empty() {
            if options.start_timestamp != 0 || options.stop_timestamp != 0 {
                Err("failed to generate log file for the specified time period".to_string())
            } else {
                Err("failed to generate log file".to_string())
            }
        } else {
            Err("directory not supported".to_string())
        }
    } else {
        Err("unsupported log format (you may need to regenerate your log file)".to_string())
    }
}

fn try_fetch_format(
    logfile: &str,
    format: &str,
    options: &VcsOptions,
    abort_flag: Option<&Arc<AtomicBool>>,
    child_process: Option<&Arc<Mutex<Option<Child>>>>,
) -> Option<CommitLog> {
    let path = Path::new(logfile);
    if path.is_dir() {
        // Only git, hg, bzr, svn support generating from directory
        match format {
            "git" | "hg" | "bzr" | "svn" => {}
            _ => return None,
        }

        let (temp_file, command_str) = if format == "git" && options.git_backend == "in-process" {
            match crate::in_process_git::generate_in_process_git_log(path, options) {
                Ok(temp) => (temp, "in-process git (gitoxide)".to_string()),
                Err(_) => return None,
            }
        } else {
            generate_log(format, path, options, abort_flag, child_process).ok()?
        };
        let file = File::open(temp_file.path()).ok()?;
        let seekable = SeekableLog::new(file, Some(temp_file)).ok()?;
        let mut clog =
            CommitLog::from_seekable(format, Some(command_str), seekable, options.clone());
        if clog.check_format() {
            Some(clog)
        } else {
            None
        }
    } else {
        CommitLog::open_file(logfile, format, options)
    }
}
