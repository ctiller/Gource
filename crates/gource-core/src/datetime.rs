//! Date/time parsing and formatting.
//!
//! Ports `SDLAppSettings::parseDateTime` (core/settings.cpp) and the
//! `strftime`/`localtime` usage scattered through the C++ code.
//! Timestamps are Unix seconds (`i64`). Local time uses the system time zone
//! (the `TZ` environment variable is honoured by chrono).

use chrono::format::StrftimeItems;
use chrono::{
    DateTime, FixedOffset, Local, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, Offset,
    TimeDelta, TimeZone, Utc,
};
use regex::Regex;
use std::sync::OnceLock;

static TIMESTAMP_REGEX: OnceLock<Regex> = OnceLock::new();

fn get_timestamp_regex() -> &'static Regex {
    TIMESTAMP_REGEX.get_or_init(|| {
        Regex::new(
            r"^(\d{4})-(\d{2})-(\d{2})(?:[T ](\d{1,2}):(\d{2})(?::(\d{2}(?:\.\d+)?))?)?(Z| ?([+-])(\d{1,2})(?::?(\d{2}))?)?$",
        )
        .expect("valid regex")
    })
}

/// Parse the date formats accepted by `--start-date`, `--stop-date` and caption
/// files. Port of `SDLAppSettings::parseDateTime`:
///
/// ```text
/// "2010-01-02"                 local midnight
/// "2010-01-02Z"                UTC midnight
/// "2010-01-02 03:04"           local
/// "2010-01-02 03:04:05"        local
/// "2010-01-01 03:04:05+12"     explicit offset
/// "2010-01-01T03:04"
/// "2010-01-01T03:04:05"
/// "2010-01-01T03:04:05Z"
/// "2010-01-01T03:04:05+12"
/// "2010-01-01T00:04:05+5:30"
/// "2010-01-01T03:04:05.6789"   sub-seconds parsed but discarded
/// ```
///
/// The C++ regex is
/// `^(\d{4})-(\d{2})-(\d{2})(?:[T ](\d{1,2}):(\d{2})(?::(\d{2}(?:\.\d+)?))?)?(Z| ?([+-])(\d{1,2})(?::?(\d{2}))?)?$`.
/// Without a zone suffix the time is interpreted in local time (like `mktime`
/// with `tm_isdst = -1`; for ambiguous local times pick the earliest).
/// Returns `None` if the string does not match.
pub fn parse_date_time(datetime: &str) -> Option<i64> {
    parse_date_time_in(&Local, datetime)
}

/// Core parse logic generic over a `chrono::TimeZone`.
///
/// If no timezone or offset is specified in the string, `tz` is used, with
/// DST transitions handled like `mktime` (see [`local_timestamp_in`]).
pub fn parse_date_time_in<Tz: TimeZone>(tz: &Tz, datetime: &str) -> Option<i64> {
    let re = get_timestamp_regex();
    let caps = re.captures(datetime)?;

    // Group 1: Year (4 digits)
    let year: i32 = caps.get(1)?.as_str().parse().ok()?;
    // Group 2: Month (2 digits)
    let month: u32 = caps.get(2)?.as_str().parse().ok()?;
    // Group 3: Day (2 digits)
    let day: u32 = caps.get(3)?.as_str().parse().ok()?;

    let naive_date = NaiveDate::from_ymd_opt(year, month, day)?;

    // Optional: hours, minutes, seconds (groups 4, 5, 6)
    let hour: u32 = if let Some(m) = caps.get(4) {
        m.as_str().parse().ok()?
    } else {
        0
    };

    let min: u32 = if let Some(m) = caps.get(5) {
        m.as_str().parse().ok()?
    } else {
        0
    };

    let sec: u32 = if let Some(m) = caps.get(6) {
        // May contain fractional seconds like "59.123", which are truncated/discarded
        let s = m.as_str();
        let int_sec = s.split('.').next().unwrap_or(s);
        int_sec.parse().ok()?
    } else {
        0
    };

    let naive_time = NaiveTime::from_hms_opt(hour, min, sec)?;
    let naive_dt = NaiveDateTime::new(naive_date, naive_time);

    // Group 7 is (Z| ?([+-])(\d{1,2})(?::?(\d{2}))?)?
    // In C++ regex:
    // results[6] == "Z" -> Zulu time (UTC)
    // results.size() >= 9 -> results[7] is sign, results[8] is hour, results[9] is min
    if let Some(z_or_offset) = caps.get(7) {
        let z_str = z_or_offset.as_str();
        if z_str == "Z" {
            return Some(naive_dt.and_utc().timestamp());
        }
        if let (Some(sign_m), Some(tz_h_m)) = (caps.get(8), caps.get(9)) {
            let sign = sign_m.as_str();
            let tz_hour: i32 = tz_h_m.as_str().parse().ok()?;
            let tz_min: i32 = if let Some(tz_m) = caps.get(10) {
                tz_m.as_str().parse().ok()?
            } else {
                0
            };
            let mut offset_secs = tz_hour * 3600 + tz_min * 60;
            if sign == "-" {
                offset_secs = -offset_secs;
            }
            let offset = FixedOffset::east_opt(offset_secs)?;
            let dt = offset.from_local_datetime(&naive_dt).earliest()?;
            return Some(dt.timestamp());
        }
    }

    Some(local_timestamp_in(tz, &naive_dt))
}

/// Converts a wall-clock time in `tz` to a Unix timestamp the way C `mktime`
/// does with `tm_isdst = -1`, which is how the C++ code reads local times.
///
/// A time repeated when the clocks go back resolves to the earlier instant
/// (glibc does this unless its previous call landed after the transition). A
/// time skipped when the clocks go forward is read with the UTC offset in
/// effect before the jump, as glibc does: 02:30 on a spring-forward night in
/// New York becomes 03:30 EDT.
pub fn local_timestamp_in<Tz: TimeZone>(tz: &Tz, local: &NaiveDateTime) -> i64 {
    match tz.from_local_datetime(local) {
        LocalResult::Single(dt) => dt.timestamp(),
        // chrono doesn't always put the earlier instant first (its POSIX TZ
        // rule path returns standard time first), so compare.
        LocalResult::Ambiguous(a, b) => a.timestamp().min(b.timestamp()),
        LocalResult::None => {
            // A day earlier is before the transition for any UTC offset.
            let before = local
                .checked_sub_signed(TimeDelta::days(1))
                .unwrap_or(*local);
            let offset = tz.offset_from_utc_datetime(&before).fix().local_minus_utc();
            local.and_utc().timestamp() - i64::from(offset)
        }
    }
}

/// [`local_timestamp_in`] for the system time zone (honours `TZ`).
pub fn local_timestamp(local: &NaiveDateTime) -> i64 {
    local_timestamp_in(&Local, local)
}

/// Format a timestamp with a given timezone using C `strftime` format specifiers.
///
/// Falls back to emitting verbatim text on any formatting failure rather than panicking.
pub fn format_in<Tz: TimeZone>(tz: &Tz, timestamp: i64, format: &str) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let naive_dt = match DateTime::from_timestamp(timestamp, 0) {
        Some(utc) => utc.naive_utc(),
        None => return format.to_owned(),
    };
    let dt = tz.from_utc_datetime(&naive_dt);

    // Try formatting with StrftimeItems.
    // If strftime parsing or formatting fails (e.g. invalid format specifier),
    // fall back to custom verbatim-preserving formatter or returning format.
    let items = StrftimeItems::new(format);
    let mut out = String::new();
    use std::fmt::Write;
    if write!(&mut out, "{}", dt.format_with_items(items)).is_ok() {
        out
    } else {
        format.to_owned()
    }
}

/// Format a timestamp in local time using a C `strftime` style format string
/// (e.g. the default `--date-format` `"%A, %d %B, %Y %X"`).
/// Unsupported/invalid specifiers must not panic; output them verbatim.
pub fn format_local(timestamp: i64, format: &str) -> String {
    format_in(&Local, timestamp, format)
}

/// Format a timestamp in UTC using a C `strftime` style format string.
pub fn format_utc(timestamp: i64, format: &str) -> String {
    format_in(&Utc, timestamp, format)
}

/// Current Unix time in seconds.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_date_time_utc_cases() {
        // "2021-11-01" in UTC
        assert_eq!(parse_date_time_in(&Utc, "2021-11-01"), Some(1635724800));
        // "2021-11-01Z"
        assert_eq!(parse_date_time_in(&Utc, "2021-11-01Z"), Some(1635724800));
        assert_eq!(
            parse_date_time_in(&FixedOffset::east_opt(3600).unwrap(), "2021-11-01Z"),
            Some(1635724800)
        );
        // Explicit offsets
        assert_eq!(parse_date_time_in(&Utc, "2021-11-01+0"), Some(1635724800));
        assert_eq!(parse_date_time_in(&Utc, "2021-11-01+13"), Some(1635678000));
        assert_eq!(parse_date_time_in(&Utc, "2021-11-01 +13"), Some(1635678000));
        assert_eq!(
            parse_date_time_in(&Utc, "2021-11-01-08"),
            Some(1635724800 + 8 * 3600)
        );
        assert_eq!(
            parse_date_time_in(&Utc, "2021-11-01+5:30"),
            Some(1635705000)
        );
        // With hours & minutes
        assert_eq!(
            parse_date_time_in(&Utc, "2021-11-01 12:01"),
            Some(1635768060)
        );
        assert_eq!(
            parse_date_time_in(&Utc, "2021-11-01 12:01Z"),
            Some(1635768060)
        );
        assert_eq!(
            parse_date_time_in(&Utc, "2021-11-01 12:01+0"),
            Some(1635768060)
        );
        assert_eq!(
            parse_date_time_in(&Utc, "2021-11-01 12:01+13"),
            Some(1635721260)
        );
        assert_eq!(
            parse_date_time_in(&Utc, "2021-11-01 12:01 +13"),
            Some(1635721260)
        );
        assert_eq!(
            parse_date_time_in(&Utc, "2021-11-01 12:01+5:30"),
            Some(1635748260)
        );
        // With seconds
        assert_eq!(
            parse_date_time_in(&Utc, "2021-11-01 12:01:59"),
            Some(1635768119)
        );
        assert_eq!(
            parse_date_time_in(&Utc, "2021-11-01 12:01:59Z"),
            Some(1635768119)
        );
        assert_eq!(
            parse_date_time_in(&Utc, "2021-11-01 12:01:59+13"),
            Some(1635721319)
        );
        assert_eq!(
            parse_date_time_in(&Utc, "2021-11-01 12:01:59 +13"),
            Some(1635721319)
        );
        assert_eq!(
            parse_date_time_in(&Utc, "2021-11-01 12:01:59+5:30"),
            Some(1635748319)
        );
        // ISO 'T' separator
        assert_eq!(
            parse_date_time_in(&Utc, "2021-11-01T12:01:59"),
            Some(1635768119)
        );
        assert_eq!(
            parse_date_time_in(&Utc, "2021-11-01T12:01:59Z"),
            Some(1635768119)
        );
        // Subseconds discarded
        assert_eq!(
            parse_date_time_in(&Utc, "2021-11-01T12:01:59.123"),
            Some(1635768119)
        );
        assert_eq!(
            parse_date_time_in(&Utc, "2021-11-01T12:01:59.123+5:30"),
            Some(1635748319)
        );
    }

    #[test]
    fn parse_date_time_with_fixed_timezone() {
        // Auckland is UTC+13 during daylight saving in November
        let auckland_tz = FixedOffset::east_opt(13 * 3600).unwrap();
        assert_eq!(
            parse_date_time_in(&auckland_tz, "2021-11-01"),
            Some(1635678000)
        );
        assert_eq!(
            parse_date_time_in(&auckland_tz, "2021-11-01 12:01"),
            Some(1635721260)
        );
        assert_eq!(
            parse_date_time_in(&auckland_tz, "2021-11-01 12:01:59"),
            Some(1635721319)
        );
        assert_eq!(
            parse_date_time_in(&auckland_tz, "2021-11-01T12:01:59.123"),
            Some(1635721319)
        );
    }

    #[test]
    fn parse_date_time_invalid_inputs() {
        assert_eq!(parse_date_time("not a date"), None);
        assert_eq!(parse_date_time("2021-13-01"), None); // Invalid month
        assert_eq!(parse_date_time("2021-11-35"), None); // Invalid day
        assert_eq!(parse_date_time("2021-11-01 25:00"), None); // Invalid hour
        assert_eq!(parse_date_time("2021-11-01 12:65"), None); // Invalid min
        assert_eq!(parse_date_time("2021-11-01 12:00:99"), None); // Invalid sec
        // Invalid timezone offsets > 24 hours
        assert_eq!(parse_date_time("2021-11-01 12:00:00+25:00"), None);
        assert_eq!(parse_date_time("2021-11-01 12:00:00-25:00"), None);
    }

    /// US Eastern time as a POSIX rule, so the test doesn't need tzdata.
    const DST_TZ: &str = "XST5XDT,M3.2.0,M11.1.0";
    const DST_TEST: &str = "datetime::tests::local_times_across_dst_match_mktime";

    /// The expected values are what glibc `mktime` (`tm_isdst = -1`) returns
    /// for the same wall-clock times with the same `TZ`. Runs in a child
    /// process because `TZ` is process-wide.
    #[test]
    fn local_times_across_dst_match_mktime() {
        if std::env::var_os("GOURCE_DST_CHILD").is_none() {
            let exe = std::env::current_exe().expect("test binary path");
            let output = std::process::Command::new(exe)
                .args(["--exact", DST_TEST, "--nocapture", "--test-threads=1"])
                .env("GOURCE_DST_CHILD", "1")
                .env("TZ", DST_TZ)
                .output()
                .expect("run child test");
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success() && stdout.contains("DST_CHILD_OK"),
                "child failed: {stdout}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let at = |y, mo, d, h, mi| {
            NaiveDate::from_ymd_opt(y, mo, d)
                .and_then(|date| date.and_hms_opt(h, mi, 0))
                .expect("valid date")
        };
        // Summer: plain EDT.
        assert_eq!(local_timestamp(&at(2021, 6, 1, 12, 0)), 1_622_563_200);
        // Skipped by the spring-forward jump: read as EST, i.e. 03:30 EDT.
        assert_eq!(local_timestamp(&at(2021, 3, 14, 2, 30)), 1_615_707_000);
        // Repeated by the fall-back: the earlier instant, 01:30 EDT.
        assert_eq!(local_timestamp(&at(2021, 11, 7, 1, 30)), 1_636_263_000);
        assert_eq!(parse_date_time("2021-03-14 02:30"), Some(1_615_707_000));
        assert_eq!(parse_date_time("2021-11-07 01:30"), Some(1_636_263_000));
        println!("DST_CHILD_OK");
    }

    #[test]
    fn formatting_tests() {
        let ts = 1635724800; // 2021-11-01 00:00:00 UTC
        let formatted = format_utc(ts, "%Y-%m-%d %H:%M:%S");
        assert_eq!(formatted, "2021-11-01 00:00:00");

        let custom = format_utc(ts, "%A, %d %B, %Y");
        assert_eq!(custom, "Monday, 01 November, 2021");

        // Format with timezone
        let auckland_tz = FixedOffset::east_opt(13 * 3600).unwrap();
        let formatted_akl = format_in(&auckland_tz, ts, "%Y-%m-%d %H:%M:%S");
        assert_eq!(formatted_akl, "2021-11-01 13:00:00");

        // format_local does not panic
        let _ = format_local(ts, "%A, %d %B, %Y %X");

        // Invalid specifiers do not panic
        let invalid = format_utc(ts, "%Q %%%% %%");
        assert!(!invalid.is_empty());

        // Extreme timestamp does not panic
        let extreme = format_utc(i64::MAX, "%Y");
        assert!(!extreme.is_empty());
    }

    #[test]
    fn now_returns_sensible_timestamp() {
        let n = now();
        assert!(n > 1_600_000_000);
        let parsed = parse_date_time("2021-11-01 12:00:00");
        assert!(parsed.is_some());
    }

    #[test]
    fn matches_cpp_datetime_goldens() {
        let golden_data = include_str!("../tests/data/datetime_golden.txt");
        for line in golden_data.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            let date_str = parts[0];
            let should_succeed = parts[1] == "1";
            let expected_ts: i64 = parts[2].parse().unwrap();

            let parsed = parse_date_time_in(&Utc, date_str);
            if should_succeed {
                assert_eq!(
                    parsed,
                    Some(expected_ts),
                    "Failed on date_str: {}",
                    date_str
                );
            } else {
                assert_eq!(parsed, None, "Expected failure on date_str: {}", date_str);
            }
        }
    }

    #[test]
    fn common_gource_strftime_specifiers() {
        let ts = 1635768119; // 2021-11-01 12:01:59 UTC
        // %d-%b-%y
        assert_eq!(format_utc(ts, "%d-%b-%y"), "01-Nov-21");
        // %Y/%m/%d %H:%M:%S
        assert_eq!(format_utc(ts, "%Y/%m/%d %H:%M:%S"), "2021/11/01 12:01:59");
        // %x %X
        assert!(!format_utc(ts, "%x %X").is_empty());
        // %j (day of year: 305)
        assert_eq!(format_utc(ts, "%j"), "305");
        // %u (weekday 1..7: 1 for Monday)
        assert_eq!(format_utc(ts, "%u"), "1");
    }
}
