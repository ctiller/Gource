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

static ISO_DATE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d{4})-(\d{2})-(\d{2})(?:[T ](\d{1,2}):(\d{2})(?::(\d{2}(?:\.\d+)?))?)?(Z| ?([+-])(\d{1,2})(?::?(\d{2}))?)?$").unwrap()
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

/// Port of `SDLAppSettings::parseDateTime`.
pub fn parse_date_time(datetime: &str) -> Option<i64> {
    let caps = ISO_DATE_REGEX.captures(datetime)?;
    let year: i32 = caps.get(1)?.as_str().parse().ok()?;
    let month: u32 = caps.get(2)?.as_str().parse().ok()?;
    let day: u32 = caps.get(3)?.as_str().parse().ok()?;

    let hour: u32 = match caps.get(4) {
        Some(m) if !m.as_str().is_empty() => m.as_str().parse().ok()?,
        _ => 0,
    };
    let min: u32 = match caps.get(5) {
        Some(m) if !m.as_str().is_empty() => m.as_str().parse().ok()?,
        _ => 0,
    };
    let sec: u32 = match caps.get(6) {
        Some(m) if !m.as_str().is_empty() => {
            let s_str = m.as_str();
            let sec_part = s_str.split('.').next().unwrap_or(s_str);
            sec_part.parse().ok()?
        }
        _ => 0,
    };

    let naive_date = chrono::NaiveDate::from_ymd_opt(year, month, day)?;
    let naive_time = chrono::NaiveTime::from_hms_opt(hour, min, sec)?;
    let naive_dt = chrono::NaiveDateTime::new(naive_date, naive_time);

    if let Some(z_match) = caps.get(7) {
        let z_str = z_match.as_str().trim_start();
        if z_str == "Z" {
            return Some(naive_dt.and_utc().timestamp());
        }
    }

    if let Some(sign_match) = caps.get(8) {
        let sign = sign_match.as_str();
        let tz_hour: i32 = caps.get(9).map_or(Ok(0), |m| m.as_str().parse()).ok()?;
        let tz_min: i32 = caps.get(10).map_or(Ok(0), |m| m.as_str().parse()).ok()?;
        let mut total_offset_secs = tz_hour * 3600 + tz_min * 60;
        if sign == "-" {
            total_offset_secs = -total_offset_secs;
        }
        let offset = chrono::FixedOffset::east_opt(total_offset_secs)?;
        let dt = naive_dt.and_local_timezone(offset).single()?;
        return Some(dt.timestamp());
    }

    // Local time
    use chrono::TimeZone;
    match chrono::Local.from_local_datetime(&naive_dt) {
        chrono::LocalResult::Single(dt) => Some(dt.timestamp()),
        chrono::LocalResult::Ambiguous(dt1, _) => Some(dt1.timestamp()),
        chrono::LocalResult::None => None,
    }
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
