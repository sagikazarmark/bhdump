use chrono::{DateTime, Duration, TimeZone, Utc};

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
/// Supports:
/// - Relative: "7d" (7 days ago), "2w" (2 weeks), "3mo" (3 months), "1y" (1 year)
/// - ISO 8601: "2025-01-01", "2025-01-01T00:00:00Z"
pub fn parse_user_datetime(input: &str) -> crate::error::Result<DateTime<Utc>> {
    let input = input.trim();

    // Try relative formats first
    if let Some(dt) = parse_relative(input) {
        return Ok(dt);
    }

    // Try ISO 8601 with time
    if let Ok(dt) = DateTime::parse_from_rfc3339(input) {
        return Ok(dt.with_timezone(&Utc));
    }

    // Try ISO 8601 with time but without timezone (assume UTC)
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(input, "%Y-%m-%dT%H:%M:%S") {
        return Ok(dt.and_utc());
    }

    // Try date-only (assume start of day UTC)
    if let Ok(date) = chrono::NaiveDate::parse_from_str(input, "%Y-%m-%d") {
        let dt = date.and_hms_opt(0, 0, 0).expect("midnight is always valid");
        return Ok(dt.and_utc());
    }

    Err(crate::error::Error::InvalidDateTime(format!(
        "cannot parse '{input}' -- expected ISO 8601 date, or relative like '7d', '2w', '3mo', '1y'"
    )))
}

fn parse_relative(input: &str) -> Option<DateTime<Utc>> {
    let now = Utc::now();

    // Match patterns like "7d", "2w", "3mo", "1y"
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_parse_invalid() {
        assert!(parse_user_datetime("not-a-date").is_err());
    }
}
