//! Git log format parser.
//! Port of `src/formats/git.cpp`.

use crate::commit::Commit;
use crate::options::VcsOptions;
use regex::Regex;
use std::sync::LazyLock;

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
    std::process::Command::new("git")
        .arg("--version")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map_or((0, 0, 0), |out| {
            parse_git_version(&String::from_utf8_lossy(&out.stdout))
        })
}

/// Parses the first `major[.minor[.patch]]` in `git --version` output;
/// missing parts are 0.
fn parse_git_version(text: &str) -> (u32, u32, u32) {
    static VERSION_REGEX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"([0-9]+)(?:\.([0-9]+))?(?:\.([0-9]+))?").unwrap());
    let Some(caps) = VERSION_REGEX.captures(text) else {
        return (0, 0, 0);
    };
    let part = |i| {
        caps.get(i)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0)
    };
    (part(1), part(2), part(3))
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

        // One byte, like C++'s substr(tab - 1, 1). Half of a multi-byte
        // character isn't a status; `get` avoids panicking on it.
        let status = line.get(tab - 1..tab).unwrap_or("");
        // Non-empty: the tab isn't the last byte.
        let mut file = &line[tab + 1..];

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_version_parsing() {
        assert_eq!(parse_git_version("git version 2.39.5\n"), (2, 39, 5));
        assert_eq!(parse_git_version("git version 2.10"), (2, 10, 0));
        assert_eq!(parse_git_version("git version 3"), (3, 0, 0));
        assert_eq!(parse_git_version("no version here"), (0, 0, 0));
    }

    #[test]
    fn multibyte_character_before_tab_is_not_a_status() {
        let lines = [
            "user:Alice",
            "1600000000",
            "\u{e9}\tsrc/main.rs",
            "M\tREADME",
        ];
        let mut it = lines.iter();
        let mut commit = Commit::default();
        let ok = parse_commit(
            |line: &mut String| match it.next() {
                Some(l) => {
                    *line = (*l).to_string();
                    true
                }
                None => false,
            },
            &mut commit,
            &VcsOptions::default(),
        );
        assert!(ok);
        let names: Vec<_> = commit.files.iter().map(|f| f.filename.as_str()).collect();
        assert_eq!(names, ["/src/main.rs", "/README"]);
    }
}
