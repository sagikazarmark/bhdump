use super::{Browser, BrowserSource, HistoryEntry};
use crate::error::Result;
use crate::timestamp;
use chrono::{DateTime, Utc};
use std::path::PathBuf;

/// Discover all Chromium-based browser profiles for the given browser kind.
pub fn discover_profiles(kind: Browser) -> Vec<BrowserSource> {
    let Some(base_dir) = browser_data_dir(kind) else {
        return Vec::new();
    };

    if !base_dir.exists() {
        return Vec::new();
    }

    let mut sources = Vec::new();

    // Check for profile directories: Default, Profile 1, Profile 2, ...
    let profile_patterns = ["Default", "Profile "];

    if let Ok(entries) = std::fs::read_dir(&base_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_profile = profile_patterns
                .iter()
                .any(|p| name == *p || name.starts_with(p));

            if is_profile {
                let history_file = entry.path().join("History");
                if history_file.exists() {
                    sources.push(BrowserSource {
                        browser: kind,
                        profile: name,
                        db_path: history_file,
                    });
                }
            }
        }
    }

    sources
}

/// Read history entries from a Chromium-family database.
pub fn read_history(
    conn: &rusqlite::Connection,
    source: &BrowserSource,
    since: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
    individual_visits: bool,
) -> Result<Vec<HistoryEntry>> {
    if individual_visits {
        read_individual_visits(conn, source, since, before)
    } else {
        read_aggregated(conn, source, since, before)
    }
}

/// Read one entry per URL (aggregated from the `urls` table).
fn read_aggregated(
    conn: &rusqlite::Connection,
    source: &BrowserSource,
    since: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
) -> Result<Vec<HistoryEntry>> {
    let mut sql = String::from(
        "SELECT url, title, visit_count, last_visit_time
         FROM urls
         WHERE url IS NOT NULL AND hidden = 0",
    );

    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(since) = since {
        sql.push_str(" AND last_visit_time >= ?");
        params.push(Box::new(datetime_to_webkit(since)));
    }
    if let Some(before) = before {
        sql.push_str(" AND last_visit_time < ?");
        params.push(Box::new(datetime_to_webkit(before)));
    }

    sql.push_str(" ORDER BY last_visit_time DESC");

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        let url: String = row.get(0)?;
        let title: Option<String> = row.get(1)?;
        let visit_count: Option<i64> = row.get(2)?;
        let last_visit_time: i64 = row.get(3)?;
        Ok((url, title, visit_count, last_visit_time))
    })?;

    let mut entries = Vec::new();
    for row in rows {
        let (url, title, visit_count, last_visit_time) = row?;
        if let Some(visit_time) = timestamp::from_webkit(last_visit_time) {
            entries.push(HistoryEntry {
                url,
                title,
                visit_time,
                visit_count: visit_count.map(|c| c as u64),
                visit_duration_ms: None,
                browser: source.browser,
                profile: source.profile.clone(),
            });
        }
    }

    Ok(entries)
}

/// Read one entry per individual visit (from the `visits` JOIN `urls` tables).
fn read_individual_visits(
    conn: &rusqlite::Connection,
    source: &BrowserSource,
    since: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
) -> Result<Vec<HistoryEntry>> {
    let mut sql = String::from(
        "SELECT u.url, u.title, v.visit_time, v.visit_duration
         FROM visits v
         INNER JOIN urls u ON v.url = u.id
         WHERE u.url IS NOT NULL AND u.hidden = 0",
    );

    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(since) = since {
        sql.push_str(" AND v.visit_time >= ?");
        params.push(Box::new(datetime_to_webkit(since)));
    }
    if let Some(before) = before {
        sql.push_str(" AND v.visit_time < ?");
        params.push(Box::new(datetime_to_webkit(before)));
    }

    sql.push_str(" ORDER BY v.visit_time DESC");

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        let url: String = row.get(0)?;
        let title: Option<String> = row.get(1)?;
        let visit_time: i64 = row.get(2)?;
        let visit_duration: Option<i64> = row.get(3)?;
        Ok((url, title, visit_time, visit_duration))
    })?;

    let mut entries = Vec::new();
    for row in rows {
        let (url, title, visit_time_raw, visit_duration) = row?;
        if let Some(visit_time) = timestamp::from_webkit(visit_time_raw) {
            let duration_ms = visit_duration.filter(|&d| d > 0).map(|d| (d / 1000) as u64); // microseconds to milliseconds
            entries.push(HistoryEntry {
                url,
                title,
                visit_time,
                visit_count: None, // individual visit, no count
                visit_duration_ms: duration_ms,
                browser: source.browser,
                profile: source.profile.clone(),
            });
        }
    }

    Ok(entries)
}

/// Convert a UTC DateTime to a WebKit timestamp (microseconds since 1601-01-01).
fn datetime_to_webkit(dt: DateTime<Utc>) -> i64 {
    let unix_seconds = dt.timestamp();
    let sub_micros = dt.timestamp_subsec_micros() as i64;
    (unix_seconds + 11_644_473_600) * 1_000_000 + sub_micros
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timestamp;
    use chrono::{TimeZone, Utc};

    #[test]
    fn test_datetime_to_webkit_roundtrip() {
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap();
        let webkit_ts = datetime_to_webkit(dt);
        let roundtripped = timestamp::from_webkit(webkit_ts).unwrap();
        assert_eq!(roundtripped, dt);
    }

    #[test]
    fn test_datetime_to_webkit_roundtrip_epoch() {
        // Unix epoch
        let dt = Utc.with_ymd_and_hms(1970, 1, 1, 0, 0, 0).unwrap();
        let webkit_ts = datetime_to_webkit(dt);
        let roundtripped = timestamp::from_webkit(webkit_ts).unwrap();
        assert_eq!(roundtripped, dt);
    }

    #[test]
    fn test_datetime_to_webkit_known_value() {
        // 2024-01-01T00:00:00Z should produce the known WebKit timestamp
        let dt = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let webkit_ts = datetime_to_webkit(dt);
        assert_eq!(webkit_ts, 13_348_540_800_000_000);
    }
}

/// Get the base data directory for a Chromium-based browser on the current platform.
fn browser_data_dir(kind: Browser) -> Option<PathBuf> {
    let home = dirs::home_dir()?;

    #[cfg(target_os = "macos")]
    {
        let base = home.join("Library/Application Support");
        match kind {
            Browser::Chrome => Some(base.join("Google/Chrome")),
            Browser::Chromium => Some(base.join("Chromium")),
            Browser::Edge => Some(base.join("Microsoft Edge")),
            Browser::Brave => Some(base.join("BraveSoftware/Brave-Browser")),
            Browser::Vivaldi => Some(base.join("Vivaldi")),
            Browser::Opera => Some(base.join("com.operasoftware.Opera")),
            Browser::Arc => Some(base.join("Arc/User Data")),
            _ => None,
        }
    }

    #[cfg(target_os = "linux")]
    {
        let config = home.join(".config");
        match kind {
            Browser::Chrome => Some(config.join("google-chrome")),
            Browser::Chromium => Some(config.join("chromium")),
            Browser::Edge => Some(config.join("microsoft-edge")),
            Browser::Brave => Some(config.join("BraveSoftware/Brave-Browser")),
            Browser::Vivaldi => Some(config.join("vivaldi")),
            Browser::Opera => Some(config.join("opera")),
            _ => None, // Arc not available on Linux
        }
    }

    #[cfg(target_os = "windows")]
    {
        let local = dirs::data_local_dir()?;
        let roaming = dirs::data_dir()?;
        match kind {
            Browser::Chrome => Some(local.join("Google/Chrome/User Data")),
            Browser::Chromium => Some(local.join("chromium/User Data")),
            Browser::Edge => Some(local.join("Microsoft/Edge/User Data")),
            Browser::Brave => Some(local.join("BraveSoftware/Brave-Browser/User Data")),
            Browser::Vivaldi => Some(local.join("Vivaldi/User Data")),
            Browser::Opera => Some(roaming.join("Opera Software/Opera Stable")),
            _ => None,
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}
