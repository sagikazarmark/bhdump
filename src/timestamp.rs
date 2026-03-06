use chrono::{DateTime, Duration, TimeZone, Utc};
use interim::Dialect;

/// Seconds between 1601-01-01 and 1970-01-01 (Unix epoch).
const WEBKIT_EPOCH_OFFSET: i64 = 11_644_473_600;

/// Seconds between 1970-01-01 and 2001-01-01 (Core Data epoch).
const CORE_DATA_EPOCH_OFFSET: i64 = 978_307_200;

/// Convert a Chromium/WebKit timestamp (microseconds since 1601-01-01 UTC)
/// to a UTC DateTime.
///
/// Returns `None` if the timestamp is 0 or produces an out-of-range date.
pub fn from_webkit(microseconds: i64) -> Option<DateTime<Utc>> {
    if microseconds <= 0 {
        return None;
    }
    let unix_seconds = (microseconds / 1_000_000) - WEBKIT_EPOCH_OFFSET;
    let sub_micros = (microseconds % 1_000_000) as u32 * 1000; // to nanoseconds
    Utc.timestamp_opt(unix_seconds, sub_micros).single()
}

/// Convert a Firefox/Mozilla timestamp (microseconds since 1970-01-01 UTC)
/// to a UTC DateTime.
///
/// Returns `None` if the timestamp is 0 or produces an out-of-range date.
pub fn from_firefox(microseconds: i64) -> Option<DateTime<Utc>> {
    if microseconds <= 0 {
        return None;
    }
    let unix_seconds = microseconds / 1_000_000;
    let sub_micros = (microseconds % 1_000_000) as u32 * 1000; // to nanoseconds
    Utc.timestamp_opt(unix_seconds, sub_micros).single()
}

/// Convert a Safari/Core Data timestamp (seconds as f64 since 2001-01-01 UTC)
/// to a UTC DateTime.
///
/// Returns `None` if the timestamp is 0 or produces an out-of-range date.
pub fn from_safari(seconds: f64) -> Option<DateTime<Utc>> {
    if seconds <= 0.0 {
        return None;
    }
    let unix_seconds = seconds as i64 + CORE_DATA_EPOCH_OFFSET;
    let sub_nanos = ((seconds.fract()) * 1_000_000_000.0) as u32;
    Utc.timestamp_opt(unix_seconds, sub_nanos).single()
}

/// Parse a user-supplied date string into a UTC DateTime.
///
/// Tries the following strategies in order:
///
/// 1. **Named keywords** — `today`, `yesterday`, `last-week`, `last-month`,
///    `last-year` resolve to midnight UTC.
/// 2. **Compact shorthands** — `7d`, `2w`, `3mo`, `1y`, `1h` (subtracted
///    from now).
/// 3. **Natural language** via [`interim`] — handles `"last friday"`,
///    `"3 days ago"`, `"friday 8pm"`, ISO 8601, and month-name dates
///    like `"April 1, 2024"`.
pub fn parse_user_datetime(input: &str) -> crate::error::Result<DateTime<Utc>> {
    let input = input.trim();

    // 1. Named keywords (resolve to midnight, which is what users expect)
    if let Some(dt) = parse_named(input) {
        return Ok(dt);
    }

    // 2. Compact shorthands (7d, 2w, 3mo, 1y, 1h)
    if let Some(dt) = parse_compact(input) {
        return Ok(dt);
    }

    // 3. Natural language / ISO via interim
    match interim::parse_date_string(input, Utc::now(), Dialect::Us) {
        Ok(dt) => Ok(dt),
        Err(_) => Err(crate::error::Error::InvalidDateTime(format!(
            "cannot parse '{input}' -- try ISO 8601, relative (7d, 2w), \
             or natural language (last friday, 3 days ago)"
        ))),
    }
}

/// Named keywords that resolve to midnight UTC.
fn parse_named(input: &str) -> Option<DateTime<Utc>> {
    match input.to_lowercase().as_str() {
        "today" => Some(today_midnight()),
        "yesterday" => today_midnight().checked_sub_signed(Duration::days(1)),
        "last-week" | "last week" => today_midnight().checked_sub_signed(Duration::weeks(1)),
        "last-month" | "last month" => today_midnight().checked_sub_signed(Duration::days(30)),
        "last-year" | "last year" => today_midnight().checked_sub_signed(Duration::days(365)),
        _ => None,
    }
}

/// Compact shorthands: `7d`, `2w`, `3mo`, `1y`, `1h` (subtracted from now).
///
/// These are terse forms that `interim` doesn't handle — it expects full
/// words like `"7 days"` rather than `"7d"`.
fn parse_compact(input: &str) -> Option<DateTime<Utc>> {
    let now = Utc::now();

    // "mo" must be checked before single-char suffixes
    if let Some(num_str) = input.strip_suffix("mo") {
        let n: i64 = num_str.parse().ok()?;
        return now.checked_sub_signed(Duration::days(n * 30));
    }
    if let Some(num_str) = input.strip_suffix('y') {
        let n: i64 = num_str.parse().ok()?;
        return now.checked_sub_signed(Duration::days(n * 365));
    }
    if let Some(num_str) = input.strip_suffix('w') {
        let n: i64 = num_str.parse().ok()?;
        return now.checked_sub_signed(Duration::weeks(n));
    }
    if let Some(num_str) = input.strip_suffix('d') {
        let n: i64 = num_str.parse().ok()?;
        return now.checked_sub_signed(Duration::days(n));
    }
    if let Some(num_str) = input.strip_suffix('h') {
        let n: i64 = num_str.parse().ok()?;
        return now.checked_sub_signed(Duration::hours(n));
    }

    None
}

/// Returns midnight (00:00:00 UTC) of the current date.
fn today_midnight() -> DateTime<Utc> {
    Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight is always valid")
        .and_utc()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_webkit_known_value() {
        // 2024-01-01T00:00:00Z in WebKit timestamp
        // Unix: 1704067200, WebKit offset: 11644473600
        // WebKit seconds: 1704067200 + 11644473600 = 13348540800
        // WebKit microseconds: 13348540800 * 1_000_000
        let webkit_ts = 13_348_540_800_000_000i64;
        let dt = from_webkit(webkit_ts).unwrap();
        assert_eq!(dt.to_rfc3339(), "2024-01-01T00:00:00+00:00");
    }

    #[test]
    fn test_webkit_zero() {
        assert!(from_webkit(0).is_none());
    }

    #[test]
    fn test_firefox_known_value() {
        // 2024-01-01T00:00:00Z in Firefox timestamp (microseconds since Unix epoch)
        let firefox_ts = 1_704_067_200_000_000i64;
        let dt = from_firefox(firefox_ts).unwrap();
        assert_eq!(dt.to_rfc3339(), "2024-01-01T00:00:00+00:00");
    }

    #[test]
    fn test_firefox_zero() {
        assert!(from_firefox(0).is_none());
    }

    #[test]
    fn test_safari_known_value() {
        // 2024-01-01T00:00:00Z in Safari timestamp
        // Unix: 1704067200, Core Data offset: 978307200
        // Safari seconds: 1704067200 - 978307200 = 725760000.0
        let safari_ts = 725_760_000.0f64;
        let dt = from_safari(safari_ts).unwrap();
        assert_eq!(dt.to_rfc3339(), "2024-01-01T00:00:00+00:00");
    }

    #[test]
    fn test_safari_zero() {
        assert!(from_safari(0.0).is_none());
    }

    #[test]
    fn test_parse_iso_date() {
        let dt = parse_user_datetime("2024-01-15").unwrap();
        assert_eq!(dt.to_rfc3339(), "2024-01-15T00:00:00+00:00");
    }

    #[test]
    fn test_parse_iso_datetime() {
        let dt = parse_user_datetime("2024-01-15T10:30:00Z").unwrap();
        assert_eq!(dt.to_rfc3339(), "2024-01-15T10:30:00+00:00");
    }

    #[test]
    fn test_parse_relative_days() {
        let dt = parse_user_datetime("7d").unwrap();
        let now = Utc::now();
        let diff = now - dt;
        // Should be approximately 7 days (within a second)
        assert!((diff.num_seconds() - 7 * 86400).abs() < 2);
    }

    #[test]
    fn test_parse_relative_weeks() {
        let dt = parse_user_datetime("2w").unwrap();
        let now = Utc::now();
        let diff = now - dt;
        assert!((diff.num_seconds() - 14 * 86400).abs() < 2);
    }

    #[test]
    fn test_parse_relative_months() {
        let dt = parse_user_datetime("3mo").unwrap();
        let now = Utc::now();
        let diff = now - dt;
        // 3 months ≈ 90 days (3 * 30)
        assert!((diff.num_seconds() - 90 * 86400).abs() < 2);
    }

    #[test]
    fn test_parse_relative_years() {
        let dt = parse_user_datetime("1y").unwrap();
        let now = Utc::now();
        let diff = now - dt;
        // 1 year ≈ 365 days
        assert!((diff.num_seconds() - 365 * 86400).abs() < 2);
    }

    #[test]
    fn test_parse_relative_hours() {
        let dt = parse_user_datetime("12h").unwrap();
        let now = Utc::now();
        let diff = now - dt;
        assert!((diff.num_seconds() - 12 * 3600).abs() < 2);
    }

    #[test]
    fn test_parse_invalid() {
        assert!(parse_user_datetime("not-a-date").is_err());
    }

    #[test]
    fn test_parse_today() {
        let dt = parse_user_datetime("today").unwrap();
        let midnight = today_midnight();
        assert_eq!(dt, midnight);
    }

    #[test]
    fn test_parse_yesterday() {
        let dt = parse_user_datetime("yesterday").unwrap();
        let expected = today_midnight() - Duration::days(1);
        assert_eq!(dt, expected);
    }

    #[test]
    fn test_parse_last_week() {
        let dt = parse_user_datetime("last-week").unwrap();
        let expected = today_midnight() - Duration::weeks(1);
        assert_eq!(dt, expected);
    }

    #[test]
    fn test_parse_last_month() {
        let dt = parse_user_datetime("last-month").unwrap();
        let expected = today_midnight() - Duration::days(30);
        assert_eq!(dt, expected);
    }

    #[test]
    fn test_parse_last_year() {
        let dt = parse_user_datetime("last-year").unwrap();
        let expected = today_midnight() - Duration::days(365);
        assert_eq!(dt, expected);
    }

    // ------------------------------------------------------------------
    // Natural language via interim
    // ------------------------------------------------------------------

    #[test]
    fn test_parse_natural_days_ago() {
        let dt = parse_user_datetime("3 days ago").unwrap();
        let now = Utc::now();
        let diff = now - dt;
        assert!((diff.num_seconds() - 3 * 86400).abs() < 2);
    }

    #[test]
    fn test_parse_natural_hours() {
        let dt = parse_user_datetime("2 hours ago").unwrap();
        let now = Utc::now();
        let diff = now - dt;
        assert!((diff.num_seconds() - 2 * 3600).abs() < 2);
    }

    #[test]
    fn test_parse_natural_last_friday() {
        // Should parse without error and return a date in the past
        let dt = parse_user_datetime("last friday").unwrap();
        assert!(dt < Utc::now());
    }

    #[test]
    fn test_parse_natural_month_name() {
        // "April 1, 2024" should parse to that date
        let dt = parse_user_datetime("April 1, 2024").unwrap();
        assert_eq!(dt.month(), 4);
        assert_eq!(dt.day(), 1);
        assert_eq!(dt.year(), 2024);
    }

    #[test]
    fn test_parse_natural_iso_still_works() {
        // ISO 8601 should still work via interim fallback
        let dt = parse_user_datetime("2024-06-15").unwrap();
        assert_eq!(dt.to_rfc3339(), "2024-06-15T00:00:00+00:00");
    }

    #[test]
    fn test_parse_natural_iso_datetime_still_works() {
        let dt = parse_user_datetime("2024-01-15T10:30:00Z").unwrap();
        assert_eq!(dt.to_rfc3339(), "2024-01-15T10:30:00+00:00");
    }
}
