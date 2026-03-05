use crate::browsers::{BrowserKind, HistoryEntry};
use regex::Regex;
use std::collections::HashSet;

/// Configuration for filtering history entries.
///
/// All filters are AND-combined: an entry must pass all active filters.
#[derive(Debug, Default)]
pub struct FilterConfig {
    /// Include only URLs matching this regex.
    pub url_pattern: Option<Regex>,
    /// Exclude URLs matching this regex.
    pub url_exclude: Option<Regex>,
    /// Include only entries with titles matching this regex.
    pub title_pattern: Option<Regex>,
    /// Include only entries from these domains.
    pub domains: Option<Vec<String>>,
    /// Minimum visit count (only applies to aggregated entries).
    pub min_visit_count: Option<u64>,
    /// Maximum number of entries to return.
    pub limit: Option<usize>,
    /// Deduplicate by URL, keeping the most recent entry.
    pub deduplicate: bool,
    /// Include browser-internal URLs (chrome://, about:, etc.).
    pub include_internal: bool,
    /// Include noise domains (auth, tracking, redirects, etc.).
    /// When false (the default), URLs matching [`NOISE_DOMAINS`] are excluded.
    pub include_noise: bool,
}

impl FilterConfig {
    /// Apply all filters to a list of history entries, consuming and returning a new list.
    pub fn apply(&self, entries: Vec<HistoryEntry>) -> Vec<HistoryEntry> {
        let mut result: Vec<HistoryEntry> =
            entries.into_iter().filter(|e| self.matches(e)).collect();

        if self.deduplicate {
            result = deduplicate(result);
        }

        if let Some(limit) = self.limit {
            result.truncate(limit);
        }

        result
    }

    /// Check if a single entry passes all filters.
    fn matches(&self, entry: &HistoryEntry) -> bool {
        // Filter internal URLs unless explicitly included
        if !self.include_internal && is_internal_url(&entry.url) {
            return false;
        }

        // Filter noise domains unless explicitly included
        if !self.include_noise && is_noise_url(&entry.url) {
            return false;
        }

        if let Some(ref pattern) = self.url_pattern
            && !pattern.is_match(&entry.url)
        {
            return false;
        }

        if let Some(ref exclude) = self.url_exclude
            && exclude.is_match(&entry.url)
        {
            return false;
        }

        if let Some(ref pattern) = self.title_pattern {
            match &entry.title {
                Some(title) => {
                    if !pattern.is_match(title) {
                        return false;
                    }
                }
                None => return false, // no title to match against
            }
        }

        if let Some(ref domains) = self.domains
            && !matches_domain(&entry.url, domains)
        {
            return false;
        }

        if let Some(min) = self.min_visit_count
            && let Some(count) = entry.visit_count
            && count < min
        {
            return false;
        }

        true
    }
}

/// Filter browser sources by inclusion/exclusion lists.
pub fn filter_browsers(
    browsers: &[BrowserKind],
    include: &Option<Vec<BrowserKind>>,
    exclude: &Option<Vec<BrowserKind>>,
) -> Vec<BrowserKind> {
    browsers
        .iter()
        .copied()
        .filter(|b| {
            if let Some(inc) = include
                && !inc.contains(b)
            {
                return false;
            }
            if let Some(exc) = exclude
                && exc.contains(b)
            {
                return false;
            }
            true
        })
        .collect()
}

/// Domains excluded by default as "noise" — pages that appear frequently in
/// browser history but rarely represent intentional browsing destinations.
///
/// Subdomains are matched automatically (e.g. "google.com" in the list also
/// matches "mail.google.com"). Each entry matches the domain and all its
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
fn extract_host(url: &str) -> &str {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("")
}

/// Check if a URL's domain matches any of the given domains.
fn matches_domain(url: &str, domains: &[String]) -> bool {
    let host = extract_host(url);
    domains
        .iter()
        .any(|d| host == d.as_str() || host.ends_with(&format!(".{d}")))
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
    fn test_matches_domain() {
        let domains = vec!["example.com".to_string(), "rust-lang.org".to_string()];
        assert!(matches_domain("https://example.com/page", &domains));
        assert!(matches_domain("https://www.example.com/page", &domains));
        assert!(matches_domain("https://rust-lang.org/learn", &domains));
        assert!(!matches_domain("https://other.com", &domains));
    }

    #[test]
    fn test_filter_browsers_include() {
        let all = vec![
            BrowserKind::Chrome,
            BrowserKind::Firefox,
            BrowserKind::Safari,
        ];
        let include = Some(vec![BrowserKind::Chrome, BrowserKind::Firefox]);
        let result = filter_browsers(&all, &include, &None);
        assert_eq!(result, vec![BrowserKind::Chrome, BrowserKind::Firefox]);
    }

    #[test]
    fn test_is_noise_url() {
        // Exact domain matches
        assert!(is_noise_url("https://accounts.google.com/signin"));
        assert!(is_noise_url("https://t.co/abc123"));
        assert!(is_noise_url("https://bit.ly/xyz"));
        assert!(is_noise_url("https://login.microsoftonline.com/common/oauth2"));
        assert!(is_noise_url("https://consent.youtube.com/m?continue=https://youtube.com"));
        assert!(is_noise_url("https://fonts.googleapis.com/css2?family=Roboto"));

        // Subdomain matches
        assert!(is_noise_url("https://sub.auth0.com/authorize"));
        assert!(is_noise_url("https://company.okta.com/login"));

        // Webmail / messaging
        assert!(is_noise_url("https://mail.google.com/mail/u/0/"));
        assert!(is_noise_url("https://outlook.live.com/mail/0/inbox"));
        assert!(is_noise_url("https://web.whatsapp.com"));
        assert!(is_noise_url("https://discord.com/channels/@me"));

        // Should NOT be noise
        assert!(!is_noise_url("https://google.com/search?q=rust"));
        assert!(!is_noise_url("https://www.google.com/search?q=rust"));
        assert!(!is_noise_url("https://youtube.com/watch?v=abc"));
        assert!(!is_noise_url("https://github.com/rust-lang/rust"));
        assert!(!is_noise_url("https://example.com"));
    }

    #[test]
    fn test_filter_browsers_exclude() {
        let all = vec![
            BrowserKind::Chrome,
            BrowserKind::Firefox,
            BrowserKind::Safari,
        ];
        let exclude = Some(vec![BrowserKind::Safari]);
        let result = filter_browsers(&all, &None, &exclude);
        assert_eq!(result, vec![BrowserKind::Chrome, BrowserKind::Firefox]);
    }
}
