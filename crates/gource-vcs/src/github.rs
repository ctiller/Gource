//! GitHub repository / owner watcher (`--github`).
//!
//! Streams commits or push events from GitHub in real time or for a single batch pass.

use crate::log::CommitLog;
use crate::options::VcsOptions;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::thread;
use std::time::Duration;

/// Target GitHub resource: either a single repository or an entire owner/user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitHubTarget {
    Repo { owner: String, repo: String },
    Owner { owner: String },
}

impl GitHubTarget {
    /// Parses a GitHub target specifier.
    ///
    /// Accepts:
    /// - `"owner/repo"` -> `Repo { owner, repo }`
    /// - `"owner/*"` or `"owner"` -> `Owner { owner }`
    /// - URL forms like `"https://github.com/owner/repo"`, `"github:owner/repo"`, with optional `".git"`.
    pub fn parse(input: &str) -> Result<Self, String> {
        let mut s = input.trim();
        if s.is_empty() {
            return Err("invalid github target".to_string());
        }

        if let Some(rest) = s.strip_prefix("github:") {
            s = rest.trim();
        } else if let Some(rest) = s.strip_prefix("https://github.com/") {
            s = rest.trim();
        } else if let Some(rest) = s.strip_prefix("http://github.com/") {
            s = rest.trim();
        }

        let s = s.trim_matches('/');
        let s = s.strip_suffix(".git").unwrap_or(s);

        if s.is_empty() {
            return Err("invalid github target".to_string());
        }

        let parts: Vec<&str> = s.split('/').collect();
        match parts.len() {
            1 => {
                let owner = parts[0];
                if !is_valid_github_ident(owner) {
                    return Err("invalid github target".to_string());
                }
                Ok(GitHubTarget::Owner {
                    owner: owner.to_string(),
                })
            }
            2 => {
                let owner = parts[0];
                let repo = parts[1];
                if !is_valid_github_ident(owner) {
                    return Err("invalid github target".to_string());
                }
                if repo == "*" {
                    Ok(GitHubTarget::Owner {
                        owner: owner.to_string(),
                    })
                } else {
                    if !is_valid_github_ident(repo) {
                        return Err("invalid github target".to_string());
                    }
                    Ok(GitHubTarget::Repo {
                        owner: owner.to_string(),
                        repo: repo.to_string(),
                    })
                }
            }
            _ => Err("invalid github target".to_string()),
        }
    }

    /// Checks if a string looks like a GitHub URL or prefixed path.
    pub fn looks_like_github_path(path: &str) -> bool {
        let trimmed = path.trim();
        trimmed.starts_with("github:")
            || trimmed.starts_with("https://github.com/")
            || trimmed.starts_with("http://github.com/")
    }
}

/// Strict identifier validation for GitHub owner / repository names.
pub fn is_valid_github_ident(s: &str) -> bool {
    if s.is_empty() || s.len() > 100 || s == "." || s == ".." || s.starts_with('-') {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn is_valid_sha(s: &str) -> bool {
    is_valid_github_ident(s) && s.len() <= 64
}

fn is_valid_repo_full_name(s: &str) -> bool {
    let mut parts = s.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(owner), Some(repo), None) => {
            is_valid_github_ident(owner) && is_valid_github_ident(repo)
        }
        _ => false,
    }
}

fn is_valid_git_ref(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && !s.starts_with('-')
        && !s.contains("..")
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
}

/// Resolves the GitHub token from options, environment, or GitHub CLI.
/// Never prints or formats the token in error messages.
pub fn resolve_github_token(explicit: &str) -> Option<String> {
    let trimmed = explicit.trim();
    if !trimmed.is_empty() {
        return Some(trimmed.to_string());
    }
    if let Ok(tok) = std::env::var("GITHUB_TOKEN") {
        let t = tok.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    if let Ok(tok) = std::env::var("GH_TOKEN") {
        let t = tok.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    if let Ok(output) = Command::new("gh").args(["auth", "token"]).output()
        && output.status.success()
    {
        let t = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !t.is_empty() {
            return Some(t);
        }
    }
    None
}

/// Simple HTTP response structure.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        let name_lower = name.to_ascii_lowercase();
        for (k, v) in &self.headers {
            if k.to_ascii_lowercase() == name_lower {
                return Some(v.as_str());
            }
        }
        None
    }
}

/// Trait for HTTP GET requests.
pub trait HttpTransport: Send + Sync + 'static {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<HttpResponse, String>;
}

/// Default HTTP transport supporting local test TCP servers and GitHub API via curl.
pub struct DefaultHttpTransport;

impl HttpTransport for DefaultHttpTransport {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<HttpResponse, String> {
        if url.starts_with("http://127.0.0.1:") || url.starts_with("http://localhost:") {
            Self::get_http_local(url, headers)
        } else if url.starts_with("https://api.github.com/") {
            Self::get_curl(url, headers)
        } else {
            Err("disallowed URL scheme or host".to_string())
        }
    }
}

impl DefaultHttpTransport {
    fn get_http_local(url: &str, headers: &[(&str, &str)]) -> Result<HttpResponse, String> {
        // Parse host:port and path
        let without_proto = url.strip_prefix("http://").unwrap();
        let (host_port, path_query) = match without_proto.find('/') {
            Some(idx) => (&without_proto[..idx], &without_proto[idx..]),
            None => (without_proto, "/"),
        };

        let mut stream = TcpStream::connect(host_port).map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;

        let mut req =
            format!("GET {path_query} HTTP/1.1\r\nHost: {host_port}\r\nConnection: close\r\n");
        for (k, v) in headers {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        req.push_str("\r\n");

        stream
            .write_all(req.as_bytes())
            .map_err(|e| e.to_string())?;

        let mut response_bytes = Vec::new();
        stream
            .read_to_end(&mut response_bytes)
            .map_err(|e| e.to_string())?;

        parse_http_response(&response_bytes)
    }

    fn get_curl(url: &str, headers: &[(&str, &str)]) -> Result<HttpResponse, String> {
        let mut cmd = Command::new("curl");
        cmd.args([
            "--silent",
            "--show-error",
            "--include",
            "--location",
            "--max-time",
            "15",
            "--user-agent",
            "gource-rust/0.57",
        ]);

        for (k, v) in headers {
            cmd.arg("-H");
            cmd.arg(format!("{k}: {v}"));
        }

        cmd.arg("--");
        cmd.arg(url);

        let output = cmd.output().map_err(|e| e.to_string())?;
        if !output.status.success() {
            let stderr_msg = String::from_utf8_lossy(&output.stderr);
            return Err(format!("curl request failed: {stderr_msg}"));
        }

        parse_http_response(&output.stdout)
    }
}

/// Parses raw HTTP response bytes into an `HttpResponse`.
pub fn parse_http_response(bytes: &[u8]) -> Result<HttpResponse, String> {
    // If there are multiple response header blocks (e.g. 100 Continue), find the final block
    let mut cursor = 0;
    let mut last_response = None;

    while cursor < bytes.len() {
        let remaining = &bytes[cursor..];
        let header_end = match remaining.windows(4).position(|w| w == b"\r\n\r\n") {
            Some(pos) => pos,
            None => match remaining.windows(2).position(|w| w == b"\n\n") {
                Some(pos) => pos,
                None => break,
            },
        };

        let delim_len = if &remaining[header_end..header_end + 2] == b"\r\n" {
            4
        } else {
            2
        };
        let header_bytes = &remaining[..header_end];
        let header_str = String::from_utf8_lossy(header_bytes);
        let mut lines = header_str.lines();

        let status_line = lines.next().ok_or("empty HTTP response")?;
        let status_parts: Vec<&str> = status_line.split_whitespace().collect();
        if status_parts.len() < 2 {
            return Err("malformed HTTP status line".to_string());
        }
        let status = status_parts[1].parse::<u16>().map_err(|e| e.to_string())?;

        let mut headers = Vec::new();
        for line in lines {
            if let Some(pos) = line.find(':') {
                let key = line[..pos].trim().to_string();
                let val = line[pos + 1..].trim().to_string();
                headers.push((key, val));
            }
        }

        let body = remaining[header_end + delim_len..].to_vec();
        last_response = Some(HttpResponse {
            status,
            headers,
            body,
        });

        // If status is 100 Continue, skip to next block
        if status == 100 {
            cursor += header_end + delim_len;
        } else {
            break;
        }
    }

    last_response.ok_or_else(|| "failed to parse HTTP response".to_string())
}

/// GitHub repository or owner watcher.
pub struct GitHubWatcher;

impl GitHubWatcher {
    /// Spawns a background thread polling GitHub using the default HTTP transport.
    pub fn spawn(
        target: GitHubTarget,
        options: VcsOptions,
        abort_flag: Arc<AtomicBool>,
    ) -> Result<CommitLog, String> {
        let base_url = "https://api.github.com".to_string();
        let transport = Arc::new(DefaultHttpTransport);
        Self::spawn_with_transport(target, base_url, transport, options, abort_flag)
    }

    /// Spawns a background thread polling GitHub with a custom base URL and transport.
    pub fn spawn_with_transport<T: HttpTransport>(
        target: GitHubTarget,
        base_url: String,
        transport: Arc<T>,
        options: VcsOptions,
        abort_flag: Arc<AtomicBool>,
    ) -> Result<CommitLog, String> {
        let (tx, rx) = channel::<String>();

        let worker_target = target;
        let worker_base_url = base_url.trim_end_matches('/').to_string();
        let worker_options = options.clone();
        let worker_abort = abort_flag;

        thread::Builder::new()
            .name("gource-live-github".to_string())
            .spawn(move || {
                Self::worker_loop(
                    worker_target,
                    worker_base_url,
                    transport,
                    worker_options,
                    worker_abort,
                    tx,
                );
            })
            .map_err(|e| e.to_string())?;

        Ok(CommitLog::from_live_stream("custom", None, rx, options))
    }

    fn worker_loop<T: HttpTransport>(
        target: GitHubTarget,
        base_url: String,
        transport: Arc<T>,
        options: VcsOptions,
        abort_flag: Arc<AtomicBool>,
        tx: Sender<String>,
    ) {
        let token = resolve_github_token(&options.github_token);
        let interval_secs = if options.live_interval_secs > 0.0 {
            options.live_interval_secs
        } else {
            5.0
        };

        let sleep_step = Duration::from_millis(50);
        let total_sleep = Duration::from_secs_f32(interval_secs);

        let mut etag: Option<String> = None;
        let mut seen_shas: HashSet<String> = HashSet::new();

        loop {
            if abort_flag.load(Ordering::SeqCst) {
                return;
            }

            match &target {
                GitHubTarget::Repo { owner, repo } => {
                    let mut url = format!("{base_url}/repos/{owner}/{repo}/commits?per_page=25");
                    if is_valid_git_ref(&options.git_branch) {
                        url.push_str(&format!("&sha={}", options.git_branch));
                    }

                    let mut headers: Vec<(&str, &str)> =
                        vec![("Accept", "application/vnd.github+json")];
                    let auth_header;
                    if let Some(tok) = &token {
                        auth_header = format!("Bearer {tok}");
                        headers.push(("Authorization", &auth_header));
                    }
                    if let Some(tag) = &etag {
                        headers.push(("If-None-Match", tag));
                    }

                    if let Ok(resp) = transport.get(&url, &headers) {
                        if resp.status == 200 {
                            if let Some(new_etag) = resp.header("etag") {
                                etag = Some(new_etag.to_string());
                            }

                            if let Ok(commits_val) =
                                serde_json::from_slice::<serde_json::Value>(&resp.body)
                                && let Some(commits_arr) = commits_val.as_array()
                            {
                                // Collect unseen commits
                                let mut new_commits = Vec::new();
                                for item in commits_arr {
                                    if let Some(sha) = item.get("sha").and_then(|s| s.as_str())
                                        && is_valid_sha(sha)
                                        && !seen_shas.contains(sha)
                                    {
                                        new_commits.push(item.clone());
                                        seen_shas.insert(sha.to_string());
                                    }
                                }

                                // Process oldest to newest
                                new_commits.reverse();

                                let should_fetch_detail = token.is_some() || new_commits.len() <= 5;
                                for item in new_commits {
                                    let sha = match item.get("sha").and_then(|s| s.as_str()) {
                                        Some(s) if is_valid_sha(s) => s,
                                        _ => continue,
                                    };

                                    let files = if should_fetch_detail {
                                        let detail_url = format!(
                                            "{base_url}/repos/{owner}/{repo}/commits/{sha}"
                                        );
                                        let mut detail_headers: Vec<(&str, &str)> =
                                            vec![("Accept", "application/vnd.github+json")];
                                        let detail_auth;
                                        if let Some(tok) = &token {
                                            detail_auth = format!("Bearer {tok}");
                                            detail_headers.push(("Authorization", &detail_auth));
                                        }

                                        if let Ok(detail_resp) =
                                            transport.get(&detail_url, &detail_headers)
                                            && detail_resp.status == 200
                                        {
                                            serde_json::from_slice::<serde_json::Value>(
                                                &detail_resp.body,
                                            )
                                            .ok()
                                            .and_then(|v| v.get("files").cloned())
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    };

                                    if !emit_github_commit(&item, files.as_ref(), None, &tx) {
                                        return;
                                    }
                                }
                            }
                        } else if resp.status == 304 {
                            // Not modified
                        }
                    }
                }
                GitHubTarget::Owner { owner } => {
                    let url = format!("{base_url}/users/{owner}/events?per_page=30");
                    let mut headers: Vec<(&str, &str)> =
                        vec![("Accept", "application/vnd.github+json")];
                    let auth_header;
                    if let Some(tok) = &token {
                        auth_header = format!("Bearer {tok}");
                        headers.push(("Authorization", &auth_header));
                    }
                    if let Some(tag) = &etag {
                        headers.push(("If-None-Match", tag));
                    }

                    if let Ok(resp) = transport.get(&url, &headers)
                        && resp.status == 200
                    {
                        if let Some(new_etag) = resp.header("etag") {
                            etag = Some(new_etag.to_string());
                        }

                        if let Ok(events_val) =
                            serde_json::from_slice::<serde_json::Value>(&resp.body)
                            && let Some(events_arr) = events_val.as_array()
                        {
                            // Collect commits from PushEvents
                            let mut push_commits = Vec::new();

                            for ev in events_arr {
                                let ev_type = ev.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                if ev_type == "PushEvent" {
                                    let repo_name = ev
                                        .get("repo")
                                        .and_then(|r| r.get("name"))
                                        .and_then(|n| n.as_str())
                                        .unwrap_or("");
                                    if !is_valid_repo_full_name(repo_name) {
                                        continue;
                                    }
                                    if let Some(payload) = ev.get("payload") {
                                        if let Some(commits) =
                                            payload.get("commits").and_then(|c| c.as_array())
                                        {
                                            for c in commits {
                                                if let Some(sha) =
                                                    c.get("sha").and_then(|s| s.as_str())
                                                    && is_valid_sha(sha)
                                                {
                                                    let key = format!("{repo_name}:{sha}");
                                                    if !seen_shas.contains(&key) {
                                                        push_commits.push((
                                                            repo_name.to_string(),
                                                            sha.to_string(),
                                                            c.clone(),
                                                        ));
                                                        seen_shas.insert(key);
                                                    }
                                                }
                                            }
                                        } else if let Some(head) =
                                            payload.get("head").and_then(|h| h.as_str())
                                            && is_valid_sha(head)
                                        {
                                            let key = format!("{repo_name}:{head}");
                                            if !seen_shas.contains(&key) {
                                                push_commits.push((
                                                    repo_name.to_string(),
                                                    head.to_string(),
                                                    serde_json::json!({ "sha": head }),
                                                ));
                                                seen_shas.insert(key);
                                            }
                                        }
                                    }
                                }
                            }

                            // If no PushEvents found and seen_shas is empty, fall back to owner repos
                            if push_commits.is_empty() && seen_shas.is_empty() {
                                let repos_url = format!(
                                    "{base_url}/users/{owner}/repos?sort=pushed&per_page=5"
                                );
                                let mut rheaders: Vec<(&str, &str)> =
                                    vec![("Accept", "application/vnd.github+json")];
                                let rauth;
                                if let Some(tok) = &token {
                                    rauth = format!("Bearer {tok}");
                                    rheaders.push(("Authorization", &rauth));
                                }
                                if let Ok(rresp) = transport.get(&repos_url, &rheaders)
                                    && rresp.status == 200
                                    && let Ok(repos_val) =
                                        serde_json::from_slice::<serde_json::Value>(&rresp.body)
                                    && let Some(repos_arr) = repos_val.as_array()
                                {
                                    for r in repos_arr {
                                        let full_name = r
                                            .get("full_name")
                                            .and_then(|n| n.as_str())
                                            .unwrap_or("");
                                        if !is_valid_repo_full_name(full_name) {
                                            continue;
                                        }
                                        let rcommits_url = format!(
                                            "{base_url}/repos/{full_name}/commits?per_page=5"
                                        );
                                        if let Ok(rc_resp) = transport.get(&rcommits_url, &rheaders)
                                            && rc_resp.status == 200
                                            && let Ok(rc_val) =
                                                serde_json::from_slice::<serde_json::Value>(
                                                    &rc_resp.body,
                                                )
                                            && let Some(rc_arr) = rc_val.as_array()
                                        {
                                            for rc in rc_arr {
                                                if let Some(sha) =
                                                    rc.get("sha").and_then(|s| s.as_str())
                                                    && is_valid_sha(sha)
                                                {
                                                    let key = format!("{full_name}:{sha}");
                                                    if !seen_shas.contains(&key) {
                                                        push_commits.push((
                                                            full_name.to_string(),
                                                            sha.to_string(),
                                                            rc.clone(),
                                                        ));
                                                        seen_shas.insert(key);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Process oldest to newest
                            push_commits.reverse();

                            let should_fetch_detail = token.is_some() || push_commits.len() <= 5;
                            for (repo_full_name, sha, item) in push_commits {
                                let mut detail_item = item;
                                let mut files = None;

                                if should_fetch_detail {
                                    let detail_url =
                                        format!("{base_url}/repos/{repo_full_name}/commits/{sha}");
                                    let mut detail_headers: Vec<(&str, &str)> =
                                        vec![("Accept", "application/vnd.github+json")];
                                    let detail_auth;
                                    if let Some(tok) = &token {
                                        detail_auth = format!("Bearer {tok}");
                                        detail_headers.push(("Authorization", &detail_auth));
                                    }

                                    if let Ok(detail_resp) =
                                        transport.get(&detail_url, &detail_headers)
                                        && detail_resp.status == 200
                                        && let Ok(v) = serde_json::from_slice::<serde_json::Value>(
                                            &detail_resp.body,
                                        )
                                    {
                                        files = v.get("files").cloned();
                                        detail_item = v;
                                    }
                                }

                                let repo_short_name =
                                    repo_full_name.split('/').nth(1).unwrap_or(&repo_full_name);

                                if !emit_github_commit(
                                    &detail_item,
                                    files.as_ref(),
                                    Some(repo_short_name),
                                    &tx,
                                ) {
                                    return;
                                }
                            }
                        }
                    }
                }
            }

            // If not in live mode, stop after initial pass
            if !options.live {
                return;
            }

            // Sleep in 50ms increments
            let mut elapsed = Duration::ZERO;
            while elapsed < total_sleep {
                if abort_flag.load(Ordering::SeqCst) {
                    return;
                }
                let sleep_duration = sleep_step.min(total_sleep - elapsed);
                thread::sleep(sleep_duration);
                elapsed += sleep_duration;
            }
        }
    }
}

/// Formats and emits a GitHub commit as custom log lines + empty sentinel.
fn emit_github_commit(
    item: &serde_json::Value,
    files_val: Option<&serde_json::Value>,
    repo_prefix: Option<&str>,
    tx: &Sender<String>,
) -> bool {
    let commit_obj = item.get("commit");

    let username = item
        .get("author")
        .and_then(|a| a.get("login"))
        .and_then(|l| l.as_str())
        .or_else(|| {
            commit_obj
                .and_then(|c| c.get("author"))
                .and_then(|a| a.get("name"))
                .and_then(|n| n.as_str())
        })
        .unwrap_or("unknown");

    let date_str = commit_obj
        .and_then(|c| c.get("author"))
        .and_then(|a| a.get("date"))
        .and_then(|d| d.as_str())
        .or_else(|| {
            commit_obj
                .and_then(|c| c.get("committer"))
                .and_then(|c| c.get("date"))
                .and_then(|d| d.as_str())
        })
        .unwrap_or("");

    let timestamp = parse_iso_date(date_str).unwrap_or(0);

    let default_files = vec![serde_json::json!({
        "filename": "commit.patch",
        "status": "modified",
        "additions": 1,
        "deletions": 1
    })];

    let files_arr = files_val
        .and_then(|f| f.as_array())
        .filter(|arr| !arr.is_empty())
        .unwrap_or(&default_files);

    let mut emitted = false;

    for f in files_arr {
        let raw_filename = f.get("filename").and_then(|n| n.as_str()).unwrap_or("file");
        let filename = match repo_prefix {
            Some(prefix) => format!("{prefix}/{}", raw_filename.trim_start_matches('/')),
            None => raw_filename.trim_start_matches('/').to_string(),
        };

        let status = f
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("modified");
        let action = match status {
            "added" => "A",
            "removed" => "D",
            _ => "M",
        };

        let additions = f.get("additions").and_then(|a| a.as_u64()).unwrap_or(0);
        let deletions = f.get("deletions").and_then(|d| d.as_u64()).unwrap_or(0);

        let line = format!("{timestamp}|{username}|{action}|/{filename}|{additions}|{deletions}");

        if tx.send(line).is_err() {
            return false;
        }
        emitted = true;
    }

    if emitted && tx.send(String::new()).is_err() {
        return false;
    }

    true
}

fn parse_iso_date(s: &str) -> Option<i64> {
    if s.is_empty() {
        return None;
    }
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp())
        .or_else(|| {
            // Try standard format %Y-%m-%dT%H:%M:%SZ
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%SZ")
                .ok()
                .map(|dt| dt.and_utc().timestamp())
        })
}
