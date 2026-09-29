//! Version control log readers for Gource.
//!
//! * [`commit`]: the commit model shared with the simulation.
//! * [`log`]: [`CommitLog`], an incremental, optionally seekable commit reader
//!   (port of `formats/commitlog.cpp` + `core/seeklog.cpp`).
//! * [`formats`]: one parser per supported log format (port of `formats/*`).
//! * [`logmill`]: background repository detection / log generation
//!   (port of `logmill.cpp`).
//!
//! This crate has no global state: filters, hash seed and VCS options are
//! passed in via [`VcsOptions`].
#![allow(clippy::result_unit_err)]
#![allow(clippy::field_reassign_with_default)]

pub mod commit;
pub mod formats;
pub mod github;
pub mod live;
pub mod log;
pub mod logmill;
pub mod options;

#[cfg(test)]
mod coverage_tests;

pub use commit::{Commit, CommitFile, FileAction};
pub use github::{
    DefaultHttpTransport, GitHubTarget, GitHubWatcher, HttpResponse, HttpTransport,
    resolve_github_token,
};
pub use live::LiveGitWatcher;
pub use log::CommitLog;
pub use logmill::{LogMill, LogMillStatus};
pub use options::{CommitFilters, VcsOptions};

/// Errors produced while locating or reading logs.
#[derive(Debug, thiserror::Error)]
pub enum VcsError {
    /// A user-facing message, identical to the C++ error strings
    /// (e.g. "failed to generate log file").
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// The command line used to generate a log for a VCS (`--log-command git`),
/// or `None` for an unknown VCS name.
pub fn log_command(vcs: &str) -> Option<String> {
    formats::log_command(vcs)
}

/// Port of `Gource::writeCustomLog`: read the log at `path` (a repository
/// directory, a log file or `-` for stdin) and write it in custom log format
/// (`timestamp|username|action|filename`, one line per file) to `output`
/// (`-` for stdout).
pub fn write_custom_log(path: &str, output: &str, options: &VcsOptions) -> Result<(), VcsError> {
    use std::io::Write;

    let mut commitlog =
        logmill::LogMill::fetch_blocking(path, options).map_err(VcsError::Message)?;

    let mut out_writer: Box<dyn Write> = if output == "-" {
        Box::new(std::io::stdout())
    } else {
        Box::new(std::fs::File::create(output)?)
    };

    // Convert all of a stream, rather than stopping when no input has
    // arrived yet (C++ stops there).
    commitlog.wait_for_input(true);

    while !commitlog.is_finished() {
        let commit = match commitlog.next_commit() {
            Some(c) => c,
            None => {
                if !commitlog.is_seekable() {
                    break;
                }
                continue;
            }
        };

        for file in &commit.files {
            if file.is_binary {
                writeln!(
                    out_writer,
                    "{}|{}|{}|{}|-|-",
                    commit.timestamp,
                    commit.username,
                    file.action.code(),
                    file.filename
                )?;
            } else if let (Some(added), Some(removed)) = (file.lines_added, file.lines_removed) {
                writeln!(
                    out_writer,
                    "{}|{}|{}|{}|{}|{}",
                    commit.timestamp,
                    commit.username,
                    file.action.code(),
                    file.filename,
                    added,
                    removed
                )?;
            } else {
                writeln!(
                    out_writer,
                    "{}|{}|{}|{}",
                    commit.timestamp,
                    commit.username,
                    file.action.code(),
                    file.filename
                )?;
            }
        }
    }

    out_writer.flush()?;
    Ok(())
}
