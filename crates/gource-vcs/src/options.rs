//! Options controlling log generation and parsing.

use gource_core::StringHasher;

/// Regex filters applied while reading commits (see `RCommit::addFile` and
/// `RCommit::isValid`). Patterns use PCRE-like syntax (`fancy_regex`), and
/// like the C++ `Regex::match` they are unanchored searches.
#[derive(Debug, Clone, Default)]
pub struct CommitFilters {
    /// `--file-filter`: drop files whose (raw, unnormalised) name matches any.
    pub file_filters: Vec<fancy_regex::Regex>,
    /// `--file-show-filter`: keep only files matching all of these.
    pub file_show_filters: Vec<fancy_regex::Regex>,
    /// `--user-filter`: drop commits whose user matches any.
    pub user_filters: Vec<fancy_regex::Regex>,
    /// `--user-show-filter`: keep only commits whose user matches all.
    pub user_show_filters: Vec<fancy_regex::Regex>,
}

impl CommitFilters {
    /// Whether a file (raw name as it appears in the log) passes the filters.
    pub fn allows_file(&self, filename: &str) -> bool {
        let matches = |r: &fancy_regex::Regex| r.is_match(filename).unwrap_or(false);
        !self.file_filters.iter().any(matches) && self.file_show_filters.iter().all(matches)
    }

    /// Whether a user passes the filters.
    pub fn allows_user(&self, username: &str) -> bool {
        let matches = |r: &fancy_regex::Regex| r.is_match(username).unwrap_or(false);
        !self.user_filters.iter().any(matches) && self.user_show_filters.iter().all(matches)
    }
}

/// Everything the VCS layer needs from the settings (instead of reading the
/// global `gGourceSettings` like the C++ code).
#[derive(Debug, Clone, Default)]
pub struct VcsOptions {
    /// `--log-format`: force a format ("git", "svn", ...); empty = auto-detect.
    pub log_format: String,
    /// `--git-branch`.
    pub git_branch: String,
    /// `--author-time`: use author time instead of commit time (git).
    pub author_time: bool,
    /// `--start-date` as a timestamp (0 = unset); passed to VCS commands
    /// where the C++ code does so.
    pub start_timestamp: i64,
    /// `--stop-date` as a timestamp (0 = unset).
    pub stop_timestamp: i64,
    /// True if the path was not given explicitly (defaults to "."). Affects
    /// error messages.
    pub default_path: bool,
    pub filters: CommitFilters,
    /// Seed for file colours derived from extensions.
    pub hasher: StringHasher,
    /// Whether to include numstat when reading git logs.
    pub include_numstat: bool,
    /// Live streaming mode.
    pub live: bool,
    /// Polling interval for live mode (seconds). <= 0.0 defaults to 5.0 in watchers.
    pub live_interval_secs: f32,
    /// Run `git fetch --quiet` before checking for updates in live mode.
    pub live_fetch: bool,
    /// Target GitHub repository or owner for watch mode ("owner/repo" or "owner").
    pub github: String,
    /// Personal access token for GitHub API (or via GITHUB_TOKEN / GH_TOKEN env vars).
    pub github_token: String,
    /// Git backend selection: "auto", "cli", or "in-process".
    pub git_backend: String,
    /// Watch all linked git worktrees for uncommitted in-flight changes.
    pub watch_worktrees: bool,
    /// Polling debounce interval for worktree status checks in seconds (default: 0.25).
    pub worktree_poll_interval: f32,
    /// Opacity multiplier for shadow in-flight files and beams (default: 0.45).
    pub shadow_alpha: f32,
}
