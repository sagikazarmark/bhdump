use crate::browsers::HistoryEntry;
use crate::error::Error;
use cel_interpreter::{Context, Program, Value};
use chrono::FixedOffset;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

/// A compiled `--where` expression ready to evaluate against history entries.
///
/// The expression is parsed once at startup and evaluated per entry. Available
/// fields in the expression:
///
/// | Field           | Type      | Description                                |
/// |-----------------|-----------|--------------------------------------------|
/// | `url`           | string    | The full URL                               |
/// | `domain`        | string    | Host extracted from the URL                |
/// | `title`         | string    | Page title (empty string if absent)        |
/// | `visit_count`   | uint      | Aggregated visit count (0 if absent)       |
/// | `browser`       | string    | Browser name (e.g. "chrome", "firefox")    |
/// | `profile`       | string    | Profile name                               |
/// | `visit_time`    | timestamp | When the visit occurred                    |
///
/// CEL built-in string methods are available: `contains`, `startsWith`,
/// `endsWith`, `matches` (regex), `size`.
///
/// # Examples
///
/// ```text
/// url.contains("github.com") && visit_count > 5
/// browser == "firefox" || browser == "safari"
/// title.contains("rust") && !url.matches("reddit\\.com")
/// domain == "github.com"
/// ```
pub struct WhereExpr {
    program: Program,
}

impl WhereExpr {
    /// Compile a CEL expression. Returns an error if the expression is invalid.
    pub fn compile(source: &str) -> Result<Self, Error> {
        let program = Program::compile(source).map_err(|e| Error::Expression(format!("{e}")))?;
        Ok(Self { program })
    }

    /// Returns the variables and functions referenced by this expression.
    pub fn references(&self) -> (Vec<String>, Vec<String>) {
        let refs = self.program.references();
        let mut vars: Vec<String> = refs.variables().into_iter().map(String::from).collect();
        let mut funcs: Vec<String> = refs.functions().into_iter().map(String::from).collect();
        vars.sort();
        funcs.sort();
        (vars, funcs)
    }

    /// Evaluate the expression against a history entry. Returns `true` if the
    /// entry matches, `false` otherwise, or an error if execution fails.
    pub fn matches(&self, entry: &HistoryEntry) -> Result<bool, Error> {
        let mut ctx = Context::default();

        ctx.add_variable("url", entry.url.as_str())
            .map_err(|e| Error::Expression(format!("{e}")))?;
        ctx.add_variable("domain", extract_host(&entry.url))
            .map_err(|e| Error::Expression(format!("{e}")))?;
        ctx.add_variable("title", entry.title.as_deref().unwrap_or(""))
            .map_err(|e| Error::Expression(format!("{e}")))?;
        ctx.add_variable("visit_count", entry.visit_count.unwrap_or(0))
            .map_err(|e| Error::Expression(format!("{e}")))?;
        ctx.add_variable("browser", entry.browser.as_str())
            .map_err(|e| Error::Expression(format!("{e}")))?;
        ctx.add_variable("profile", entry.profile.as_str())
            .map_err(|e| Error::Expression(format!("{e}")))?;

        // Convert DateTime<Utc> to DateTime<FixedOffset> for CEL timestamp
        let visit_time_fixed = entry
            .visit_time
            .with_timezone(&FixedOffset::east_opt(0).unwrap());
        ctx.add_variable_from_value("visit_time", Value::Timestamp(visit_time_fixed));

        let result = self
            .program
            .execute(&ctx)
            .map_err(|e| Error::Expression(format!("{e}")))?;

        match result {
            Value::Bool(b) => Ok(b),
            other => Err(Error::Expression(format!(
                "--where expression must return a boolean, got {:?}",
                other.type_of()
            ))),
        }
    }
}

impl std::fmt::Debug for WhereExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WhereExpr(<compiled>)")
    }
}

/// A sortable field on a history entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    Url,
    Title,
    VisitTime,
    VisitCount,
    Browser,
    Profile,
    Domain,
}

impl FromStr for SortField {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "url" => Ok(SortField::Url),
            "title" => Ok(SortField::Title),
            "visit_time" | "time" | "date" => Ok(SortField::VisitTime),
            "visit_count" | "count" | "visits" => Ok(SortField::VisitCount),
            "browser" => Ok(SortField::Browser),
            "profile" => Ok(SortField::Profile),
            "domain" => Ok(SortField::Domain),
            _ => Err(format!(
                "unknown sort field '{s}' (try: url, title, time, count, browser, profile, domain)"
            )),
        }
    }
}

impl fmt::Display for SortField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SortField::Url => "url",
            SortField::Title => "title",
            SortField::VisitTime => "time",
            SortField::VisitCount => "count",
            SortField::Browser => "browser",
            SortField::Profile => "profile",
            SortField::Domain => "domain",
        };
        f.write_str(s)
    }
}

/// A sort specification: a field and a direction.
#[derive(Debug, Clone, Copy)]
pub struct SortKey {
    pub field: SortField,
    pub descending: bool,
}

impl FromStr for SortKey {
    type Err = String;

    /// Parse a sort key like `"visit_count"`, `"-visit_count"` (descending),
    /// or `"+url"` (ascending, explicit).
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let s = s.trim();
        if let Some(rest) = s.strip_prefix('-') {
            Ok(SortKey {
                field: rest.parse()?,
                descending: true,
            })
        } else if let Some(rest) = s.strip_prefix('+') {
            Ok(SortKey {
                field: rest.parse()?,
                descending: false,
            })
        } else {
            Ok(SortKey {
                field: s.parse()?,
                descending: false,
            })
        }
    }
}

/// Compare two entries by the given field.
fn cmp_by_field(a: &HistoryEntry, b: &HistoryEntry, field: SortField) -> Ordering {
    match field {
        SortField::Url => a.url.cmp(&b.url),
        SortField::Title => {
            let a_title = a.title.as_deref().unwrap_or("");
            let b_title = b.title.as_deref().unwrap_or("");
            a_title.cmp(b_title)
        }
        SortField::VisitTime => a.visit_time.cmp(&b.visit_time),
        SortField::VisitCount => a.visit_count.unwrap_or(0).cmp(&b.visit_count.unwrap_or(0)),
        SortField::Browser => a.browser.as_str().cmp(b.browser.as_str()),
        SortField::Profile => a.profile.cmp(&b.profile),
        SortField::Domain => {
            let a_host = extract_host(&a.url);
            let b_host = extract_host(&b.url);
            a_host.cmp(b_host)
        }
    }
}

/// Configuration for filtering history entries.
#[derive(Debug, Default)]
pub struct FilterConfig {
    /// A compiled CEL expression for user-defined filtering.
    pub where_expr: Option<WhereExpr>,
    /// Maximum number of entries to return.
    pub limit: Option<usize>,
    /// Deduplicate by URL, keeping the most recent entry.
    pub deduplicate: bool,
    /// Include browser-internal URLs (chrome://, about:, etc.).
    pub include_internal: bool,
    /// Include noise domains (auth, tracking, redirects, etc.).
    /// When false (the default), URLs matching [`NOISE_DOMAINS`] are excluded.
    pub include_noise: bool,
    /// Sort order. When set, entries are sorted by this key after filtering.
    /// When `None`, the default order is descending by visit_time.
    pub sort: Option<SortKey>,
}

impl FilterConfig {
    /// Apply all filters to a list of history entries, consuming and returning a new list.
    pub fn apply(&self, entries: Vec<HistoryEntry>) -> Result<Vec<HistoryEntry>, Error> {
        let mut result: Vec<HistoryEntry> = Vec::with_capacity(entries.len());
        for entry in entries {
            if self.matches(&entry)? {
                result.push(entry);
            }
        }

        if self.deduplicate {
            result = deduplicate(result);
        }

        if let Some(ref sort_key) = self.sort {
            let field = sort_key.field;
            let desc = sort_key.descending;
            result.sort_by(|a, b| {
                let ord = cmp_by_field(a, b, field);
                if desc { ord.reverse() } else { ord }
            });
        }

        if let Some(limit) = self.limit {
            result.truncate(limit);
        }

        Ok(result)
    }

    /// Check if a single entry passes all filters.
    fn matches(&self, entry: &HistoryEntry) -> Result<bool, Error> {
        // Filter internal URLs unless explicitly included
        if !self.include_internal && is_internal_url(&entry.url) {
            return Ok(false);
        }

        // Filter noise domains unless explicitly included
        if !self.include_noise && is_noise_url(&entry.url) {
            return Ok(false);
        }

        // Apply user-defined CEL expression
        if let Some(ref expr) = self.where_expr {
            return expr.matches(entry);
        }

        Ok(true)
    }
}

/// Domains excluded by default as "noise" — pages that appear frequently in
/// browser history but rarely represent intentional browsing destinations.
///
/// Subdomains are matched automatically (e.g. "auth0.com" in the list also
/// matches "company.auth0.com"). Each entry matches the domain and all its
/// subdomains.
///
/// Users can include these with `--include-noise`.
pub const NOISE_DOMAINS: &[&str] = &[
    // Webmail / messaging (constant background traffic, not browsing)
    "mail.google.com",
    "outlook.live.com",
    "outlook.office.com",
    "outlook.office365.com",
    "mail.yahoo.com",
    "mail.proton.me",
    "mail.zoho.com",
    "web.whatsapp.com",
    "web.telegram.org",
    "discord.com",
    "messages.google.com",
    // Authentication / SSO
    "accounts.google.com",
    "accounts.youtube.com",
    "myaccount.google.com",
    "login.microsoftonline.com",
    "login.live.com",
    "appleid.apple.com",
    "auth0.com",
    "okta.com",
    "onelogin.com",
    // Ad / tracking redirects
    "googleadservices.com",
    "doubleclick.net",
    "googlesyndication.com",
    // URL shorteners / redirects
    "t.co",
    "bit.ly",
    "goo.gl",
    "ow.ly",
    "tinyurl.com",
    // Analytics / beacons
    "analytics.google.com",
    "www.googletagmanager.com",
    // Consent / cookie banners (often iframed)
    "consent.google.com",
    "consent.youtube.com",
    "cookiebot.com",
    // CDN / asset domains (sometimes leak into history)
    "cdn.jsdelivr.net",
    "cdnjs.cloudflare.com",
    "fonts.googleapis.com",
    "ajax.googleapis.com",
];

/// Check if a URL belongs to a noise domain.
fn is_noise_url(url: &str) -> bool {
    let host = extract_host(url);
    NOISE_DOMAINS
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

/// Check if a URL is a browser-internal URL.
fn is_internal_url(url: &str) -> bool {
    let internal_prefixes = [
        "chrome://",
        "chrome-extension://",
        "edge://",
        "brave://",
        "vivaldi://",
        "opera://",
        "about:",
        "moz-extension://",
        "file:///",
        "data:",
        "blob:",
        "javascript:",
    ];
    internal_prefixes.iter().any(|p| url.starts_with(p))
}

/// Extract the host from a URL (the part between `://` and the next `/`).
pub fn extract_host(url: &str) -> &str {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("")
}

/// Deduplicate entries by URL, keeping the most recent (first, since entries
/// are sorted by visit_time descending).
fn deduplicate(entries: Vec<HistoryEntry>) -> Vec<HistoryEntry> {
    let mut seen = HashSet::new();
    entries
        .into_iter()
        .filter(|e| seen.insert(e.url.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browsers::Browser;
    use chrono::{DateTime, TimeZone, Utc};

    fn test_entry() -> HistoryEntry {
        HistoryEntry {
            url: "https://github.com/rust-lang/rust".to_string(),
            title: Some("Rust Programming Language".to_string()),
            visit_time: Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap(),
            visit_count: Some(42),
            visit_duration_ms: None,
            browser: Browser::Chrome,
            profile: "Default".to_string(),
        }
    }

    #[test]
    fn test_is_internal_url() {
        assert!(is_internal_url("chrome://settings"));
        assert!(is_internal_url("about:blank"));
        assert!(is_internal_url("edge://newtab"));
        assert!(is_internal_url("file:///tmp/foo.html"));
        assert!(!is_internal_url("https://example.com"));
        assert!(!is_internal_url("http://localhost:3000"));
    }

    #[test]
    fn test_is_noise_url() {
        assert!(is_noise_url("https://accounts.google.com/signin"));
        assert!(is_noise_url("https://t.co/abc123"));
        assert!(is_noise_url("https://bit.ly/xyz"));
        assert!(is_noise_url(
            "https://login.microsoftonline.com/common/oauth2"
        ));
        assert!(is_noise_url(
            "https://consent.youtube.com/m?continue=https://youtube.com"
        ));
        assert!(is_noise_url(
            "https://fonts.googleapis.com/css2?family=Roboto"
        ));
        assert!(is_noise_url("https://sub.auth0.com/authorize"));
        assert!(is_noise_url("https://company.okta.com/login"));
        assert!(is_noise_url("https://mail.google.com/mail/u/0/"));
        assert!(is_noise_url("https://outlook.live.com/mail/0/inbox"));
        assert!(is_noise_url("https://web.whatsapp.com"));
        assert!(is_noise_url("https://discord.com/channels/@me"));

        assert!(!is_noise_url("https://google.com/search?q=rust"));
        assert!(!is_noise_url("https://www.google.com/search?q=rust"));
        assert!(!is_noise_url("https://youtube.com/watch?v=abc"));
        assert!(!is_noise_url("https://github.com/rust-lang/rust"));
        assert!(!is_noise_url("https://example.com"));
    }

    #[test]
    fn test_where_url_contains() {
        let expr = WhereExpr::compile(r#"url.contains("github.com")"#).unwrap();
        assert!(expr.matches(&test_entry()).unwrap());

        let expr = WhereExpr::compile(r#"url.contains("gitlab.com")"#).unwrap();
        assert!(!expr.matches(&test_entry()).unwrap());
    }

    #[test]
    fn test_where_domain_equals() {
        let expr = WhereExpr::compile(r#"domain == "github.com""#).unwrap();
        assert!(expr.matches(&test_entry()).unwrap());

        let expr = WhereExpr::compile(r#"domain == "gitlab.com""#).unwrap();
        assert!(!expr.matches(&test_entry()).unwrap());
    }

    #[test]
    fn test_where_visit_count() {
        let expr = WhereExpr::compile("visit_count > 10").unwrap();
        assert!(expr.matches(&test_entry()).unwrap());

        let expr = WhereExpr::compile("visit_count > 100").unwrap();
        assert!(!expr.matches(&test_entry()).unwrap());
    }

    #[test]
    fn test_where_browser_equals() {
        let expr = WhereExpr::compile(r#"browser == "chrome""#).unwrap();
        assert!(expr.matches(&test_entry()).unwrap());

        let expr = WhereExpr::compile(r#"browser == "firefox""#).unwrap();
        assert!(!expr.matches(&test_entry()).unwrap());
    }

    #[test]
    fn test_where_title_contains() {
        let expr = WhereExpr::compile(r#"title.contains("Rust")"#).unwrap();
        assert!(expr.matches(&test_entry()).unwrap());
    }

    #[test]
    fn test_where_combined_and_or() {
        let expr = WhereExpr::compile(r#"url.contains("github") && visit_count > 10"#).unwrap();
        assert!(expr.matches(&test_entry()).unwrap());

        let expr = WhereExpr::compile(r#"browser == "firefox" || browser == "chrome""#).unwrap();
        assert!(expr.matches(&test_entry()).unwrap());
    }

    #[test]
    fn test_where_matches_regex() {
        let expr = WhereExpr::compile(r#"url.matches("rust-lang.*rust")"#).unwrap();
        assert!(expr.matches(&test_entry()).unwrap());
    }

    #[test]
    fn test_where_negation() {
        let expr = WhereExpr::compile(r#"!url.contains("reddit")"#).unwrap();
        assert!(expr.matches(&test_entry()).unwrap());
    }

    #[test]
    fn test_where_compile_error() {
        assert!(WhereExpr::compile("invalid $$$ expression").is_err());
    }

    #[test]
    fn test_where_non_boolean_result() {
        let expr = WhereExpr::compile(r#"url + "foo""#).unwrap();
        assert!(expr.matches(&test_entry()).is_err());
    }

    // ------------------------------------------------------------------
    // SortKey parsing tests
    // ------------------------------------------------------------------

    #[test]
    fn test_sort_key_ascending() {
        let key: SortKey = "url".parse().unwrap();
        assert_eq!(key.field, SortField::Url);
        assert!(!key.descending);
    }

    #[test]
    fn test_sort_key_ascending_explicit() {
        let key: SortKey = "+visit_count".parse().unwrap();
        assert_eq!(key.field, SortField::VisitCount);
        assert!(!key.descending);
    }

    #[test]
    fn test_sort_key_descending() {
        let key: SortKey = "-count".parse().unwrap();
        assert_eq!(key.field, SortField::VisitCount);
        assert!(key.descending);
    }

    #[test]
    fn test_sort_key_aliases() {
        assert_eq!(
            "time".parse::<SortKey>().unwrap().field,
            SortField::VisitTime
        );
        assert_eq!(
            "date".parse::<SortKey>().unwrap().field,
            SortField::VisitTime
        );
        assert_eq!(
            "visits".parse::<SortKey>().unwrap().field,
            SortField::VisitCount
        );
        assert_eq!(
            "domain".parse::<SortKey>().unwrap().field,
            SortField::Domain
        );
    }

    #[test]
    fn test_sort_key_invalid() {
        assert!("nonexistent".parse::<SortKey>().is_err());
    }

    // ------------------------------------------------------------------
    // Sort ordering tests
    // ------------------------------------------------------------------

    #[test]
    fn test_sort_by_url_ascending() {
        let entries = vec![
            HistoryEntry {
                url: "https://zebra.com".to_string(),
                title: None,
                visit_time: Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap(),
                visit_count: Some(1),
                visit_duration_ms: None,
                browser: Browser::Chrome,
                profile: "Default".to_string(),
            },
            HistoryEntry {
                url: "https://alpha.com".to_string(),
                title: None,
                visit_time: Utc.with_ymd_and_hms(2024, 1, 14, 10, 0, 0).unwrap(),
                visit_count: Some(1),
                visit_duration_ms: None,
                browser: Browser::Chrome,
                profile: "Default".to_string(),
            },
        ];

        let filter = FilterConfig {
            sort: Some(SortKey {
                field: SortField::Url,
                descending: false,
            }),
            ..Default::default()
        };
        let result = filter.apply(entries).unwrap();
        assert_eq!(result[0].url, "https://alpha.com");
        assert_eq!(result[1].url, "https://zebra.com");
    }

    // ------------------------------------------------------------------
    // extract_host tests
    // ------------------------------------------------------------------

    #[test]
    fn test_extract_host_https() {
        assert_eq!(extract_host("https://example.com/path"), "example.com");
    }

    #[test]
    fn test_extract_host_http() {
        assert_eq!(extract_host("http://example.com/path"), "example.com");
    }

    #[test]
    fn test_extract_host_with_port() {
        assert_eq!(
            extract_host("https://example.com:8080/path"),
            "example.com:8080"
        );
    }

    #[test]
    fn test_extract_host_no_path() {
        assert_eq!(extract_host("https://example.com"), "example.com");
    }

    #[test]
    fn test_extract_host_trailing_slash() {
        assert_eq!(extract_host("https://example.com/"), "example.com");
    }

    #[test]
    fn test_extract_host_no_protocol() {
        assert_eq!(extract_host("not-a-url"), "");
    }

    #[test]
    fn test_extract_host_empty() {
        assert_eq!(extract_host(""), "");
    }

    #[test]
    fn test_extract_host_subdomain() {
        assert_eq!(
            extract_host("https://sub.domain.example.com/foo"),
            "sub.domain.example.com"
        );
    }

    #[test]
    fn test_extract_host_with_query() {
        assert_eq!(
            extract_host("https://example.com/path?q=1&b=2"),
            "example.com"
        );
    }

    #[test]
    fn test_extract_host_with_userinfo() {
        // URLs with user:pass@ -- extract_host returns everything between :// and /
        assert_eq!(
            extract_host("https://user:pass@example.com/path"),
            "user:pass@example.com"
        );
    }

    // ------------------------------------------------------------------
    // CEL visit_time tests
    // ------------------------------------------------------------------

    #[test]
    fn test_where_visit_time_comparison() {
        let entry = test_entry(); // visit_time is 2024-01-15T10:00:00Z
        let expr = WhereExpr::compile(r#"visit_time > timestamp("2024-01-01T00:00:00Z")"#).unwrap();
        assert!(expr.matches(&entry).unwrap());

        let expr = WhereExpr::compile(r#"visit_time > timestamp("2025-01-01T00:00:00Z")"#).unwrap();
        assert!(!expr.matches(&entry).unwrap());
    }

    #[test]
    fn test_where_visit_time_before() {
        let entry = test_entry(); // visit_time is 2024-01-15T10:00:00Z
        let expr = WhereExpr::compile(r#"visit_time < timestamp("2024-02-01T00:00:00Z")"#).unwrap();
        assert!(expr.matches(&entry).unwrap());

        let expr = WhereExpr::compile(r#"visit_time < timestamp("2024-01-01T00:00:00Z")"#).unwrap();
        assert!(!expr.matches(&entry).unwrap());
    }

    // ------------------------------------------------------------------
    // WhereExpr::references tests
    // ------------------------------------------------------------------

    #[test]
    fn test_where_references_single_variable() {
        let expr = WhereExpr::compile(r#"url.contains("github")"#).unwrap();
        let (vars, funcs) = expr.references();
        assert!(vars.contains(&"url".to_string()));
        assert!(funcs.contains(&"contains".to_string()));
    }

    #[test]
    fn test_where_references_multiple_variables() {
        let expr = WhereExpr::compile(r#"domain == "github.com" && visit_count > 5"#).unwrap();
        let (vars, _funcs) = expr.references();
        assert!(vars.contains(&"domain".to_string()));
        assert!(vars.contains(&"visit_count".to_string()));
    }

    #[test]
    fn test_where_references_operators_as_functions() {
        // CEL treats operators like `>` as function references internally
        let expr = WhereExpr::compile(r#"visit_count > 10"#).unwrap();
        let (vars, funcs) = expr.references();
        assert!(vars.contains(&"visit_count".to_string()));
        assert!(funcs.contains(&"_>_".to_string()));
    }

    // ------------------------------------------------------------------
    // SortField::Display tests
    // ------------------------------------------------------------------

    #[test]
    fn test_sort_field_display() {
        assert_eq!(SortField::Url.to_string(), "url");
        assert_eq!(SortField::Title.to_string(), "title");
        assert_eq!(SortField::VisitTime.to_string(), "time");
        assert_eq!(SortField::VisitCount.to_string(), "count");
        assert_eq!(SortField::Browser.to_string(), "browser");
        assert_eq!(SortField::Profile.to_string(), "profile");
        assert_eq!(SortField::Domain.to_string(), "domain");
    }

    // ------------------------------------------------------------------
    // Additional sort ordering tests (Title, VisitTime, Browser, Profile, Domain)
    // ------------------------------------------------------------------

    fn make_entry(
        url: &str,
        title: Option<&str>,
        visit_time: DateTime<Utc>,
        visit_count: Option<u64>,
        browser: Browser,
        profile: &str,
    ) -> HistoryEntry {
        HistoryEntry {
            url: url.to_string(),
            title: title.map(String::from),
            visit_time,
            visit_count,
            visit_duration_ms: None,
            browser,
            profile: profile.to_string(),
        }
    }

    #[test]
    fn test_sort_by_title_ascending() {
        let entries = vec![
            make_entry(
                "https://z.com",
                Some("Zebra"),
                Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap(),
                Some(1),
                Browser::Chrome,
                "Default",
            ),
            make_entry(
                "https://a.com",
                Some("Alpha"),
                Utc.with_ymd_and_hms(2024, 1, 14, 10, 0, 0).unwrap(),
                Some(1),
                Browser::Chrome,
                "Default",
            ),
        ];

        let filter = FilterConfig {
            sort: Some(SortKey {
                field: SortField::Title,
                descending: false,
            }),
            ..Default::default()
        };
        let result = filter.apply(entries).unwrap();
        assert_eq!(result[0].title.as_deref(), Some("Alpha"));
        assert_eq!(result[1].title.as_deref(), Some("Zebra"));
    }

    #[test]
    fn test_sort_by_title_with_none() {
        let entries = vec![
            make_entry(
                "https://a.com",
                Some("Beta"),
                Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap(),
                Some(1),
                Browser::Chrome,
                "Default",
            ),
            make_entry(
                "https://b.com",
                None,
                Utc.with_ymd_and_hms(2024, 1, 14, 10, 0, 0).unwrap(),
                Some(1),
                Browser::Chrome,
                "Default",
            ),
        ];

        let filter = FilterConfig {
            sort: Some(SortKey {
                field: SortField::Title,
                descending: false,
            }),
            ..Default::default()
        };
        let result = filter.apply(entries).unwrap();
        // None title treated as "" which sorts before "Beta"
        assert_eq!(result[0].title, None);
        assert_eq!(result[1].title.as_deref(), Some("Beta"));
    }

    #[test]
    fn test_sort_by_visit_time_ascending() {
        let early = Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap();
        let late = Utc.with_ymd_and_hms(2024, 6, 15, 10, 0, 0).unwrap();
        let entries = vec![
            make_entry(
                "https://late.com",
                None,
                late,
                Some(1),
                Browser::Chrome,
                "Default",
            ),
            make_entry(
                "https://early.com",
                None,
                early,
                Some(1),
                Browser::Chrome,
                "Default",
            ),
        ];

        let filter = FilterConfig {
            sort: Some(SortKey {
                field: SortField::VisitTime,
                descending: false,
            }),
            ..Default::default()
        };
        let result = filter.apply(entries).unwrap();
        assert_eq!(result[0].url, "https://early.com");
        assert_eq!(result[1].url, "https://late.com");
    }

    #[test]
    fn test_sort_by_visit_time_descending() {
        let early = Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap();
        let late = Utc.with_ymd_and_hms(2024, 6, 15, 10, 0, 0).unwrap();
        let entries = vec![
            make_entry(
                "https://early.com",
                None,
                early,
                Some(1),
                Browser::Chrome,
                "Default",
            ),
            make_entry(
                "https://late.com",
                None,
                late,
                Some(1),
                Browser::Chrome,
                "Default",
            ),
        ];

        let filter = FilterConfig {
            sort: Some(SortKey {
                field: SortField::VisitTime,
                descending: true,
            }),
            ..Default::default()
        };
        let result = filter.apply(entries).unwrap();
        assert_eq!(result[0].url, "https://late.com");
        assert_eq!(result[1].url, "https://early.com");
    }

    #[test]
    fn test_sort_by_browser_ascending() {
        let t = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        let entries = vec![
            make_entry(
                "https://a.com",
                None,
                t,
                Some(1),
                Browser::Safari,
                "Default",
            ),
            make_entry(
                "https://b.com",
                None,
                t,
                Some(1),
                Browser::Chrome,
                "Default",
            ),
            make_entry(
                "https://c.com",
                None,
                t,
                Some(1),
                Browser::Firefox,
                "Default",
            ),
        ];

        let filter = FilterConfig {
            sort: Some(SortKey {
                field: SortField::Browser,
                descending: false,
            }),
            ..Default::default()
        };
        let result = filter.apply(entries).unwrap();
        // chrome < firefox < safari (lexicographic on as_str())
        assert_eq!(result[0].browser, Browser::Chrome);
        assert_eq!(result[1].browser, Browser::Firefox);
        assert_eq!(result[2].browser, Browser::Safari);
    }

    #[test]
    fn test_sort_by_profile_ascending() {
        let t = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        let entries = vec![
            make_entry("https://a.com", None, t, Some(1), Browser::Chrome, "Work"),
            make_entry(
                "https://b.com",
                None,
                t,
                Some(1),
                Browser::Chrome,
                "Default",
            ),
            make_entry(
                "https://c.com",
                None,
                t,
                Some(1),
                Browser::Chrome,
                "Personal",
            ),
        ];

        let filter = FilterConfig {
            sort: Some(SortKey {
                field: SortField::Profile,
                descending: false,
            }),
            ..Default::default()
        };
        let result = filter.apply(entries).unwrap();
        assert_eq!(result[0].profile, "Default");
        assert_eq!(result[1].profile, "Personal");
        assert_eq!(result[2].profile, "Work");
    }

    #[test]
    fn test_sort_by_domain_ascending() {
        let t = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        let entries = vec![
            make_entry(
                "https://zebra.com/path",
                None,
                t,
                Some(1),
                Browser::Chrome,
                "Default",
            ),
            make_entry(
                "https://alpha.org/page",
                None,
                t,
                Some(1),
                Browser::Chrome,
                "Default",
            ),
            make_entry(
                "https://middle.net/docs",
                None,
                t,
                Some(1),
                Browser::Chrome,
                "Default",
            ),
        ];

        let filter = FilterConfig {
            sort: Some(SortKey {
                field: SortField::Domain,
                descending: false,
            }),
            ..Default::default()
        };
        let result = filter.apply(entries).unwrap();
        assert_eq!(result[0].url, "https://alpha.org/page");
        assert_eq!(result[1].url, "https://middle.net/docs");
        assert_eq!(result[2].url, "https://zebra.com/path");
    }

    // ------------------------------------------------------------------
    // Standalone deduplicate tests
    // ------------------------------------------------------------------

    #[test]
    fn test_deduplicate_keeps_first_occurrence() {
        let entries = vec![
            make_entry(
                "https://example.com",
                Some("First"),
                Utc.with_ymd_and_hms(2024, 6, 1, 10, 0, 0).unwrap(),
                Some(5),
                Browser::Chrome,
                "Default",
            ),
            make_entry(
                "https://example.com",
                Some("Second"),
                Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap(),
                Some(3),
                Browser::Chrome,
                "Default",
            ),
        ];

        let result = deduplicate(entries);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title.as_deref(), Some("First"));
    }

    #[test]
    fn test_deduplicate_different_urls_kept() {
        let t = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        let entries = vec![
            make_entry(
                "https://a.com",
                None,
                t,
                Some(1),
                Browser::Chrome,
                "Default",
            ),
            make_entry(
                "https://b.com",
                None,
                t,
                Some(1),
                Browser::Chrome,
                "Default",
            ),
        ];

        let result = deduplicate(entries);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_deduplicate_same_url_different_browsers() {
        let t = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        let entries = vec![
            make_entry(
                "https://example.com",
                None,
                t,
                Some(1),
                Browser::Chrome,
                "Default",
            ),
            make_entry(
                "https://example.com",
                None,
                t,
                Some(1),
                Browser::Firefox,
                "default-release",
            ),
        ];

        // Dedup is by URL only, so second entry is dropped regardless of browser
        let result = deduplicate(entries);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].browser, Browser::Chrome);
    }

    #[test]
    fn test_deduplicate_empty() {
        let result = deduplicate(Vec::new());
        assert!(result.is_empty());
    }

    #[test]
    fn test_sort_by_visit_count_descending() {
        let entries = vec![
            HistoryEntry {
                url: "https://low.com".to_string(),
                title: None,
                visit_time: Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap(),
                visit_count: Some(2),
                visit_duration_ms: None,
                browser: Browser::Chrome,
                profile: "Default".to_string(),
            },
            HistoryEntry {
                url: "https://high.com".to_string(),
                title: None,
                visit_time: Utc.with_ymd_and_hms(2024, 1, 14, 10, 0, 0).unwrap(),
                visit_count: Some(100),
                visit_duration_ms: None,
                browser: Browser::Chrome,
                profile: "Default".to_string(),
            },
        ];

        let filter = FilterConfig {
            sort: Some(SortKey {
                field: SortField::VisitCount,
                descending: true,
            }),
            ..Default::default()
        };
        let result = filter.apply(entries).unwrap();
        assert_eq!(result[0].url, "https://high.com");
        assert_eq!(result[1].url, "https://low.com");
    }
}
