pub mod chromium;
pub mod firefox;
pub mod safari;

use crate::error::{Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

/// Identifies which browser produced a history entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Browser {
    Chrome,
    Chromium,
    Edge,
    Brave,
    Vivaldi,
    Opera,
    Arc,
    Firefox,
    #[serde(rename = "librewolf")]
    LibreWolf,
    Zen,
    Safari,
}

impl Browser {
    /// All known browser kinds.
    pub const ALL: &[Browser] = &[
        Browser::Chrome,
        Browser::Chromium,
        Browser::Edge,
        Browser::Brave,
        Browser::Vivaldi,
        Browser::Opera,
        Browser::Arc,
        Browser::Firefox,
        Browser::LibreWolf,
        Browser::Zen,
        Browser::Safari,
    ];

    /// The canonical lowercase name used in CLI flags and output.
    pub fn as_str(self) -> &'static str {
        match self {
            Browser::Chrome => "chrome",
            Browser::Chromium => "chromium",
            Browser::Edge => "edge",
            Browser::Brave => "brave",
            Browser::Vivaldi => "vivaldi",
            Browser::Opera => "opera",
            Browser::Arc => "arc",
            Browser::Firefox => "firefox",
            Browser::LibreWolf => "librewolf",
            Browser::Zen => "zen",
            Browser::Safari => "safari",
        }
    }

    /// The schema family this browser belongs to.
    pub fn schema_family(self) -> SchemaFamily {
        match self {
            Browser::Chrome
            | Browser::Chromium
            | Browser::Edge
            | Browser::Brave
            | Browser::Vivaldi
            | Browser::Opera
            | Browser::Arc => SchemaFamily::Chromium,
            Browser::Firefox | Browser::LibreWolf | Browser::Zen => SchemaFamily::Firefox,
            Browser::Safari => SchemaFamily::Safari,
        }
    }
}

impl fmt::Display for Browser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Browser {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "chrome" | "google-chrome" => Ok(Browser::Chrome),
            "chromium" => Ok(Browser::Chromium),
            "edge" | "microsoft-edge" => Ok(Browser::Edge),
            "brave" => Ok(Browser::Brave),
            "vivaldi" => Ok(Browser::Vivaldi),
            "opera" => Ok(Browser::Opera),
            "arc" => Ok(Browser::Arc),
            "firefox" => Ok(Browser::Firefox),
            "librewolf" => Ok(Browser::LibreWolf),
            "zen" => Ok(Browser::Zen),
            "safari" => Ok(Browser::Safari),
            _ => Err(format!("unknown browser: '{s}'")),
        }
    }
}

/// The SQLite schema family determines how we query the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaFamily {
    Chromium,
    Firefox,
    Safari,
}

/// A discovered browser database on disk.
#[derive(Debug, Clone)]
pub struct BrowserSource {
    pub browser: Browser,
    pub profile: String,
    pub db_path: PathBuf,
}

/// A single history entry -- the unified output type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub url: String,
    pub title: Option<String>,
    pub visit_time: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visit_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visit_duration_ms: Option<u64>,
    pub browser: Browser,
    pub profile: String,
}

/// Detect all browser databases available on this system.
///
/// Returns a list of `BrowserSource` for each (browser, profile) that has a
/// history database on disk.
pub fn discover() -> Vec<BrowserSource> {
    let mut sources = Vec::new();

    for &kind in Browser::ALL {
        match kind.schema_family() {
            SchemaFamily::Chromium => {
                sources.extend(chromium::discover_profiles(kind));
            }
            SchemaFamily::Firefox => {
                sources.extend(firefox::discover_profiles(kind));
            }
            SchemaFamily::Safari => {
                sources.extend(safari::discover());
            }
        }
    }

    sources
}

/// Read history entries from a single browser source.
///
/// The database is copied to a temporary directory before reading to avoid
/// lock contention with the running browser. WAL and SHM files are also
/// copied if present.
pub fn read_history(
    source: &BrowserSource,
    since: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
    individual_visits: bool,
) -> Result<Vec<HistoryEntry>> {
    let tmp_dir = tempfile::tempdir()?;
    let db_copy = copy_database(&source.db_path, tmp_dir.path())?;

    let conn = rusqlite::Connection::open_with_flags(
        &db_copy,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let entries = match source.browser.schema_family() {
        SchemaFamily::Chromium => {
            chromium::read_history(&conn, source, since, before, individual_visits)?
        }
        SchemaFamily::Firefox => {
            firefox::read_history(&conn, source, since, before, individual_visits)?
        }
        SchemaFamily::Safari => {
            safari::read_history(&conn, source, since, before, individual_visits)?
        }
    };

    Ok(entries)
}

/// Copy a database and its WAL/SHM companion files to a temporary directory.
///
/// SQLite WAL-mode databases consist of up to three files:
///   - `<dbname>` — the main database
///   - `<dbname>-wal` — write-ahead log (recent uncommitted pages)
///   - `<dbname>-shm` — shared-memory index for the WAL
///
/// If the WAL file is not copied, any pages that haven't been checkpointed
/// back into the main database are silently lost. Both companion files are
/// copied independently because either may exist without the other.
fn copy_database(db_path: &std::path::Path, tmp_dir: &std::path::Path) -> Result<PathBuf> {
    let file_name = db_path
        .file_name()
        .ok_or_else(|| Error::DatabaseNotFound(db_path.to_path_buf()))?;
    let file_name_str = file_name.to_string_lossy();

    let dest = tmp_dir.join(file_name);
    std::fs::copy(db_path, &dest).map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            Error::PermissionDenied {
                path: db_path.to_path_buf(),
                detail: e.to_string(),
            }
        } else {
            Error::Io(e)
        }
    })?;

    // Copy WAL and SHM companion files if they exist.
    // SQLite always names them <dbfile>-wal and <dbfile>-shm regardless of
    // whether the database file has an extension.
    for suffix in ["-wal", "-shm"] {
        let companion = db_path.with_file_name(format!("{file_name_str}{suffix}"));
        if companion.exists() {
            let companion_dest = tmp_dir.join(companion.file_name().unwrap());
            let _ = std::fs::copy(&companion, companion_dest);
        }
    }

    Ok(dest)
}

/// Read history from all provided sources, collecting results and errors.
///
/// Continues past individual failures, returning all successfully read entries
/// along with any errors encountered.
pub fn read_all(
    sources: &[BrowserSource],
    since: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
    individual_visits: bool,
) -> (Vec<HistoryEntry>, Vec<Error>) {
    let mut all_entries = Vec::new();
    let mut errors = Vec::new();

    for source in sources {
        match read_history(source, since, before, individual_visits) {
            Ok(entries) => all_entries.extend(entries),
            Err(e) => errors.push(e),
        }
    }

    // Sort by visit_time descending
    all_entries.sort_by(|a, b| b.visit_time.cmp(&a.visit_time));

    (all_entries, errors)
}
