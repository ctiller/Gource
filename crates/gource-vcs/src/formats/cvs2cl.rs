//! CVS cvs2cl log format parser (`cvs2cl --chrono --stdout --xml -g-q`).
//! Port of `src/formats/cvs2cl.cpp`.

use crate::commit::Commit;
use crate::options::VcsOptions;
use regex::Regex;
use std::sync::LazyLock;

static CVS2CL_XML_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^<\??xml").unwrap());
static CVS2CL_LOGENTRY_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^<entry").unwrap());
static CVS2CL_LOGENTRY_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^</entry>").unwrap());
static CVS2CL_TIMESTAMP_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})Z").unwrap());

pub fn log_command() -> String {
    "cvs2cl --chrono --stdout --xml -g-q".to_string()
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

    if !CVS2CL_LOGENTRY_START.is_match(&line) {
        if !CVS2CL_XML_TAG.is_match(&line) {
            return false;
        }

        let mut found = false;
        while get_line(&mut line) {
            if CVS2CL_LOGENTRY_START.is_match(&line) {
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
        if CVS2CL_LOGENTRY_END.is_match(&line) {
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

    let entry = doc.root_element();
    if !entry.has_tag_name("entry") {
        return false;
    }

    // Date
    let date_str = match entry
        .children()
        .find(|n| n.has_tag_name("isoDate"))
        .and_then(|n| n.text())
    {
        Some(s) => s,
        None => return false,
    };

    let caps = match CVS2CL_TIMESTAMP_REGEX.captures(date_str) {
        Some(c) => c,
        None => return false,
    };

    let year: i32 = caps
        .get(1)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    let month: u32 = caps
        .get(2)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    let day: u32 = caps
        .get(3)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    let hour: u32 = caps
        .get(4)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    let min: u32 = caps
        .get(5)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    let sec: u32 = caps
        .get(6)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);

    let naive_date = match chrono::NaiveDate::from_ymd_opt(year, month, day) {
        Some(d) => d,
        None => return false,
    };
    let naive_dt = match naive_date.and_hms_opt(hour, min, sec) {
        Some(dt) => dt,
        None => return false,
    };

    // Note that the C++ cvs2cl.cpp uses mktime with the parsed components,
    // which treats them as local time even though there is a Z in the regex!
    // Let's check cvs2cl.cpp line 128: mktime(&time_str).
    commit.timestamp = gource_core::datetime::local_timestamp(&naive_dt);

    // Author
    let author = entry
        .children()
        .find(|n| n.has_tag_name("author"))
        .and_then(|n| n.text())
        .filter(|s| !s.is_empty())
        .unwrap_or("Unknown");
    commit.username = author.to_string();

    // Files
    for file_elem in entry.children().filter(|n| n.has_tag_name("file")) {
        let state = file_elem
            .children()
            .find(|n| n.has_tag_name("cvsstate"))
            .and_then(|n| n.text())
            .filter(|s| !s.is_empty());
        let name = file_elem
            .children()
            .find(|n| n.has_tag_name("name"))
            .and_then(|n| n.text())
            .filter(|s| !s.is_empty());

        let (Some(state_text), Some(name_text)) = (state, name) else {
            continue;
        };

        let status = if state_text == "dead" { "D" } else { "M" };
        commit.add_file(name_text, status, options);
    }

    true
}
