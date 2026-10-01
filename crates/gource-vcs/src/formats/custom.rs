//! Custom format parser (`timestamp|username|action|filename[|colour]`).
//! Port of `src/formats/custom.cpp`.

use crate::commit::Commit;
use crate::commit::CommitExt;
use crate::options::VcsOptions;
use regex::Regex;
use std::sync::LazyLock;

static CUSTOM_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // C++ matches the UTF-8 byte-order mark as bytes (\xEF\xBB\xBF); lines
    // are text here, so it's U+FEFF.
    // Supports:
    // 4 fields: timestamp|username|action|filepath
    // 5 fields: timestamp|username|action|filepath|colour
    // 6 fields: timestamp|username|action|filepath|added|removed
    // 7 fields: timestamp|username|action|filepath|colour|added|removed
    Regex::new(
        r"^(?:\x{FEFF})?([^|]+)\|([^|]*)\|([ADM]?)\|([^|]+)(?:\|([^|]*))?(?:\|([^|]*))?(?:\|([^|]*))?$",
    )
    .unwrap()
});

/// Parse colour from a 6-digit hex string (e.g. `RRGGBB` or `#RRGGBB`).
pub fn parse_colour(cstr: &str) -> Option<[f32; 3]> {
    let hex = cstr.strip_prefix('#').unwrap_or(cstr);
    // ASCII check first: byte slicing below must not split a character.
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0])
}

/// C `atoll`: skip leading whitespace, take an optional sign and the longest
/// run of digits, ignore the rest. Returns 0 if there are no digits.
/// Saturates instead of overflowing (overflow is undefined behaviour in C).
pub fn atoll(s: &str) -> i64 {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r') {
        i += 1;
    }
    let negative = match bytes.get(i) {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let mut value: i64 = 0;
    while let Some(d) = bytes.get(i).filter(|b| b.is_ascii_digit()) {
        let digit = i64::from(d - b'0');
        value = if negative {
            value.saturating_mul(10).saturating_sub(digit)
        } else {
            value.saturating_mul(10).saturating_add(digit)
        };
        i += 1;
    }
    value
}

/// Port of `SDLAppSettings::parseDateTime` (the parser `--start-date` uses).
pub fn parse_date_time(datetime: &str) -> Option<i64> {
    gource_core::datetime::parse_date_time(datetime)
}

/// Custom log entry parser.
pub struct CustomParser;

impl CustomParser {
    pub fn parse_commit_entry(
        line: &str,
        commit: &mut Commit,
        options: &VcsOptions,
    ) -> Result<bool, ()> {
        let caps = match CUSTOM_REGEX.captures(line) {
            Some(c) => c,
            None => return Err(()),
        };

        let date_str = caps.get(1).map_or("", |m| m.as_str());
        // C++: `entries[0].find("-", 1) != npos` (a byte search, so it must
        // not slice the str at byte 1, which may be inside a character).
        let timestamp = if date_str
            .as_bytes()
            .get(1..)
            .is_some_and(|b| b.contains(&b'-'))
        {
            match parse_date_time(date_str) {
                Some(ts) => ts,
                None => return Err(()),
            }
        } else {
            // C++: `timestamp = atoll(...); if(!timestamp && entries[0] != "0") return false;`
            let ts = atoll(date_str);
            if ts == 0 && date_str != "0" {
                return Err(());
            }
            ts
        };

        let user_raw = caps.get(2).map_or("", |m| m.as_str());
        let username = if user_raw.is_empty() {
            "Unknown"
        } else {
            user_raw
        };

        let action_raw = caps.get(3).map_or("", |m| m.as_str());
        let action = if action_raw.is_empty() {
            "A"
        } else {
            action_raw
        };

        let file_path = caps.get(4).map_or("", |m| m.as_str());

        if commit.files.is_empty() {
            commit.timestamp = timestamp;
            commit.username = username.to_string();
            commit.is_shadow = username.starts_with("worktree:");
        } else if commit.timestamp != timestamp || commit.username != username {
            return Ok(false); // belongs to next commit
        }

        let f5 = caps.get(5).map(|m| m.as_str());
        let f6 = caps.get(6).map(|m| m.as_str());
        let f7 = caps.get(7).map(|m| m.as_str());

        let mut colour = None;
        let mut lines_added = None;
        let mut lines_removed = None;
        let mut is_binary = false;

        let parse_stat = |s: &str| -> (Option<u32>, bool) {
            if s == "-" {
                (None, true)
            } else {
                (s.parse::<u32>().ok(), false)
            }
        };

        match (f5, f6, f7) {
            // 7 fields: |colour|added|removed
            (Some(c), Some(a), Some(r)) => {
                colour = parse_colour(c);
                let (a_val, a_bin) = parse_stat(a);
                let (r_val, r_bin) = parse_stat(r);
                is_binary = a_bin && r_bin;
                lines_added = a_val;
                lines_removed = r_val;
            }
            // 6 fields: either |colour|added or |added|removed.
            // If f5 parses as colour and f6 is a number or '-', it's |colour|added (removed None).
            // But standard 6-field format without colour is |added|removed.
            (Some(a), Some(b), None) => {
                if let Some(col) = parse_colour(a) {
                    colour = Some(col);
                    let (val, bin) = parse_stat(b);
                    lines_added = val;
                    is_binary = bin;
                } else {
                    let (a_val, a_bin) = parse_stat(a);
                    let (b_val, b_bin) = parse_stat(b);
                    is_binary = a_bin && b_bin;
                    lines_added = a_val;
                    lines_removed = b_val;
                }
            }
            // 5 fields: |colour or |added
            (Some(a), None, None) => {
                if let Some(col) = parse_colour(a) {
                    colour = Some(col);
                } else {
                    let (val, bin) = parse_stat(a);
                    lines_added = val;
                    is_binary = bin;
                }
            }
            // 4 fields or any other fallback
            _ => {}
        }

        if let Some(col) = colour {
            commit.add_file_with_colour_and_stats(
                file_path,
                action,
                col,
                lines_added,
                lines_removed,
                is_binary,
                options,
            );
        } else {
            commit.add_file_with_stats(
                file_path,
                action,
                lines_added,
                lines_removed,
                is_binary,
                options,
            );
        }

        Ok(true)
    }
}
