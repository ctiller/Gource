//! Bazaar (bzr) log format parser.
//! Port of `src/formats/bzr.cpp`.

use crate::commit::Commit;
use crate::options::VcsOptions;
use regex::Regex;
use std::sync::LazyLock;

static BZR_COMMIT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^ *([\d.]+) (.+)\t(\d{4})-(\d+)-(\d+)(?: \{[^}]+})?(?: \[merge\])?$").unwrap()
});

static BZR_FILE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^ *([AMDR])  (.*[^/])$").unwrap());

pub fn log_command(options: &VcsOptions) -> String {
    let start = if options.start_timestamp != 0 {
        format!("date:{}", format_timestamp_date(options.start_timestamp))
    } else {
        "1".to_string()
    };

    let stop = if options.stop_timestamp != 0 {
        format!("date:{}", format_timestamp_date(options.stop_timestamp))
    } else {
        "-1".to_string()
    };

    let range = format!("{}..{}", start, stop);
    format!("bzr log --verbose -r {} --short -n0 --forward", range)
}

fn format_timestamp_date(timestamp: i64) -> String {
    use chrono::TimeZone;
    if let Some(dt) = chrono::Local.timestamp_opt(timestamp, 0).single() {
        dt.format("%Y-%m-%d").to_string()
    } else {
        String::new()
    }
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

    let caps = match BZR_COMMIT_REGEX.captures(&line) {
        Some(c) => c,
        None => return false,
    };

    commit.username = caps.get(2).map_or("", |m| m.as_str()).to_string();

    let year: i32 = caps
        .get(3)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    let month: u32 = caps
        .get(4)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    let day: u32 = caps
        .get(5)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);

    let naive_date = match chrono::NaiveDate::from_ymd_opt(year, month, day) {
        Some(d) => d,
        None => return false,
    };
    let naive_dt = naive_date.and_time(chrono::NaiveTime::MIN);

    commit.timestamp = gource_core::datetime::local_timestamp(&naive_dt);

    while get_line(&mut line) && !line.is_empty() {
        if let Some(fcaps) = BZR_FILE_REGEX.captures(&line) {
            let action = fcaps.get(1).map_or("M", |m| m.as_str());
            let file = fcaps.get(2).map_or("", |m| m.as_str());
            commit.add_file(file, action, options);
        }
    }

    true
}
