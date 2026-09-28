//! CVS cvs-exp log format parser (`cvs-exp.pl -notree`).
//! Port of `src/formats/cvs-exp.cpp`.

use crate::commit::Commit;
use crate::options::VcsOptions;
use regex::Regex;
use std::sync::LazyLock;

static CVSEXP_COMMITNO_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([0-9]{6}):").unwrap());
static CVSEXP_BRANCH_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^BRANCH \[(.+)\]$").unwrap());
static CVSEXP_DATE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\(date: ([0-9]{4})[-/]([0-9]{2})[-/]([0-9]{2}) ([0-9]{2}):([0-9]{2}):([0-9]{2})(?: [+-][0-9]{4})?;(.+)$").unwrap()
});
static CVSEXP_DETAIL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"author: ([^;]+);  state: ([^;]+);(.+)$").unwrap());
static CVSEXP_ENTRY_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\| (.+),v:([0-9.]+),?").unwrap());
static CVSEXP_END_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(=+)$").unwrap());

pub fn log_command() -> String {
    "cvs-exp.pl -notree".to_string()
}

pub fn parse_commit<F>(mut get_line: F, commit: &mut Commit, options: &VcsOptions) -> bool
where
    F: FnMut(&mut String) -> bool,
{
    let mut line = String::new();
    commit.username.clear();
    commit.files.clear();

    if !get_line(&mut line) {
        return false;
    }

    // Skip empty line if there is one
    if line.is_empty() && !get_line(&mut line) {
        return false;
    }

    // Read commit no
    if !CVSEXP_COMMITNO_REGEX.is_match(&line) {
        return false;
    }

    if !get_line(&mut line) {
        return false;
    }

    // Should be a branch (optional)
    if CVSEXP_BRANCH_REGEX.is_match(&line) {
        if !get_line(&mut line) {
            return false;
        }
        if !line.is_empty() {
            return false;
        }
        if !get_line(&mut line) {
            return false;
        }
    }

    // Parse date
    let date_caps = match CVSEXP_DATE_REGEX.captures(&line) {
        Some(c) => c,
        None => return false,
    };

    let year: i32 = date_caps.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
    let month: u32 = date_caps.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
    let day: u32 = date_caps.get(3).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
    let hour: u32 = date_caps.get(4).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
    let min: u32 = date_caps.get(5).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
    let sec: u32 = date_caps.get(6).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);

    let naive_date = match chrono::NaiveDate::from_ymd_opt(year, month, day) {
        Some(d) => d,
        None => return false,
    };
    let naive_dt = match naive_date.and_hms_opt(hour, min, sec) {
        Some(dt) => dt,
        None => return false,
    };

    use chrono::TimeZone;
    commit.timestamp = match chrono::Local.from_local_datetime(&naive_dt) {
        chrono::LocalResult::Single(dt) => dt.timestamp(),
        chrono::LocalResult::Ambiguous(dt1, _) => dt1.timestamp(),
        chrono::LocalResult::None => 0,
    };

    let rest = date_caps.get(7).map_or("", |m| m.as_str());
    let detail_caps = match CVSEXP_DETAIL_REGEX.captures(rest) {
        Some(c) => c,
        None => return false,
    };

    commit.username = detail_caps.get(1).map_or("", |m| m.as_str()).to_string();
    let commit_state = detail_caps.get(2).map_or("", |m| m.as_str());
    let commit_action = if commit_state == "dead" { "D" } else { "M" };

    if !get_line(&mut line) {
        return false;
    }

    while let Some(entry_caps) = CVSEXP_ENTRY_REGEX.captures(&line) {
        let filename = entry_caps.get(1).map_or("", |m| m.as_str());
        // ignore files in Attic
        if !filename.contains("/Attic/") {
            commit.add_file(filename, commit_action, options);
        }

        if !get_line(&mut line) {
            return false;
        }
    }

    // The while loop already read the line after entries into `line`.
    // If it's not empty, read commit message until blank line
    if !line.is_empty() {
        while get_line(&mut line) && !line.is_empty() {}
    }

    // Read until end of commit or eof
    while get_line(&mut line) {
        if CVSEXP_END_REGEX.is_match(&line) {
            break;
        }
    }

    true
}
