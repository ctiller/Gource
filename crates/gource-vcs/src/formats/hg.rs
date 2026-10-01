//! Mercurial (hg) log format parser.
//! Port of `src/formats/hg.cpp`.

use crate::commit::Commit;
use crate::commit::CommitExt;
use crate::options::VcsOptions;
use regex::Regex;
use std::sync::LazyLock;

static HG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([0-9]+) -?[0-9]+\|([^|]+)\|([ADM]?)\|(.+)$").unwrap());

pub fn log_command(options: &VcsOptions) -> String {
    // Port of MercurialLog::logCommand(): gSDLAppResourceDir + "gource.style"
    let style_path = format!("{}gource.style", gource_core::resources::resource_dir());

    let start_date = if options.start_timestamp != 0 {
        format_timestamp_date(options.start_timestamp)
    } else {
        String::new()
    };

    let stop_date = if options.stop_timestamp != 0 {
        format_timestamp_date(options.stop_timestamp)
    } else {
        String::new()
    };

    let range = if !start_date.is_empty() && !stop_date.is_empty() {
        format!("--date '{} to {}'", start_date, stop_date)
    } else if !start_date.is_empty() {
        format!("--date '>{}'", start_date)
    } else if !stop_date.is_empty() {
        format!("--date '<{}'", stop_date)
    } else {
        String::new()
    };

    let log_command = if range.is_empty() {
        format!("hg log  -r 0:tip --style '{}'", style_path)
    } else {
        format!("hg log {} -r 0:tip --style '{}'", range, style_path)
    };

    #[cfg(windows)]
    let log_command = log_command.replace('\'', "\"");

    log_command
}

fn format_timestamp_date(timestamp: i64) -> String {
    use chrono::TimeZone;
    if let Some(dt) = chrono::Local.timestamp_opt(timestamp, 0).single() {
        dt.format("%Y-%m-%d").to_string()
    } else {
        String::new()
    }
}

pub struct HgParser;

impl HgParser {
    pub fn parse_commit_entry(
        line: &str,
        commit: &mut Commit,
        options: &VcsOptions,
    ) -> Result<bool, ()> {
        let caps = match HG_REGEX.captures(line) {
            Some(c) => c,
            None => return Err(()),
        };

        let timestamp: i64 = match caps.get(1).map_or("", |m| m.as_str()).parse() {
            Ok(ts) => ts,
            Err(_) => return Err(()),
        };

        let username = caps.get(2).map_or("", |m| m.as_str());

        if commit.files.is_empty() {
            commit.timestamp = timestamp;
            commit.username = username.to_string();
        } else if commit.timestamp != timestamp || commit.username != username {
            return Ok(false);
        }

        let action_raw = caps.get(3).map_or("", |m| m.as_str());
        let action = if action_raw.is_empty() {
            "A"
        } else {
            action_raw
        };
        let filename = caps.get(4).map_or("", |m| m.as_str());

        commit.add_file(filename, action, options);

        Ok(true)
    }
}
