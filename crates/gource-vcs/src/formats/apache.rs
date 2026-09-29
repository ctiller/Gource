//! Apache combined log format parser.
//! Port of `src/formats/apache.cpp`.

use crate::commit::Commit;
use crate::options::VcsOptions;
use regex::Regex;
use std::sync::LazyLock;

static APACHE_ENTRY_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:[^ ]+ )?([^ ]+) +[^ ]+ +([^ ]+) +\[(.*?)\] +(.*)$").unwrap());
static APACHE_ENTRY_DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d+)/([A-Za-z]+)/(\d+):(\d+):(\d+):(\d+) ([+-])(\d+)").unwrap());
static APACHE_ENTRY_REQUEST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""([^ ]+) +([^ ]+) +([^ ]+)" +([^ ]+) +([^\s+]+)(.*)"#).unwrap());

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

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

    let start_caps = match APACHE_ENTRY_START.captures(&line) {
        Some(c) => c,
        None => return false,
    };

    commit.username = start_caps.get(1).map_or("", |m| m.as_str()).to_string();
    let datestr = start_caps.get(3).map_or("", |m| m.as_str());
    let request_str = start_caps.get(4).map_or("", |m| m.as_str());

    let date_caps = match APACHE_ENTRY_DATE.captures(datestr) {
        Some(c) => c,
        None => return false,
    };

    let day: u32 = date_caps
        .get(1)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    let month_str = date_caps.get(2).map_or("", |m| m.as_str());
    let year: i32 = date_caps
        .get(3)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    let hour: u32 = date_caps
        .get(4)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    let min: u32 = date_caps
        .get(5)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    let sec: u32 = date_caps
        .get(6)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);

    let month = match MONTHS.iter().position(|&m| m == month_str) {
        Some(idx) => (idx + 1) as u32,
        None => return false,
    };

    let naive_date = match chrono::NaiveDate::from_ymd_opt(year, month, day) {
        Some(d) => d,
        None => return false,
    };
    let naive_dt = match naive_date.and_hms_opt(hour, min, sec) {
        Some(dt) => dt,
        None => return false,
    };

    // The C++ code uses mktime with time_str (which evaluates as local time)
    commit.timestamp = gource_core::datetime::local_timestamp(&naive_dt);

    let req_caps = match APACHE_ENTRY_REQUEST.captures(request_str) {
        Some(c) => c,
        None => return false,
    };

    let raw_file = req_caps.get(2).map_or("", |m| m.as_str());
    let mut file = match raw_file.rfind('?') {
        Some(pos) => &raw_file[..pos],
        None => raw_file,
    };

    if file.is_empty() {
        file = "/";
    }

    let mut file_str = file.to_string();
    if file_str.ends_with('/') {
        file_str.push_str("index.html");
    }

    commit.add_file(&file_str, "A", options);

    true
}
