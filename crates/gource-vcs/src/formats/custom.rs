//! Custom format parser (`timestamp|username|action|filename[|colour]`).
//! Port of `src/formats/custom.cpp`.

use crate::commit::Commit;
use crate::options::VcsOptions;
use glam::Vec3;
use regex::Regex;
use std::sync::LazyLock;

static CUSTOM_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // C++ matches the UTF-8 byte-order mark as bytes (\xEF\xBB\xBF); lines
    // are text here, so it's U+FEFF.
    Regex::new(r"^(?:\x{FEFF})?([^|]+)\|([^|]*)\|([ADM]?)\|([^|]+)(?:\|#?([a-fA-F0-9]{6}))?")
        .unwrap()
});

/// Parse colour from a 6-digit hex string (e.g. `RRGGBB` or `#RRGGBB`).
pub fn parse_colour(cstr: &str) -> Option<Vec3> {
    let hex = cstr.strip_prefix('#').unwrap_or(cstr);
    // ASCII check first: byte slicing below must not split a character.
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Vec3::new(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
    ))
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
        } else if commit.timestamp != timestamp || commit.username != username {
            return Ok(false); // belongs to next commit
        }

        let colour = caps.get(5).and_then(|m| parse_colour(m.as_str()));

        if let Some(col) = colour {
            commit.add_file_with_colour(file_path, action, col, options);
        } else {
            commit.add_file(file_path, action, options);
        }

        Ok(true)
    }
}
