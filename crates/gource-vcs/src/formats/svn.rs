//! Subversion (svn) log format parser (XML format).
//! Port of `src/formats/svn.cpp`.

use crate::commit::Commit;
use crate::options::VcsOptions;
use regex::Regex;
use std::sync::LazyLock;

static SVN_XML_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^<\??xml").unwrap());
static SVN_LOGENTRY_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^<logentry").unwrap());
static SVN_LOGENTRY_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^</logentry>").unwrap());
static SVN_TIMESTAMP_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})").unwrap());

pub fn log_command(options: &VcsOptions) -> String {
    let start = if options.start_timestamp != 0 {
        format!("{{{}}}", format_timestamp_date(options.start_timestamp))
    } else {
        "1".to_string()
    };

    let stop = if options.stop_timestamp != 0 {
        format!("{{{}}}", format_timestamp_date(options.stop_timestamp))
    } else {
        "HEAD".to_string()
    };

    let range = format!("{}:{}", start, stop);
    format!("svn log -r {} --xml --verbose --quiet", range)
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

    if !SVN_LOGENTRY_START.is_match(&line) {
        if !SVN_XML_TAG.is_match(&line) {
            return false;
        }

        let mut found = false;
        while get_line(&mut line) {
            if SVN_LOGENTRY_START.is_match(&line) {
                found = true;
                break;
            }
        }
        if !found {
            return false;
        }
    }

    let mut xml = String::new();
    xml.push_str(&line);
    xml.push('\n');

    let mut end_found = false;
    while get_line(&mut line) {
        xml.push_str(&line);
        xml.push('\n');
        if SVN_LOGENTRY_END.is_match(&line) {
            end_found = true;
            break;
        }
    }

    if !end_found {
        return false;
    }

    let doc = match roxmltree::Document::parse(&xml) {
        Ok(d) => d,
        Err(_) => return false,
    };

    let logentry = match doc.root_element().tag_name().name() {
        "logentry" => doc.root_element(),
        _ => match doc
            .root_element()
            .children()
            .find(|n| n.has_tag_name("logentry"))
        {
            Some(node) => node,
            None => return false,
        },
    };

    // Date
    let date_str = match logentry
        .children()
        .find(|n| n.has_tag_name("date"))
        .and_then(|n| n.text())
    {
        Some(s) => s,
        None => return false,
    };

    let caps = match SVN_TIMESTAMP_REGEX.captures(date_str) {
        Some(c) => c,
        None => return false,
    };

    let year: i32 = caps.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
    let month: u32 = caps.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
    let day: u32 = caps.get(3).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
    let hour: u32 = caps.get(4).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
    let min: u32 = caps.get(5).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
    let sec: u32 = caps.get(6).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);

    let naive_date = match chrono::NaiveDate::from_ymd_opt(year, month, day) {
        Some(d) => d,
        None => return false,
    };
    let naive_dt = match naive_date.and_hms_opt(hour, min, sec) {
        Some(dt) => dt,
        None => return false,
    };

    // SVN date is in UTC
    commit.timestamp = naive_dt.and_utc().timestamp();

    // Author
    if let Some(author_node) = logentry.children().find(|n| n.has_tag_name("author")) {
        let text = author_node.text().unwrap_or("");
        commit.username = if text.is_empty() {
            "Unknown".to_string()
        } else {
            text.to_string()
        };
    }

    // Paths
    if let Some(paths_elem) = logentry.children().find(|n| n.has_tag_name("paths")) {
        for path_elem in paths_elem.children().filter(|n| n.has_tag_name("path")) {
            let action = match path_elem.attribute("action") {
                Some(a) => a,
                None => continue,
            };
            let kind = path_elem.attribute("kind");

            let mut is_dir = false;
            if let Some(k) = kind
                && k == "dir"
            {
                if action != "D" {
                    continue;
                }
                is_dir = true;
            }

            let file_text = match path_elem.text() {
                Some(t) => t,
                None => continue,
            };
            if file_text.is_empty() || action.is_empty() {
                continue;
            }

            let mut path_str = file_text.to_string();
            if is_dir && !path_str.ends_with('/') {
                path_str.push('/');
            }

            commit.add_file(&path_str, action, options);
        }
    }

    true
}
