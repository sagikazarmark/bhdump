//! Integration tests for bhdump library.
//!
//! These tests create in-memory SQLite databases matching each browser's schema,
//! then exercise the full read -> filter -> format pipeline.

mod fixtures;

use bhdump::browsers::chromium;
use bhdump::browsers::firefox;
use bhdump::browsers::safari;
use bhdump::browsers::{BrowserKind, BrowserSource, HistoryEntry};
use bhdump::filter::{FilterConfig, WhereExpr};
use bhdump::format::{OutputFormat, write_entries};
use chrono::{TimeZone, Utc};

fn chrome_source() -> BrowserSource {
    BrowserSource {
        browser: BrowserKind::Chrome,
        profile: "Default".to_string(),
        db_path: "test-fixture".into(), // not used -- we pass conn directly
    }
}

fn firefox_source() -> BrowserSource {
    BrowserSource {
        browser: BrowserKind::Firefox,
        profile: "test-profile".to_string(),
        db_path: "test-fixture".into(),
    }
}

fn safari_source() -> BrowserSource {
    BrowserSource {
        browser: BrowserKind::Safari,
        profile: "default".to_string(),
        db_path: "test-fixture".into(),
    }
}

// ---------------------------------------------------------------------------
// Chromium integration tests
// ---------------------------------------------------------------------------

#[test]
fn chromium_aggregated_reads_all_visible_entries() {
    let conn = fixtures::chromium_db();
    let source = chrome_source();
    let entries = chromium::read_history(&conn, &source, None, None, false).unwrap();

    // Should have 4 visible entries (ids 1,2,3,6 -- not id 4 which is hidden)
    // id 5 (chrome://settings) is NOT hidden=1, so it's included at the DB level.
    // Filtering of internal URLs is done by FilterConfig, not the reader.
    assert_eq!(entries.len(), 5);

    // Check they're tagged correctly
    for entry in &entries {
        assert_eq!(entry.browser, BrowserKind::Chrome);
        assert_eq!(entry.profile, "Default");
    }

    // Verify the first entry (most recent) is example.com
    assert_eq!(entries[0].url, "https://example.com");
    assert_eq!(entries[0].title.as_deref(), Some("Example Domain"));
    assert_eq!(entries[0].visit_count, Some(5));
}

#[test]
fn chromium_aggregated_since_filter() {
    let conn = fixtures::chromium_db();
    let source = chrome_source();

    // Only entries since 2024-01-14T12:00:00Z
    let since = Utc.with_ymd_and_hms(2024, 1, 14, 12, 0, 0).unwrap();
    let entries = chromium::read_history(&conn, &source, Some(since), None, false).unwrap();

    // Should only include example.com (2024-01-15T10:00:00Z) and chrome://settings
    // rust-lang.org is at 08:00 on the 14th -- excluded
    // docs.rs is at 12:00 on the 13th -- excluded
    let urls: Vec<&str> = entries.iter().map(|e| e.url.as_str()).collect();
    assert!(urls.contains(&"https://example.com"));
    assert!(!urls.contains(&"https://docs.rs/serde"));
}

#[test]
fn chromium_aggregated_before_filter() {
    let conn = fixtures::chromium_db();
    let source = chrome_source();

    // Only entries before 2024-01-14T00:00:00Z
    let before = Utc.with_ymd_and_hms(2024, 1, 14, 0, 0, 0).unwrap();
    let entries = chromium::read_history(&conn, &source, None, Some(before), false).unwrap();

    // Should only include docs.rs (2024-01-13T12:00:00Z) and no-title.example.com
    let urls: Vec<&str> = entries.iter().map(|e| e.url.as_str()).collect();
    assert!(urls.contains(&"https://docs.rs/serde"));
    assert!(!urls.contains(&"https://example.com"));
    assert!(!urls.contains(&"https://rust-lang.org"));
}

#[test]
fn chromium_aggregated_date_range() {
    let conn = fixtures::chromium_db();
    let source = chrome_source();

    let since = Utc.with_ymd_and_hms(2024, 1, 14, 0, 0, 0).unwrap();
    let before = Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap();
    let entries = chromium::read_history(&conn, &source, Some(since), Some(before), false).unwrap();

    // Only rust-lang.org (2024-01-14T08:00:00Z) falls in this range
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].url, "https://rust-lang.org");
}

#[test]
fn chromium_individual_visits() {
    let conn = fixtures::chromium_db();
    let source = chrome_source();
    let entries = chromium::read_history(&conn, &source, None, None, true).unwrap();

    // 4 visits for visible URLs (visits 1,2,3,4 -- not visit 5 which is for hidden url)
    assert_eq!(entries.len(), 4);

    // Individual visits should have no visit_count
    for entry in &entries {
        assert!(entry.visit_count.is_none());
    }

    // Check visit durations
    let example_visits: Vec<&HistoryEntry> = entries
        .iter()
        .filter(|e| e.url == "https://example.com")
        .collect();
    assert_eq!(example_visits.len(), 2);

    // The zero-duration visit should have visit_duration_ms = None
    let rust_visits: Vec<&HistoryEntry> = entries
        .iter()
        .filter(|e| e.url == "https://rust-lang.org")
        .collect();
    assert_eq!(rust_visits.len(), 2);
    let has_zero_duration = rust_visits.iter().any(|v| v.visit_duration_ms.is_none());
    assert!(has_zero_duration);
}

#[test]
fn chromium_individual_visits_with_date_filter() {
    let conn = fixtures::chromium_db();
    let source = chrome_source();

    let since = Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap();
    let entries = chromium::read_history(&conn, &source, Some(since), None, true).unwrap();

    // Only visit 1 (example.com at 2024-01-15T10:00:00Z) is after this date
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].url, "https://example.com");
    assert_eq!(entries[0].visit_duration_ms, Some(5000)); // 5 seconds
}

// ---------------------------------------------------------------------------
// Firefox integration tests
// ---------------------------------------------------------------------------

#[test]
fn firefox_aggregated_reads_all_visible_entries() {
    let conn = fixtures::firefox_db();
    let source = firefox_source();
    let entries = firefox::read_history(&conn, &source, None, None, false).unwrap();

    // 3 visible entries (ids 1,2,3). id 4 is hidden=1, id 5 (about:blank) has
    // no visit in moz_historyvisits for it... wait, it does (visit 5 is for place 4).
    // Place 5 (about:blank) has no visits, so the INNER JOIN excludes it.
    assert_eq!(entries.len(), 3);

    for entry in &entries {
        assert_eq!(entry.browser, BrowserKind::Firefox);
        assert_eq!(entry.profile, "test-profile");
    }

    assert_eq!(entries[0].url, "https://example.com");
    assert_eq!(entries[0].visit_count, Some(5));
}

#[test]
fn firefox_aggregated_since_filter() {
    let conn = fixtures::firefox_db();
    let source = firefox_source();

    let since = Utc.with_ymd_and_hms(2024, 1, 14, 12, 0, 0).unwrap();
    let entries = firefox::read_history(&conn, &source, Some(since), None, false).unwrap();

    // Only example.com has visits after 2024-01-14T12:00:00Z
    // (visit at 2024-01-15T10:00:00Z)
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].url, "https://example.com");
}

#[test]
fn firefox_individual_visits() {
    let conn = fixtures::firefox_db();
    let source = firefox_source();
    let entries = firefox::read_history(&conn, &source, None, None, true).unwrap();

    // 4 visits for visible places (visits 1,2,3,4 -- not visit 5 which is for hidden place 4)
    assert_eq!(entries.len(), 4);

    for entry in &entries {
        assert!(entry.visit_count.is_none());
        assert!(entry.visit_duration_ms.is_none()); // Firefox doesn't track duration
    }
}

#[test]
fn firefox_individual_visits_date_range() {
    let conn = fixtures::firefox_db();
    let source = firefox_source();

    let since = Utc.with_ymd_and_hms(2024, 1, 14, 0, 0, 0).unwrap();
    let before = Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap();
    let entries = firefox::read_history(&conn, &source, Some(since), Some(before), true).unwrap();

    // Visits in this range:
    // visit 2: example.com at 2024-01-14T10:00:00Z
    // visit 3: mozilla.org at 2024-01-14T08:00:00Z
    assert_eq!(entries.len(), 2);
}

// ---------------------------------------------------------------------------
// Safari integration tests
// ---------------------------------------------------------------------------

#[test]
fn safari_aggregated_reads_all_entries() {
    let conn = fixtures::safari_db();
    let source = safari_source();
    let entries = safari::read_history(&conn, &source, None, None, false).unwrap();

    // 3 items, all with visits
    assert_eq!(entries.len(), 3);

    for entry in &entries {
        assert_eq!(entry.browser, BrowserKind::Safari);
        assert_eq!(entry.profile, "default");
    }

    // apple.com has 2 visits
    let apple = entries
        .iter()
        .find(|e| e.url == "https://apple.com")
        .unwrap();
    assert_eq!(apple.visit_count, Some(2));

    // webkit.org has 2 visits
    let webkit = entries
        .iter()
        .find(|e| e.url == "https://webkit.org")
        .unwrap();
    assert_eq!(webkit.visit_count, Some(2));

    // developer.apple.com has 1 visit
    let dev = entries
        .iter()
        .find(|e| e.url == "https://developer.apple.com/documentation")
        .unwrap();
    assert_eq!(dev.visit_count, Some(1));
}

#[test]
fn safari_title_comes_from_visits() {
    let conn = fixtures::safari_db();
    let source = safari_source();
    let entries = safari::read_history(&conn, &source, None, None, false).unwrap();

    // Safari stores titles on history_visits, not history_items.
    // The aggregated query uses MAX(v.title), so it should pick up a title.
    let apple = entries
        .iter()
        .find(|e| e.url == "https://apple.com")
        .unwrap();
    assert!(apple.title.is_some());
}

#[test]
fn safari_individual_visits() {
    let conn = fixtures::safari_db();
    let source = safari_source();
    let entries = safari::read_history(&conn, &source, None, None, true).unwrap();

    // 5 individual visits total
    assert_eq!(entries.len(), 5);

    // One visit should have NULL title
    let null_title_count = entries.iter().filter(|e| e.title.is_none()).count();
    assert_eq!(null_title_count, 1);
}

#[test]
fn safari_since_filter() {
    let conn = fixtures::safari_db();
    let source = safari_source();

    let since = Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap();
    let entries = safari::read_history(&conn, &source, Some(since), None, true).unwrap();

    // Visits at 2024-01-15T10:00:00Z: visit 1 (apple.com) and visit 4 (webkit.org)
    assert_eq!(entries.len(), 2);
}

// ---------------------------------------------------------------------------
// Filter integration tests (using Chromium fixture data)
// ---------------------------------------------------------------------------

fn make_test_entries() -> Vec<HistoryEntry> {
    vec![
        HistoryEntry {
            url: "https://example.com/page1".to_string(),
            title: Some("Example Page 1".to_string()),
            visit_time: Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap(),
            visit_count: Some(5),
            visit_duration_ms: None,
            browser: BrowserKind::Chrome,
            profile: "Default".to_string(),
        },
        HistoryEntry {
            url: "https://example.com/page2".to_string(),
            title: Some("Example Page 2".to_string()),
            visit_time: Utc.with_ymd_and_hms(2024, 1, 14, 10, 0, 0).unwrap(),
            visit_count: Some(2),
            visit_duration_ms: None,
            browser: BrowserKind::Chrome,
            profile: "Default".to_string(),
        },
        HistoryEntry {
            url: "https://rust-lang.org".to_string(),
            title: Some("Rust Programming Language".to_string()),
            visit_time: Utc.with_ymd_and_hms(2024, 1, 13, 10, 0, 0).unwrap(),
            visit_count: Some(20),
            visit_duration_ms: None,
            browser: BrowserKind::Firefox,
            profile: "test-profile".to_string(),
        },
        HistoryEntry {
            url: "chrome://settings".to_string(),
            title: Some("Settings".to_string()),
            visit_time: Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap(),
            visit_count: Some(1),
            visit_duration_ms: None,
            browser: BrowserKind::Chrome,
            profile: "Default".to_string(),
        },
        HistoryEntry {
            url: "https://example.com/page1".to_string(),
            title: Some("Example Page 1 (Firefox)".to_string()),
            visit_time: Utc.with_ymd_and_hms(2024, 1, 12, 10, 0, 0).unwrap(),
            visit_count: Some(3),
            visit_duration_ms: None,
            browser: BrowserKind::Firefox,
            profile: "test-profile".to_string(),
        },
    ]
}

#[test]
fn filter_excludes_internal_urls_by_default() {
    let entries = make_test_entries();
    let filter = FilterConfig::default();
    let result = filter.apply(entries).unwrap();

    assert!(!result.iter().any(|e| e.url.starts_with("chrome://")));
    assert_eq!(result.len(), 4);
}

#[test]
fn filter_includes_internal_urls_when_requested() {
    let entries = make_test_entries();
    let filter = FilterConfig {
        include_internal: true,
        ..Default::default()
    };
    let result = filter.apply(entries).unwrap();

    assert!(result.iter().any(|e| e.url.starts_with("chrome://")));
    assert_eq!(result.len(), 5);
}

#[test]
fn filter_where_url_contains() {
    let entries = make_test_entries();
    let filter = FilterConfig {
        where_expr: Some(WhereExpr::compile(r#"url.contains("rust-lang")"#).unwrap()),
        ..Default::default()
    };
    let result = filter.apply(entries).unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].url, "https://rust-lang.org");
}

#[test]
fn filter_where_url_exclude() {
    let entries = make_test_entries();
    let filter = FilterConfig {
        where_expr: Some(WhereExpr::compile(r#"!url.matches("example\\.com")"#).unwrap()),
        ..Default::default()
    };
    let result = filter.apply(entries).unwrap();

    // Should only have rust-lang.org (chrome://settings is excluded as internal)
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].url, "https://rust-lang.org");
}

#[test]
fn filter_where_domain() {
    let entries = make_test_entries();
    let filter = FilterConfig {
        where_expr: Some(WhereExpr::compile(r#"domain == "example.com""#).unwrap()),
        ..Default::default()
    };
    let result = filter.apply(entries).unwrap();

    assert_eq!(result.len(), 3); // page1 (Chrome), page2, page1 (Firefox)
    assert!(result.iter().all(|e| e.url.contains("example.com")));
}

#[test]
fn filter_where_title() {
    let entries = make_test_entries();
    let filter = FilterConfig {
        where_expr: Some(WhereExpr::compile(r#"title.matches("(?i)rust")"#).unwrap()),
        ..Default::default()
    };
    let result = filter.apply(entries).unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].url, "https://rust-lang.org");
}

#[test]
fn filter_where_min_visit_count() {
    let entries = make_test_entries();
    let filter = FilterConfig {
        where_expr: Some(WhereExpr::compile("visit_count >= 5").unwrap()),
        ..Default::default()
    };
    let result = filter.apply(entries).unwrap();

    // example.com/page1 (5), rust-lang.org (20) -- but not chrome://settings or example.com/page2
    assert_eq!(result.len(), 2);
}

#[test]
fn filter_limit() {
    let entries = make_test_entries();
    let filter = FilterConfig {
        limit: Some(2),
        ..Default::default()
    };
    let result = filter.apply(entries).unwrap();

    assert_eq!(result.len(), 2);
}

#[test]
fn filter_deduplicate() {
    let entries = make_test_entries();
    let filter = FilterConfig {
        deduplicate: true,
        ..Default::default()
    };
    let result = filter.apply(entries).unwrap();

    // page1 appears twice (Chrome and Firefox), dedup keeps first (most recent)
    let page1_entries: Vec<&HistoryEntry> = result
        .iter()
        .filter(|e| e.url == "https://example.com/page1")
        .collect();
    assert_eq!(page1_entries.len(), 1);
    assert_eq!(page1_entries[0].browser, BrowserKind::Chrome); // Chrome entry is more recent
}

#[test]
fn filter_combined() {
    let entries = make_test_entries();
    let filter = FilterConfig {
        where_expr: Some(
            WhereExpr::compile(r#"url.matches("example\\.com") && visit_count >= 3"#).unwrap(),
        ),
        deduplicate: true,
        limit: Some(10),
        ..Default::default()
    };
    let result = filter.apply(entries).unwrap();

    // example.com entries with visit_count >= 3: page1 (5, Chrome), page1 (3, Firefox)
    // After dedup: page1 (Chrome only, since it's more recent)
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].url, "https://example.com/page1");
}

fn make_entries_with_noise() -> Vec<HistoryEntry> {
    let mut entries = make_test_entries();
    entries.push(HistoryEntry {
        url: "https://accounts.google.com/signin/v2/identifier".to_string(),
        title: Some("Sign in - Google Accounts".to_string()),
        visit_time: Utc.with_ymd_and_hms(2024, 1, 15, 9, 0, 0).unwrap(),
        visit_count: Some(50),
        visit_duration_ms: None,
        browser: BrowserKind::Chrome,
        profile: "Default".to_string(),
    });
    entries.push(HistoryEntry {
        url: "https://t.co/abc123".to_string(),
        title: None,
        visit_time: Utc.with_ymd_and_hms(2024, 1, 14, 12, 0, 0).unwrap(),
        visit_count: Some(1),
        visit_duration_ms: None,
        browser: BrowserKind::Chrome,
        profile: "Default".to_string(),
    });
    entries.push(HistoryEntry {
        url: "https://consent.youtube.com/m?continue=https://youtube.com".to_string(),
        title: Some("Before you continue to YouTube".to_string()),
        visit_time: Utc.with_ymd_and_hms(2024, 1, 13, 8, 0, 0).unwrap(),
        visit_count: Some(5),
        visit_duration_ms: None,
        browser: BrowserKind::Firefox,
        profile: "test-profile".to_string(),
    });
    entries
}

#[test]
fn filter_excludes_noise_by_default() {
    let entries = make_entries_with_noise();
    let filter = FilterConfig::default();
    let result = filter.apply(entries).unwrap();

    assert!(!result.iter().any(|e| e.url.contains("accounts.google.com")));
    assert!(!result.iter().any(|e| e.url.contains("t.co")));
    assert!(!result.iter().any(|e| e.url.contains("consent.youtube.com")));
    // The 4 non-noise, non-internal entries remain
    assert_eq!(result.len(), 4);
}

#[test]
fn filter_includes_noise_when_requested() {
    let entries = make_entries_with_noise();
    let filter = FilterConfig {
        include_noise: true,
        ..Default::default()
    };
    let result = filter.apply(entries).unwrap();

    assert!(result.iter().any(|e| e.url.contains("accounts.google.com")));
    assert!(result.iter().any(|e| e.url.contains("t.co")));
    assert!(result.iter().any(|e| e.url.contains("consent.youtube.com")));
    // 4 normal + 3 noise entries (chrome://settings still excluded)
    assert_eq!(result.len(), 7);
}

// ---------------------------------------------------------------------------
// Output format roundtrip tests
// ---------------------------------------------------------------------------

#[test]
fn json_roundtrip() {
    let entries = make_test_entries();
    let filter = FilterConfig::default();
    let filtered = filter.apply(entries).unwrap();

    let mut buf = Vec::new();
    write_entries(&mut buf, &filtered, OutputFormat::Json).unwrap();
    let output = String::from_utf8(buf).unwrap();

    // Parse back
    let parsed: Vec<HistoryEntry> = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed.len(), filtered.len());
    assert_eq!(parsed[0].url, filtered[0].url);
    assert_eq!(parsed[0].browser, filtered[0].browser);
    assert_eq!(parsed[0].profile, filtered[0].profile);
    assert_eq!(parsed[0].visit_time, filtered[0].visit_time);
}

#[test]
fn jsonl_roundtrip() {
    let entries = make_test_entries();
    let filter = FilterConfig::default();
    let filtered = filter.apply(entries).unwrap();

    let mut buf = Vec::new();
    write_entries(&mut buf, &filtered, OutputFormat::JsonLines).unwrap();
    let output = String::from_utf8(buf).unwrap();

    let parsed: Vec<HistoryEntry> = output
        .trim()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(parsed.len(), filtered.len());

    for (parsed_entry, original) in parsed.iter().zip(filtered.iter()) {
        assert_eq!(parsed_entry.url, original.url);
        assert_eq!(parsed_entry.browser, original.browser);
        assert_eq!(parsed_entry.profile, original.profile);
    }
}

#[test]
fn csv_has_correct_structure() {
    let entries = make_test_entries();
    let filter = FilterConfig::default();
    let filtered = filter.apply(entries).unwrap();

    let mut buf = Vec::new();
    write_entries(&mut buf, &filtered, OutputFormat::Csv).unwrap();
    let output = String::from_utf8(buf).unwrap();

    let mut reader = csv::Reader::from_reader(output.as_bytes());
    let headers = reader.headers().unwrap();
    assert_eq!(
        headers.iter().collect::<Vec<_>>(),
        vec![
            "url",
            "title",
            "visit_time",
            "visit_count",
            "visit_duration_ms",
            "browser",
            "profile"
        ]
    );

    let records: Vec<csv::StringRecord> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(records.len(), filtered.len());

    // Verify first record
    assert_eq!(&records[0][0], filtered[0].url.as_str());
    assert_eq!(&records[0][5], filtered[0].browser.as_str());
    assert_eq!(&records[0][6], filtered[0].profile.as_str());
}

#[test]
fn tsv_uses_tabs() {
    let entries = vec![HistoryEntry {
        url: "https://example.com".to_string(),
        title: Some("Test".to_string()),
        visit_time: Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap(),
        visit_count: Some(1),
        visit_duration_ms: None,
        browser: BrowserKind::Chrome,
        profile: "Default".to_string(),
    }];

    let mut buf = Vec::new();
    write_entries(&mut buf, &entries, OutputFormat::Tsv).unwrap();
    let output = String::from_utf8(buf).unwrap();

    // Header and data rows should use tabs
    let lines: Vec<&str> = output.trim().lines().collect();
    assert!(lines[0].contains('\t'));
    assert!(!lines[0].contains(','));
}

#[test]
fn empty_entries_produces_valid_output() {
    let entries: Vec<HistoryEntry> = vec![];

    // JSON: should produce "[]"
    let mut buf = Vec::new();
    write_entries(&mut buf, &entries, OutputFormat::Json).unwrap();
    let json_output = String::from_utf8(buf).unwrap();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&json_output).unwrap();
    assert!(parsed.is_empty());

    // JSONL: should produce empty string
    let mut buf = Vec::new();
    write_entries(&mut buf, &entries, OutputFormat::JsonLines).unwrap();
    let jsonl_output = String::from_utf8(buf).unwrap();
    assert!(jsonl_output.is_empty());

    // CSV: should produce header only
    let mut buf = Vec::new();
    write_entries(&mut buf, &entries, OutputFormat::Csv).unwrap();
    let csv_output = String::from_utf8(buf).unwrap();
    let lines: Vec<&str> = csv_output.trim().lines().collect();
    assert_eq!(lines.len(), 1); // header only
}

// ---------------------------------------------------------------------------
// Full pipeline test (library API)
// ---------------------------------------------------------------------------

#[test]
fn full_pipeline_chromium_to_json() {
    let conn = fixtures::chromium_db();
    let source = chrome_source();

    // Step 1: Read
    let entries = chromium::read_history(&conn, &source, None, None, false).unwrap();
    assert!(!entries.is_empty());

    // Step 2: Filter (exclude internal, limit to 2)
    let filter = FilterConfig {
        limit: Some(2),
        ..Default::default()
    };
    let filtered = filter.apply(entries).unwrap();
    assert_eq!(filtered.len(), 2);

    // Step 3: Format as JSON
    let mut buf = Vec::new();
    write_entries(&mut buf, &filtered, OutputFormat::Json).unwrap();
    let output = String::from_utf8(buf).unwrap();

    // Step 4: Parse back and verify
    let parsed: Vec<HistoryEntry> = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].browser, BrowserKind::Chrome);
    assert_eq!(parsed[0].profile, "Default");
    assert!(parsed[0].visit_time > parsed[1].visit_time); // descending order
}

#[test]
fn full_pipeline_firefox_to_csv() {
    let conn = fixtures::firefox_db();
    let source = firefox_source();

    let entries = firefox::read_history(&conn, &source, None, None, false).unwrap();

    let filter = FilterConfig {
        where_expr: Some(
            WhereExpr::compile(r#"domain == "mozilla.org" || domain.endsWith(".mozilla.org")"#)
                .unwrap(),
        ),
        ..Default::default()
    };
    let filtered = filter.apply(entries).unwrap();

    // mozilla.org and developer.mozilla.org should match
    assert_eq!(filtered.len(), 2);

    let mut buf = Vec::new();
    write_entries(&mut buf, &filtered, OutputFormat::Csv).unwrap();
    let output = String::from_utf8(buf).unwrap();

    let mut reader = csv::Reader::from_reader(output.as_bytes());
    let records: Vec<csv::StringRecord> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(records.len(), 2);

    for record in &records {
        assert_eq!(&record[5], "firefox");
        assert_eq!(&record[6], "test-profile");
    }
}

#[test]
fn full_pipeline_safari_to_jsonl() {
    let conn = fixtures::safari_db();
    let source = safari_source();

    let entries = safari::read_history(&conn, &source, None, None, true).unwrap();

    let filter = FilterConfig::default();
    let filtered = filter.apply(entries).unwrap();

    let mut buf = Vec::new();
    write_entries(&mut buf, &filtered, OutputFormat::JsonLines).unwrap();
    let output = String::from_utf8(buf).unwrap();

    let parsed: Vec<HistoryEntry> = output
        .trim()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();

    assert_eq!(parsed.len(), filtered.len());
    for entry in &parsed {
        assert_eq!(entry.browser, BrowserKind::Safari);
        assert_eq!(entry.profile, "default");
    }
}

// ---------------------------------------------------------------------------
// BrowserKind serialization/deserialization tests
// ---------------------------------------------------------------------------

#[test]
fn browser_kind_serde_roundtrip() {
    for &kind in BrowserKind::ALL {
        let entry = HistoryEntry {
            url: "https://example.com".to_string(),
            title: Some("Test".to_string()),
            visit_time: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            visit_count: Some(1),
            visit_duration_ms: None,
            browser: kind,
            profile: "test".to_string(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let parsed: HistoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.browser, kind);
    }
}

#[test]
fn browser_kind_from_str() {
    assert_eq!(
        "chrome".parse::<BrowserKind>().unwrap(),
        BrowserKind::Chrome
    );
    assert_eq!(
        "CHROME".parse::<BrowserKind>().unwrap(),
        BrowserKind::Chrome
    );
    assert_eq!(
        "google-chrome".parse::<BrowserKind>().unwrap(),
        BrowserKind::Chrome
    );
    assert_eq!(
        "microsoft-edge".parse::<BrowserKind>().unwrap(),
        BrowserKind::Edge
    );
    assert_eq!(
        "librewolf".parse::<BrowserKind>().unwrap(),
        BrowserKind::LibreWolf
    );
    assert!("unknown-browser".parse::<BrowserKind>().is_err());
}

// ---------------------------------------------------------------------------
// WAL copy safety tests
// ---------------------------------------------------------------------------

/// Helper: create a WAL-mode Chromium database on disk with data only in the WAL
/// (not yet checkpointed to the main DB file).
///
/// Returns `(db_path, _guard_connection)`. The guard connection must be kept
/// alive to prevent SQLite from checkpointing and deleting the WAL on close.
/// This simulates a real browser that still has the database open.
fn create_wal_mode_chromium_db(
    dir: &std::path::Path,
) -> (std::path::PathBuf, rusqlite::Connection) {
    let db_path = dir.join("History");
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    conn.pragma_update(None, "journal_mode", "wal").unwrap();

    conn.execute_batch(
        "CREATE TABLE urls (
            id INTEGER PRIMARY KEY,
            url TEXT,
            title TEXT,
            visit_count INTEGER DEFAULT 0,
            typed_count INTEGER DEFAULT 0,
            last_visit_time INTEGER DEFAULT 0,
            hidden INTEGER DEFAULT 0
        );

        CREATE TABLE visits (
            id INTEGER PRIMARY KEY,
            url INTEGER NOT NULL,
            visit_time INTEGER NOT NULL,
            from_visit INTEGER DEFAULT 0,
            transition INTEGER DEFAULT 0,
            segment_id INTEGER DEFAULT 0,
            visit_duration INTEGER DEFAULT 0
        );

        -- 2024-01-15T10:00:00Z as WebKit timestamp
        INSERT INTO urls (id, url, title, visit_count, last_visit_time, hidden)
        VALUES (1, 'https://wal-test.example.com', 'WAL Test Page', 1, 13349786400000000, 0);

        INSERT INTO visits (id, url, visit_time, visit_duration)
        VALUES (1, 1, 13349786400000000, 5000000);",
    )
    .unwrap();

    // Open a reader to prevent WAL deletion when the writer closes.
    // SQLite only removes the WAL when the *last* connection closes.
    let guard =
        rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    // Start a read transaction to hold the WAL open
    guard
        .execute_batch("BEGIN; SELECT * FROM urls LIMIT 1;")
        .unwrap();

    drop(conn);
    (db_path, guard)
}

#[test]
fn wal_data_readable_through_read_history_chromium() {
    let src_dir = tempfile::tempdir().unwrap();
    let (db_path, _guard) = create_wal_mode_chromium_db(src_dir.path());

    // Verify WAL file exists on disk (data hasn't been checkpointed)
    let wal_path = db_path.with_file_name("History-wal");
    assert!(wal_path.exists(), "WAL file should exist at {wal_path:?}");

    let source = BrowserSource {
        browser: BrowserKind::Chrome,
        profile: "wal-test".to_string(),
        db_path,
    };

    // read_history copies DB + WAL internally; data should be present
    let entries = bhdump::browsers::read_history(&source, None, None, false).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].url, "https://wal-test.example.com");
    assert_eq!(entries[0].title.as_deref(), Some("WAL Test Page"));
}

#[test]
fn wal_data_readable_individual_visits_chromium() {
    let src_dir = tempfile::tempdir().unwrap();
    let (db_path, _guard) = create_wal_mode_chromium_db(src_dir.path());

    let source = BrowserSource {
        browser: BrowserKind::Chrome,
        profile: "wal-test".to_string(),
        db_path,
    };

    // Individual visits mode also needs WAL data
    let entries = bhdump::browsers::read_history(&source, None, None, true).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].url, "https://wal-test.example.com");
    assert!(entries[0].visit_count.is_none()); // individual visits don't have count
}

/// Helper: create a WAL-mode Firefox database on disk.
fn create_wal_mode_firefox_db(dir: &std::path::Path) -> (std::path::PathBuf, rusqlite::Connection) {
    let db_path = dir.join("places.sqlite");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.pragma_update(None, "journal_mode", "wal").unwrap();

    conn.execute_batch(
        "CREATE TABLE moz_places (
            id INTEGER PRIMARY KEY,
            url TEXT,
            title TEXT,
            rev_host TEXT,
            visit_count INTEGER DEFAULT 0,
            hidden INTEGER DEFAULT 0,
            typed INTEGER DEFAULT 0,
            frecency INTEGER DEFAULT -1,
            last_visit_date INTEGER,
            url_hash INTEGER DEFAULT 0
        );

        CREATE TABLE moz_historyvisits (
            id INTEGER PRIMARY KEY,
            from_visit INTEGER DEFAULT 0,
            place_id INTEGER NOT NULL,
            visit_date INTEGER,
            visit_type INTEGER DEFAULT 0,
            session INTEGER DEFAULT 0
        );

        -- 2024-01-15T10:00:00Z as Firefox timestamp
        INSERT INTO moz_places (id, url, title, visit_count, hidden, last_visit_date)
        VALUES (1, 'https://wal-firefox.example.com', 'WAL Firefox Page', 3, 0, 1705312800000000);

        INSERT INTO moz_historyvisits (id, place_id, visit_date, visit_type)
        VALUES (1, 1, 1705312800000000, 1);",
    )
    .unwrap();

    let guard =
        rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    guard
        .execute_batch("BEGIN; SELECT * FROM moz_places LIMIT 1;")
        .unwrap();

    drop(conn);
    (db_path, guard)
}

#[test]
fn wal_data_readable_through_read_history_firefox() {
    let src_dir = tempfile::tempdir().unwrap();
    let (db_path, _guard) = create_wal_mode_firefox_db(src_dir.path());

    let wal_path = db_path.with_file_name("places.sqlite-wal");
    assert!(wal_path.exists(), "WAL file should exist at {wal_path:?}");

    let source = BrowserSource {
        browser: BrowserKind::Firefox,
        profile: "wal-test".to_string(),
        db_path,
    };

    let entries = bhdump::browsers::read_history(&source, None, None, false).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].url, "https://wal-firefox.example.com");
    assert_eq!(entries[0].title.as_deref(), Some("WAL Firefox Page"));
    assert_eq!(entries[0].visit_count, Some(3));
}

/// Helper: create a WAL-mode Safari database on disk.
fn create_wal_mode_safari_db(dir: &std::path::Path) -> (std::path::PathBuf, rusqlite::Connection) {
    let db_path = dir.join("History.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.pragma_update(None, "journal_mode", "wal").unwrap();

    conn.execute_batch(
        "CREATE TABLE history_items (
            id INTEGER PRIMARY KEY,
            url TEXT NOT NULL UNIQUE,
            domain_expansion TEXT,
            visit_count INTEGER DEFAULT 0,
            daily_visit_counts BLOB,
            weekly_visit_counts BLOB,
            score REAL DEFAULT 0
        );

        CREATE TABLE history_visits (
            id INTEGER PRIMARY KEY,
            history_item INTEGER NOT NULL,
            visit_time REAL NOT NULL,
            title TEXT,
            http_non_get INTEGER DEFAULT 0,
            redirect_source INTEGER,
            redirect_destination INTEGER,
            origin INTEGER DEFAULT 0,
            generation INTEGER DEFAULT 0,
            attributes INTEGER DEFAULT 0,
            score REAL DEFAULT 0
        );

        -- 2024-01-15T10:00:00Z: Safari seconds = 1705312800 - 978307200 = 727005600.0
        INSERT INTO history_items (id, url, visit_count)
        VALUES (1, 'https://wal-safari.example.com', 2);

        INSERT INTO history_visits (id, history_item, visit_time, title)
        VALUES (1, 1, 727005600.0, 'WAL Safari Page');",
    )
    .unwrap();

    let guard =
        rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    guard
        .execute_batch("BEGIN; SELECT * FROM history_items LIMIT 1;")
        .unwrap();

    drop(conn);
    (db_path, guard)
}

#[test]
fn wal_data_readable_through_read_history_safari() {
    let src_dir = tempfile::tempdir().unwrap();
    let (db_path, _guard) = create_wal_mode_safari_db(src_dir.path());

    let wal_path = db_path.with_file_name("History.db-wal");
    assert!(wal_path.exists(), "WAL file should exist at {wal_path:?}");

    let source = BrowserSource {
        browser: BrowserKind::Safari,
        profile: "default".to_string(),
        db_path,
    };

    let entries = bhdump::browsers::read_history(&source, None, None, false).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].url, "https://wal-safari.example.com");
    assert_eq!(entries[0].title.as_deref(), Some("WAL Safari Page"));
    // visit_count is COUNT(visits), not the value from history_items
    assert_eq!(entries[0].visit_count, Some(1));
}
