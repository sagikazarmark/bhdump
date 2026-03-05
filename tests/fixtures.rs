//! Helpers for creating in-memory SQLite fixture databases matching each browser schema.

use rusqlite::Connection;

/// Create an in-memory Chromium-schema database with test data.
///
/// Schema matches Chrome/Edge/Brave/Vivaldi/Opera/Arc `History` file.
pub fn chromium_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();

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
        -- Unix: 1705312800, WebKit offset: 11644473600
        -- WebKit seconds: 13349786400, microseconds: 13349786400000000
        INSERT INTO urls (id, url, title, visit_count, last_visit_time, hidden)
        VALUES (1, 'https://example.com', 'Example Domain', 5, 13349786400000000, 0);

        -- 2024-01-14T08:00:00Z => 13349692800000000
        INSERT INTO urls (id, url, title, visit_count, last_visit_time, hidden)
        VALUES (2, 'https://rust-lang.org', 'Rust Programming Language', 12, 13349692800000000, 0);

        -- 2024-01-13T12:00:00Z => 13349620800000000
        INSERT INTO urls (id, url, title, visit_count, last_visit_time, hidden)
        VALUES (3, 'https://docs.rs/serde', 'serde - Rust', 3, 13349620800000000, 0);

        -- Hidden entry (should be excluded by default)
        INSERT INTO urls (id, url, title, visit_count, last_visit_time, hidden)
        VALUES (4, 'https://hidden.example.com', 'Hidden Page', 1, 13349786400000000, 1);

        -- Internal URL (should be excluded by default)
        INSERT INTO urls (id, url, title, visit_count, last_visit_time, hidden)
        VALUES (5, 'chrome://settings', 'Settings', 2, 13349786400000000, 0);

        -- Entry with NULL title
        INSERT INTO urls (id, url, title, visit_count, last_visit_time, hidden)
        VALUES (6, 'https://no-title.example.com', NULL, 1, 13349600000000000, 0);

        -- Individual visits for url 1 (example.com)
        INSERT INTO visits (id, url, visit_time, visit_duration)
        VALUES (1, 1, 13349786400000000, 5000000);  -- 5 seconds

        INSERT INTO visits (id, url, visit_time, visit_duration)
        VALUES (2, 1, 13349700000000000, 10000000); -- 10 seconds

        -- Individual visits for url 2 (rust-lang.org)
        INSERT INTO visits (id, url, visit_time, visit_duration)
        VALUES (3, 2, 13349692800000000, 30000000); -- 30 seconds

        -- Visit with zero duration (should still appear in individual visits)
        INSERT INTO visits (id, url, visit_time, visit_duration)
        VALUES (4, 2, 13349680000000000, 0);

        -- Visit for hidden URL (should be excluded)
        INSERT INTO visits (id, url, visit_time, visit_duration)
        VALUES (5, 4, 13349786400000000, 1000000);
        ",
    )
    .unwrap();

    conn
}

/// Create an in-memory Firefox-schema database with test data.
///
/// Schema matches Firefox/LibreWolf/Zen `places.sqlite` file.
pub fn firefox_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();

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

        -- 2024-01-15T10:00:00Z as Firefox timestamp (microseconds since Unix epoch)
        -- Unix seconds: 1705312800, microseconds: 1705312800000000
        INSERT INTO moz_places (id, url, title, visit_count, hidden, last_visit_date)
        VALUES (1, 'https://example.com', 'Example Domain', 5, 0, 1705312800000000);

        -- 2024-01-14T08:00:00Z => 1705219200000000
        INSERT INTO moz_places (id, url, title, visit_count, hidden, last_visit_date)
        VALUES (2, 'https://mozilla.org', 'Mozilla', 10, 0, 1705219200000000);

        -- 2024-01-13T12:00:00Z => 1705147200000000
        INSERT INTO moz_places (id, url, title, visit_count, hidden, last_visit_date)
        VALUES (3, 'https://developer.mozilla.org/en-US/docs/Web', 'MDN Web Docs', 20, 0, 1705147200000000);

        -- Hidden entry
        INSERT INTO moz_places (id, url, title, visit_count, hidden, last_visit_date)
        VALUES (4, 'https://hidden.example.com', 'Hidden', 1, 1, 1705312800000000);

        -- Internal URL
        INSERT INTO moz_places (id, url, title, visit_count, hidden, last_visit_date)
        VALUES (5, 'about:blank', NULL, 100, 0, 1705312800000000);

        -- Visits for place 1 (example.com)
        INSERT INTO moz_historyvisits (id, place_id, visit_date, visit_type)
        VALUES (1, 1, 1705312800000000, 1);

        INSERT INTO moz_historyvisits (id, place_id, visit_date, visit_type)
        VALUES (2, 1, 1705226400000000, 2);

        -- Visits for place 2 (mozilla.org)
        INSERT INTO moz_historyvisits (id, place_id, visit_date, visit_type)
        VALUES (3, 2, 1705219200000000, 1);

        -- Visits for place 3 (MDN)
        INSERT INTO moz_historyvisits (id, place_id, visit_date, visit_type)
        VALUES (4, 3, 1705147200000000, 2);

        -- Visit for hidden place (should be excluded)
        INSERT INTO moz_historyvisits (id, place_id, visit_date, visit_type)
        VALUES (5, 4, 1705312800000000, 1);
        ",
    )
    .unwrap();

    conn
}

/// Create an in-memory Safari-schema database with test data.
///
/// Schema matches Safari `History.db` file.
pub fn safari_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();

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

        -- 2024-01-15T10:00:00Z as Safari timestamp
        -- Unix: 1705312800, Core Data offset: 978307200
        -- Safari seconds: 1705312800 - 978307200 = 727005600.0
        INSERT INTO history_items (id, url, visit_count)
        VALUES (1, 'https://apple.com', 8);

        INSERT INTO history_items (id, url, visit_count)
        VALUES (2, 'https://developer.apple.com/documentation', 3);

        INSERT INTO history_items (id, url, visit_count)
        VALUES (3, 'https://webkit.org', 5);

        -- Visits for item 1 (apple.com) -- note: title is on visits, not items!
        INSERT INTO history_visits (id, history_item, visit_time, title)
        VALUES (1, 1, 727005600.0, 'Apple');

        INSERT INTO history_visits (id, history_item, visit_time, title)
        VALUES (2, 1, 726919200.0, 'Apple Inc.');  -- 2024-01-14T10:00:00Z

        -- Visits for item 2 (developer.apple.com)
        INSERT INTO history_visits (id, history_item, visit_time, title)
        VALUES (3, 2, 726832800.0, 'Apple Developer Documentation');  -- 2024-01-13T10:00:00Z

        -- Visits for item 3 (webkit.org) -- one visit with NULL title
        INSERT INTO history_visits (id, history_item, visit_time, title)
        VALUES (4, 3, 727005600.0, 'WebKit');

        INSERT INTO history_visits (id, history_item, visit_time, title)
        VALUES (5, 3, 726919200.0, NULL);
        ",
    )
    .unwrap();

    conn
}
