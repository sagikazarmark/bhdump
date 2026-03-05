use bhdump::browsers::{self, BrowserKind};
use bhdump::filter::FilterConfig;
use bhdump::format::{self, OutputFormat};
use bhdump::timestamp;
use clap::Parser;
use std::process::ExitCode;

/// Export browser history in JSON, CSV, and other formats.
#[derive(Parser, Debug)]
#[command(name = "bhdump", version, about)]
struct Cli {
    /// Include only these browsers (can be repeated)
    #[arg(short, long = "browser", value_parser = parse_browser)]
    browsers: Vec<BrowserKind>,

    /// Exclude these browsers (can be repeated)
    #[arg(long = "exclude-browser", value_parser = parse_browser)]
    exclude_browsers: Vec<BrowserKind>,

    /// Include only these profile names (can be repeated)
    #[arg(short, long = "profile")]
    profiles: Vec<String>,

    /// Exclude these profile names (can be repeated)
    #[arg(long = "exclude-profile")]
    exclude_profiles: Vec<String>,

    /// Output format: json, jsonl, csv, tsv
    #[arg(short, long, default_value = "json", value_parser = parse_format)]
    format: OutputFormat,

    /// Output file (default: stdout)
    #[arg(short, long)]
    output: Option<String>,

    /// Only entries after this time (ISO 8601 or relative: 7d, 2w, 3mo, 1y)
    #[arg(long)]
    since: Option<String>,

    /// Only entries before this time (ISO 8601 or relative)
    #[arg(long)]
    before: Option<String>,

    /// Include only URLs matching this regex
    #[arg(long)]
    url: Option<String>,

    /// Exclude URLs matching this regex
    #[arg(long)]
    exclude_url: Option<String>,

    /// Include only these domains (can be repeated)
    #[arg(long)]
    domain: Vec<String>,

    /// Include only titles matching this regex
    #[arg(long)]
    title: Option<String>,

    /// Minimum visit count
    #[arg(long)]
    min_visits: Option<u64>,

    /// Maximum number of entries to return
    #[arg(long)]
    limit: Option<usize>,

    /// Deduplicate by URL (keep most recent)
    #[arg(long)]
    dedup: bool,

    /// Include browser-internal URLs (chrome://, about:, etc.)
    #[arg(long)]
    include_internal: bool,

    /// Include noise domains (auth, tracking, redirects, CDNs, etc.)
    #[arg(long)]
    include_noise: bool,

    /// Output individual visits instead of per-URL summary
    #[arg(long)]
    visits: bool,

    /// List detected browsers and profiles, then exit
    #[arg(long)]
    list_browsers: bool,

    /// Increase verbosity
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

fn parse_browser(s: &str) -> Result<BrowserKind, String> {
    s.parse()
}

fn parse_format(s: &str) -> Result<OutputFormat, String> {
    s.parse()
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Discover browsers
    let all_sources = browsers::discover();

    if cli.list_browsers {
        if all_sources.is_empty() {
            eprintln!("No browsers detected.");
            return ExitCode::from(1);
        }
        for source in &all_sources {
            println!(
                "{}\t{}\t{}",
                source.browser,
                source.profile,
                source.db_path.display()
            );
        }
        return ExitCode::SUCCESS;
    }

    // Filter sources by browser include/exclude
    let sources: Vec<_> = all_sources
        .into_iter()
        .filter(|s| {
            // Browser include/exclude
            if !cli.browsers.is_empty() && !cli.browsers.contains(&s.browser) {
                return false;
            }
            if cli.exclude_browsers.contains(&s.browser) {
                return false;
            }
            // Profile include/exclude
            if !cli.profiles.is_empty() && !cli.profiles.contains(&s.profile) {
                return false;
            }
            if cli.exclude_profiles.contains(&s.profile) {
                return false;
            }
            true
        })
        .collect();

    if sources.is_empty() {
        eprintln!("No matching browsers found.");
        return ExitCode::from(1);
    }

    // Parse date filters
    let since = match cli.since.as_deref().map(timestamp::parse_user_datetime) {
        Some(Ok(dt)) => Some(dt),
        Some(Err(e)) => {
            eprintln!("Error parsing --since: {e}");
            return ExitCode::from(2);
        }
        None => None,
    };

    let before = match cli.before.as_deref().map(timestamp::parse_user_datetime) {
        Some(Ok(dt)) => Some(dt),
        Some(Err(e)) => {
            eprintln!("Error parsing --before: {e}");
            return ExitCode::from(2);
        }
        None => None,
    };

    // Read history from all sources
    let (entries, errors) = browsers::read_all(&sources, since, before, cli.visits);

    // Report errors on stderr
    for err in &errors {
        eprintln!("Warning: {err}");
    }

    if entries.is_empty() && errors.len() == sources.len() {
        eprintln!("All browsers failed to read.");
        return ExitCode::from(1);
    }

    // Build filter config
    let filter_config = match build_filter(&cli) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::from(2);
        }
    };

    // Apply filters
    let filtered = filter_config.apply(entries);

    // Write output
    let result = if let Some(ref path) = cli.output {
        let file = match std::fs::File::create(path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Error creating output file: {e}");
                return ExitCode::from(1);
            }
        };
        let mut writer = std::io::BufWriter::new(file);
        format::write_entries(&mut writer, &filtered, cli.format)
    } else {
        let mut stdout = std::io::stdout().lock();
        format::write_entries(&mut stdout, &filtered, cli.format)
    };

    if let Err(e) = result {
        eprintln!("Error writing output: {e}");
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}

fn build_filter(cli: &Cli) -> Result<FilterConfig, String> {
    let url_pattern = cli
        .url
        .as_deref()
        .map(regex::Regex::new)
        .transpose()
        .map_err(|e| format!("Invalid --url regex: {e}"))?;

    let url_exclude = cli
        .exclude_url
        .as_deref()
        .map(regex::Regex::new)
        .transpose()
        .map_err(|e| format!("Invalid --exclude-url regex: {e}"))?;

    let title_pattern = cli
        .title
        .as_deref()
        .map(regex::Regex::new)
        .transpose()
        .map_err(|e| format!("Invalid --title regex: {e}"))?;

    let domains = if cli.domain.is_empty() {
        None
    } else {
        Some(cli.domain.clone())
    };

    Ok(FilterConfig {
        url_pattern,
        url_exclude,
        title_pattern,
        domains,
        min_visit_count: cli.min_visits,
        limit: cli.limit,
        deduplicate: cli.dedup,
        include_internal: cli.include_internal,
        include_noise: cli.include_noise,
    })
}
