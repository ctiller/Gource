//! Git log format parser.
//! Port of `src/formats/git.cpp`.

use crate::commit::Commit;
use crate::options::VcsOptions;

/// Build the git log command string.
/// Matches `GitCommitLog::logCommand()` in `src/formats/git.cpp`.
pub fn log_command(options: &VcsOptions) -> String {
    let mut cmd = String::from("git log --reverse --raw --encoding=UTF-8 --no-renames");

    // Check git version or default to including --no-show-signature.
    // Git on modern systems is >= 2.10.
    let git_ver = read_git_version();
    if git_ver.0 == 0 || git_ver.0 > 2 || (git_ver.0 == 2 && git_ver.1 >= 10) {
        cmd.push_str(" --no-show-signature");
    }

    if options.author_time {
        cmd.push_str(" --pretty=format:user:%aN%n%at");
    } else {
        cmd.push_str(" --pretty=format:user:%aN%n%ct");
    }

    if options.start_timestamp != 0 {
        let date_str = format_timestamp_date(options.start_timestamp);
        cmd.push_str(" --since ");
        cmd.push_str(&date_str);
    }

    if options.stop_timestamp != 0 {
        let date_str = format_timestamp_date(options.stop_timestamp);
        cmd.push_str(" --until ");
        cmd.push_str(&date_str);
    }

    if !options.git_branch.is_empty() {
        cmd.push(' ');
        cmd.push_str(&options.git_branch);
    }

    cmd
}

fn format_timestamp_date(timestamp: i64) -> String {
    use chrono::TimeZone;
    if let Some(dt) = chrono::Local.timestamp_opt(timestamp, 0).single() {
        dt.format("%Y-%m-%d").to_string()
    } else {
        String::new()
    }
}

fn read_git_version() -> (u32, u32, u32) {
    let output = match std::process::Command::new("git").arg("--version").output() {
        Ok(out) => out,
        Err(_) => return (0, 0, 0),
    };
    if !output.status.success() {
        return (0, 0, 0);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    // Regex: ([0-9]+)(?:\.([0-9]+))?(?:\.([0-9]+))?
    let re = regex::Regex::new(r"([0-9]+)(?:\.([0-9]+))?(?:\.([0-9]+))?").unwrap();
    if let Some(caps) = re.captures(&text) {
        let major = caps
            .get(1)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        let minor = caps
            .get(2)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        let patch = caps
            .get(3)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        (major, minor, patch)
    } else {
        (0, 0, 0)
    }
}

/// Parse a git commit from a line reader.
pub fn parse_commit<F>(mut get_line: F, commit: &mut Commit, options: &VcsOptions) -> bool
where
    F: FnMut(&mut String) -> bool,
{
    let mut line = String::new();
    commit.username.clear();
    commit.files.clear();

    while get_line(&mut line) && !line.is_empty() {
        if line.starts_with("user:") {
            commit.username = line[5..].to_string();

            if !get_line(&mut line) {
                return false;
            }

            commit.timestamp = line.parse::<i64>().unwrap_or(0);
            if commit.timestamp == 0 {
                return false;
            }
            continue;
        }

        if commit.username.is_empty() {
            return false;
        }

        let tab = match line.find('\t') {
            Some(t) => t,
            None => continue,
        };

        if tab == 0 || tab == line.len() - 1 {
            continue;
        }

        let status = &line[tab - 1..tab];
        let mut file = &line[tab + 1..];

        if file.is_empty() {
            continue;
        }

        // Check for and remove double quotes
        if file.starts_with('"') && file.ends_with('"') {
            if file.len() <= 2 {
                continue;
            }
            file = &file[1..file.len() - 1];
        }

        commit.add_file(file, status, options);
    }

    !commit.username.is_empty()
}
