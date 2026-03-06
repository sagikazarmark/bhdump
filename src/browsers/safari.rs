use super::{BrowserKind, BrowserSource, HistoryEntry};
use crate::error::Result;
use crate::timestamp;
use chrono::{DateTime, Utc};

/// Discover the Safari history database.
///
/// Safari is macOS-only and has no profiles.
pub fn discover() -> Vec<BrowserSource> {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            let db_path = home.join("Library/Safari/History.db");
            if db_path.exists() {
                return vec![BrowserSource {
                    browser: BrowserKind::Safari,
                    profile: "default".to_string(),
                    db_path,
                }];
            }
        }
        Vec::new()
    }

    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

/// Read history entries from a Safari database.
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

/// Read one entry per URL (aggregated).
fn read_aggregated(
    conn: &rusqlite::Connection,
    source: &BrowserSource,
    since: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
) -> Result<Vec<HistoryEntry>> {
    let mut sql = String::from(
        "SELECT h.url, MAX(v.title) as title, COUNT(v.id) as visit_count,
                MAX(v.visit_time) as last_visit
         FROM history_items h
         INNER JOIN history_visits v ON h.id = v.history_item
         WHERE h.url IS NOT NULL",
    );

    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(since) = since {
        sql.push_str(" AND v.visit_time >= ?");
        params.push(Box::new(datetime_to_safari(since)));
    }
    if let Some(before) = before {
        sql.push_str(" AND v.visit_time < ?");
        params.push(Box::new(datetime_to_safari(before)));
    }

    sql.push_str(" GROUP BY h.id ORDER BY last_visit DESC");

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        let url: String = row.get(0)?;
        let title: Option<String> = row.get(1)?;
        let visit_count: i64 = row.get(2)?;
        let last_visit: f64 = row.get(3)?;
        Ok((url, title, visit_count, last_visit))
    })?;

    let mut entries = Vec::new();
    for row in rows {
        let (url, title, visit_count, last_visit) = row?;
        if let Some(visit_time) = timestamp::from_safari(last_visit) {
            entries.push(HistoryEntry {
                url,
                title,
                visit_time,
                visit_count: Some(visit_count as u64),
                visit_duration_ms: None,
                browser: source.browser,
                profile: source.profile.clone(),
            });
        }
    }

    Ok(entries)
}

/// Read one entry per individual visit.
fn read_individual_visits(
    conn: &rusqlite::Connection,
    source: &BrowserSource,
    since: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
) -> Result<Vec<HistoryEntry>> {
    let mut sql = String::from(
        "SELECT h.url, v.title, v.visit_time
         FROM history_visits v
         INNER JOIN history_items h ON v.history_item = h.id
         WHERE h.url IS NOT NULL",
    );

    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(since) = since {
        sql.push_str(" AND v.visit_time >= ?");
        params.push(Box::new(datetime_to_safari(since)));
    }
    if let Some(before) = before {
        sql.push_str(" AND v.visit_time < ?");
        params.push(Box::new(datetime_to_safari(before)));
    }

    sql.push_str(" ORDER BY v.visit_time DESC");

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        let url: String = row.get(0)?;
        let title: Option<String> = row.get(1)?;
        let visit_time: f64 = row.get(2)?;
        Ok((url, title, visit_time))
    })?;

    let mut entries = Vec::new();
    for row in rows {
        let (url, title, visit_time_raw) = row?;
        if let Some(visit_time) = timestamp::from_safari(visit_time_raw) {
            entries.push(HistoryEntry {
                url,
                title,
                visit_time,
                visit_count: None,
                visit_duration_ms: None,
                browser: source.browser,
                profile: source.profile.clone(),
            });
        }
    }

    Ok(entries)
}

/// Convert a UTC DateTime to a Safari timestamp (seconds since 2001-01-01 UTC).
fn datetime_to_safari(dt: DateTime<Utc>) -> f64 {
    (dt.timestamp() - 978_307_200) as f64 + (dt.timestamp_subsec_nanos() as f64 / 1_000_000_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timestamp;
    use chrono::{TimeZone, Utc};

    #[test]
    fn test_datetime_to_safari_roundtrip() {
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap();
        let safari_ts = datetime_to_safari(dt);
        let roundtripped = timestamp::from_safari(safari_ts).unwrap();
        assert_eq!(roundtripped, dt);
    }

    #[test]
    fn test_datetime_to_safari_known_value() {
        // 2024-01-01T00:00:00Z -> seconds since 2001-01-01
        let dt = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let safari_ts = datetime_to_safari(dt);
        assert!((safari_ts - 725_760_000.0).abs() < 0.001);
    }

    #[test]
    fn test_datetime_to_safari_roundtrip_core_data_epoch() {
        // 2001-01-01T00:00:01Z (just after Core Data epoch, since 0 is rejected)
        let dt = Utc.with_ymd_and_hms(2001, 1, 1, 0, 0, 1).unwrap();
        let safari_ts = datetime_to_safari(dt);
        let roundtripped = timestamp::from_safari(safari_ts).unwrap();
        assert_eq!(roundtripped, dt);
    }
}
