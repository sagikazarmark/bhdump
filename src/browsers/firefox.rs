use super::{BrowserKind, BrowserSource, HistoryEntry};
use crate::error::Result;
use crate::timestamp;
use chrono::{DateTime, Utc};
use std::path::PathBuf;

/// Discover all Firefox-family browser profiles for the given browser kind.
pub fn discover_profiles(kind: BrowserKind) -> Vec<BrowserSource> {
    let Some(profiles_dir) = browser_profiles_dir(kind) else {
        return Vec::new();
    };

    if !profiles_dir.exists() {
        return Vec::new();
    }

    let mut sources = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&profiles_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let places_file = entry.path().join("places.sqlite");
                if places_file.exists() {
                    let profile_name = entry.file_name().to_string_lossy().to_string();
                    sources.push(BrowserSource {
                        browser: kind,
                        profile: profile_name,
                        db_path: places_file,
                    });
                }
            }
        }
    }

    sources
}

/// Read history entries from a Firefox-family database.
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
        "SELECT p.url, p.title, p.visit_count, MAX(v.visit_date) as last_visit
         FROM moz_places p
         INNER JOIN moz_historyvisits v ON p.id = v.place_id
         WHERE p.url IS NOT NULL AND p.hidden = 0",
    );

    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(since) = since {
        sql.push_str(" AND v.visit_date >= ?");
        params.push(Box::new(datetime_to_firefox(since)));
    }
    if let Some(before) = before {
        sql.push_str(" AND v.visit_date < ?");
        params.push(Box::new(datetime_to_firefox(before)));
    }

    sql.push_str(" GROUP BY p.id ORDER BY last_visit DESC");

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        let url: String = row.get(0)?;
        let title: Option<String> = row.get(1)?;
        let visit_count: Option<i64> = row.get(2)?;
        let last_visit: Option<i64> = row.get(3)?;
        Ok((url, title, visit_count, last_visit))
    })?;

    let mut entries = Vec::new();
    for row in rows {
        let (url, title, visit_count, last_visit) = row?;
        if let Some(last_visit) = last_visit
            && let Some(visit_time) = timestamp::from_firefox(last_visit)
        {
            entries.push(HistoryEntry {
                url,
                title,
                visit_time,
                visit_count: visit_count.map(|c| c as u64),
                visit_duration_ms: None, // Firefox doesn't track visit duration
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
        "SELECT p.url, p.title, v.visit_date
         FROM moz_historyvisits v
         INNER JOIN moz_places p ON v.place_id = p.id
         WHERE p.url IS NOT NULL AND p.hidden = 0",
    );

    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(since) = since {
        sql.push_str(" AND v.visit_date >= ?");
        params.push(Box::new(datetime_to_firefox(since)));
    }
    if let Some(before) = before {
        sql.push_str(" AND v.visit_date < ?");
        params.push(Box::new(datetime_to_firefox(before)));
    }

    sql.push_str(" ORDER BY v.visit_date DESC");

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        let url: String = row.get(0)?;
        let title: Option<String> = row.get(1)?;
        let visit_date: i64 = row.get(2)?;
        Ok((url, title, visit_date))
    })?;

    let mut entries = Vec::new();
    for row in rows {
        let (url, title, visit_date) = row?;
        if let Some(visit_time) = timestamp::from_firefox(visit_date) {
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

/// Convert a UTC DateTime to a Firefox timestamp (microseconds since Unix epoch).
fn datetime_to_firefox(dt: DateTime<Utc>) -> i64 {
    dt.timestamp() * 1_000_000 + dt.timestamp_subsec_micros() as i64
}

/// Get the profiles directory for a Firefox-family browser on the current platform.
fn browser_profiles_dir(kind: BrowserKind) -> Option<PathBuf> {
    let home = dirs::home_dir()?;

    #[cfg(target_os = "macos")]
    {
        let base = home.join("Library/Application Support");
        match kind {
            BrowserKind::Firefox => Some(base.join("Firefox/Profiles")),
            BrowserKind::LibreWolf => Some(base.join("LibreWolf/Profiles")),
            BrowserKind::Zen => Some(base.join("zen/Profiles")),
            _ => None,
        }
    }

    #[cfg(target_os = "linux")]
    {
        match kind {
            BrowserKind::Firefox => Some(home.join(".mozilla/firefox")),
            BrowserKind::LibreWolf => Some(home.join(".librewolf")),
            BrowserKind::Zen => Some(home.join(".zen")),
            _ => None,
        }
    }

    #[cfg(target_os = "windows")]
    {
        let roaming = dirs::data_dir()?;
        match kind {
            BrowserKind::Firefox => Some(roaming.join("Mozilla/Firefox/Profiles")),
            BrowserKind::LibreWolf => Some(roaming.join("librewolf/Profiles")),
            BrowserKind::Zen => Some(roaming.join("zen/Profiles")),
            _ => None,
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}
