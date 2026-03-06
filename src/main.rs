use bhdump::browsers::{self, BrowserKind};
use bhdump::filter::{FilterConfig, SortKey, WhereExpr};
use bhdump::format::{self, OutputFormat};
use bhdump::timestamp;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use std::process::ExitCode;

/// Export browser history in JSON, CSV, and other formats.
///
/// When no subcommand is given, bhdump exports history (same as "bhdump dump").
#[derive(Parser, Debug)]
#[command(name = "bhdump", version, about, after_long_help = FILTER_HELP)]
#[command(args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[command(flatten)]
    dump: DumpArgs,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Export browser history
    #[command(after_long_help = FILTER_HELP)]
    Dump(DumpArgs),

    /// List detected browsers and profiles
    Browsers,

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: Shell,
    },

    /// Validate a CEL filter expression
    Validate {
        /// CEL expression to validate (e.g. 'url.contains("github")')
        #[arg(value_name = "EXPR")]
        expression: String,
    },
}

#[derive(Parser, Debug, Default)]
struct DumpArgs {
    /// Include only these browsers (can be repeated)
    #[arg(short, long = "browser", value_parser = parse_browser)]
    browsers: Vec<BrowserKind>,

    /// Include only these profiles (can be repeated, case-insensitive)
    #[arg(short, long = "profile")]
    profiles: Vec<String>,

    /// Output format: json, jsonl, csv, tsv
    #[arg(short, long, default_value = "json", value_parser = parse_format)]
    format: OutputFormat,

    /// Output file (default: stdout)
    #[arg(short, long)]
    output: Option<String>,

    /// Only entries after this time (ISO 8601, relative, or natural language)
    #[arg(long)]
    since: Option<String>,

    /// Only entries before this time (ISO 8601, relative, or natural language)
    #[arg(long)]
    before: Option<String>,

    /// Filter entries with a CEL expression (e.g. 'url.contains("github")')
    #[arg(short, long = "where", value_name = "EXPR")]
    where_expr: Option<String>,

    /// Sort order: field name, prefix with - for descending (e.g. -visit_count)
    #[arg(short, long, value_parser = parse_sort)]
    sort: Option<SortKey>,

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

    /// Increase verbosity
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Shorthand for --since (e.g. "today", "3 days ago", "last friday", "7d")
    #[arg(value_name = "SINCE")]
    since_positional: Option<String>,
}

fn parse_browser(s: &str) -> Result<BrowserKind, String> {
    s.parse()
}

fn parse_format(s: &str) -> Result<OutputFormat, String> {
    s.parse()
}

fn parse_sort(s: &str) -> Result<SortKey, String> {
    s.parse()
}

const FILTER_HELP: &str = "\
\x1b[1;4mFilter Expressions\x1b[0m

  The --where flag accepts an expression in CEL (Common Expression Language)
  that is evaluated against each history entry. Only entries where the
  expression returns true are included in the output.

  \x1b[1mAvailable fields:\x1b[0m

    url           string     Full URL of the page
    domain        string     Hostname extracted from the URL
    title         string     Page title (empty string if absent)
    visit_count   int        Aggregate visit count (0 if absent)
    browser       string     Browser name (chrome, firefox, safari, ...)
    profile       string     Profile name
    visit_time    timestamp  When the page was visited

  \x1b[1mString methods:\x1b[0m

    s.contains(\"sub\")        true if s contains the substring
    s.startsWith(\"prefix\")   true if s starts with the prefix
    s.endsWith(\"suffix\")     true if s ends with the suffix
    s.matches(\"regex\")       true if s matches the regular expression
    s.size()                 length of the string

  \x1b[1mOperators:\x1b[0m

    ==  !=  <  <=  >  >=    comparison
    &&  ||  !               logical and, or, not
    +                       string concatenation
    in                      membership (e.g. browser in [\"chrome\", \"edge\"])

  \x1b[1mTimestamp functions:\x1b[0m

    timestamp(\"2024-01-15T00:00:00Z\")   parse an RFC 3339 timestamp

  \x1b[1mExamples:\x1b[0m

    --where 'url.contains(\"github.com\")'
    --where 'domain == \"github.com\"'
    --where 'title.matches(\"(?i)rust\") && visit_count > 5'
    --where 'browser == \"firefox\" || browser == \"safari\"'
    --where '!url.matches(\"reddit\\\\.com\")'
    --where 'visit_time > timestamp(\"2024-06-01T00:00:00Z\")'

  Use \"bhdump validate <EXPR>\" to check an expression without running a query.
  See https://cel-spec.dev for the full CEL specification.

\x1b[1;4mSort Order\x1b[0m

  The --sort flag takes a field name, optionally prefixed with - for descending
  or + for ascending (ascending is the default).

  \x1b[1mSort fields:\x1b[0m  url, title, time, count, browser, profile, domain

  \x1b[1mExamples:\x1b[0m

    --sort url                 alphabetical by URL
    --sort -count              most-visited first
    --sort domain              group by domain
    --sort -time               most recent first (same as default)

  Without --sort, entries are ordered by visit_time descending (most recent
  first). Sorting is applied after filtering but before --limit.

\x1b[1;4mTime Shorthands\x1b[0m

  A positional argument can be used as shorthand for --since. All of these
  also work with the --since and --before flags.

  \x1b[1mKeywords:\x1b[0m

    bhdump today               entries from today (midnight UTC)
    bhdump yesterday           entries from yesterday onward

  \x1b[1mCompact:\x1b[0m

    bhdump 7d                  7 days ago
    bhdump 2w                  2 weeks ago
    bhdump 3mo                 3 months ago
    bhdump 1y                  1 year ago
    bhdump 12h                 12 hours ago

  \x1b[1mNatural language:\x1b[0m

    bhdump last-week           last 7 days (midnight UTC)
    bhdump last-month          last 30 days (midnight UTC)
    bhdump \"last friday\"       last Friday
    bhdump \"3 days ago\"        3 days ago
    bhdump \"2 hours ago\"       2 hours ago
    bhdump \"next april\"        April 1 of next year (if April has passed)

  \x1b[1mAbsolute dates:\x1b[0m

    bhdump 2024-01-01          January 1, 2024
    bhdump \"April 1, 2024\"     same, with month name
    bhdump 2024-01-15T10:30:00Z  ISO 8601 with time";

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Completions { shell }) => cmd_completions(shell),
        Some(Command::Browsers) => cmd_browsers(),
        Some(Command::Validate { expression }) => cmd_validate(&expression),
        Some(Command::Dump(args)) => cmd_dump(args),
        None => cmd_dump(cli.dump),
    }
}

fn cmd_completions(shell: Shell) -> ExitCode {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "bhdump", &mut std::io::stdout());
    ExitCode::SUCCESS
}

fn cmd_browsers() -> ExitCode {
    let all_sources = browsers::discover();

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

    ExitCode::SUCCESS
}

const KNOWN_VARIABLES: &[&str] = &[
    "browser",
    "domain",
    "profile",
    "title",
    "url",
    "visit_count",
    "visit_time",
];

fn cmd_validate(expression: &str) -> ExitCode {
    let compiled = match WhereExpr::compile(expression) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Invalid expression: {e}");
            return ExitCode::from(2);
        }
    };

    let (vars, _funcs) = compiled.references();
    let unknown: Vec<_> = vars
        .iter()
        .filter(|v| !KNOWN_VARIABLES.contains(&v.as_str()))
        .collect();

    if unknown.is_empty() {
        println!("Expression is valid.");
    } else {
        println!("Expression is valid, but references unknown variables:");
        for var in &unknown {
            eprintln!(
                "  Warning: unknown variable \"{var}\" (available: {})",
                KNOWN_VARIABLES.join(", ")
            );
        }
    }

    ExitCode::SUCCESS
}

fn cmd_dump(args: DumpArgs) -> ExitCode {
    // Discover browsers
    let all_sources = browsers::discover();

    // Filter sources by browser and profile selection
    let profiles_lower: Vec<String> = args.profiles.iter().map(|p| p.to_lowercase()).collect();
    let sources: Vec<_> = all_sources
        .into_iter()
        .filter(|s| {
            if !args.browsers.is_empty() && !args.browsers.contains(&s.browser) {
                return false;
            }
            if !profiles_lower.is_empty() && !profiles_lower.contains(&s.profile.to_lowercase()) {
                return false;
            }
            true
        })
        .collect();

    if sources.is_empty() {
        eprintln!("No matching browsers found.");
        return ExitCode::from(1);
    }

    // Resolve --since vs positional shorthand
    let since_raw = match (&args.since, &args.since_positional) {
        (Some(_), Some(_)) => {
            eprintln!("Error: cannot use both --since and a positional time argument");
            return ExitCode::from(2);
        }
        (Some(s), None) | (None, Some(s)) => Some(s.as_str()),
        (None, None) => None,
    };

    // Parse date filters
    let since = match since_raw.map(timestamp::parse_user_datetime) {
        Some(Ok(dt)) => Some(dt),
        Some(Err(e)) => {
            eprintln!("Error parsing --since: {e}");
            return ExitCode::from(2);
        }
        None => None,
    };

    let before = match args.before.as_deref().map(timestamp::parse_user_datetime) {
        Some(Ok(dt)) => Some(dt),
        Some(Err(e)) => {
            eprintln!("Error parsing --before: {e}");
            return ExitCode::from(2);
        }
        None => None,
    };

    // Read history from all sources
    let (entries, errors) = browsers::read_all(&sources, since, before, args.visits);

    // Report errors on stderr
    for err in &errors {
        eprintln!("Warning: {err}");
    }

    if entries.is_empty() && errors.len() == sources.len() {
        eprintln!("All browsers failed to read.");
        return ExitCode::from(1);
    }

    // Build filter config
    let filter_config = match build_filter(&args) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::from(2);
        }
    };

    // Apply filters
    let filtered = match filter_config.apply(entries) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error evaluating --where expression: {e}");
            return ExitCode::from(2);
        }
    };

    // Write output
    let result = if let Some(ref path) = args.output {
        let file = match std::fs::File::create(path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Error creating output file: {e}");
                return ExitCode::from(1);
            }
        };
        let mut writer = std::io::BufWriter::new(file);
        format::write_entries(&mut writer, &filtered, args.format)
    } else {
        let mut stdout = std::io::stdout().lock();
        format::write_entries(&mut stdout, &filtered, args.format)
    };

    if let Err(e) = result {
        eprintln!("Error writing output: {e}");
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}

fn build_filter(args: &DumpArgs) -> Result<FilterConfig, String> {
    let where_expr = args
        .where_expr
        .as_deref()
        .map(WhereExpr::compile)
        .transpose()
        .map_err(|e| format!("Invalid --where expression: {e}"))?;

    Ok(FilterConfig {
        where_expr,
        limit: args.limit,
        deduplicate: args.dedup,
        include_internal: args.include_internal,
        include_noise: args.include_noise,
        sort: args.sort,
    })
}
