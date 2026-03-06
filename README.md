# bhdump

![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/sagikazarmark/bhdump/ci.yaml?style=flat-square)
![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/sagikazarmark/bhdump/badge?style=flat-square)

**Export browser history in JSON, CSV, and other formats.**

bhdump is a cross-platform command-line tool that reads browser history databases and exports them as structured data.
It supports all major browsers, provides powerful filtering via [CEL](https://cel-spec.dev) expressions, and normalizes entries into a unified schema.

## Features

- **11 browsers** across three engine families (Chromium, Firefox, Safari)
- **Auto-discovery** of installed browsers and profiles on macOS, Linux, and Windows
- **Output formats**: JSON, JSONL/NDJSON, CSV, TSV
- **CEL filter expressions** for flexible querying (`--where 'domain == "github.com"'`)
- **Natural language dates** (`today`, `7d`, `"last friday"`, `"3 days ago"`)
- **Noise filtering** removes auth flows, tracking, CDNs, and other non-content URLs by default
- **Safe reads** by copying databases to a temp directory (avoids lock contention with running browsers)
- **Usable as a library** in addition to the CLI

## Supported browsers

| Engine | Browsers |
|---|---|
| Chromium | Chrome, Chromium, Edge, Brave, Vivaldi, Opera, Arc |
| Firefox | Firefox, LibreWolf, Zen |
| Safari | Safari (macOS only) |

## Installation

### Pre-built binaries

Download a binary from the [latest release](https://github.com/sagikazarmark/bhdump/releases).

### Install script

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/sagikazarmark/bhdump/releases/latest/download/bhdump-installer.sh | sh
```

### Build from source

Requires [Rust](https://www.rust-lang.org/tools/install) 1.85+.

```sh
cargo install --git https://github.com/sagikazarmark/bhdump
```

## Usage

```sh
# Export all history as JSON (default)
bhdump

# Filter by time
bhdump today
bhdump 7d
bhdump "last friday"
bhdump --since 2024-01-01 --before 2024-07-01

# Filter by browser or profile
bhdump --browser chrome --browser firefox
bhdump --profile Default

# Choose output format
bhdump --format jsonl
bhdump --format csv
bhdump --format tsv

# Write to a file
bhdump --output history.json

# CEL filter expressions
bhdump --where 'url.contains("github.com")'
bhdump --where 'domain == "github.com" && visit_count > 5'
bhdump --where 'title.matches("(?i)rust")'

# Sort results
bhdump --sort -count       # most visited first
bhdump --sort url           # alphabetical by URL
bhdump --sort domain        # group by domain

# Other options
bhdump --limit 100          # cap number of entries
bhdump --dedup              # deduplicate by URL (keep most recent)
bhdump --visits             # individual visits instead of per-URL summary
bhdump --include-internal   # include chrome://, about:, etc.
bhdump --include-noise      # include auth, tracking, CDN domains
```

### Subcommands

```sh
bhdump browsers             # list detected browsers and profiles
bhdump completions bash     # generate shell completions (bash, zsh, fish, etc.)
bhdump validate 'expr'      # validate a CEL expression without running a query
```

### Filter expressions

The `--where` flag accepts [CEL](https://cel-spec.dev) expressions evaluated against each history entry.

**Available fields:**

| Field | Type | Description |
|---|---|---|
| `url` | string | Full URL |
| `domain` | string | Hostname extracted from the URL |
| `title` | string | Page title (empty string if absent) |
| `visit_count` | uint | Aggregate visit count |
| `browser` | string | Browser name (chrome, firefox, safari, ...) |
| `profile` | string | Profile name |
| `visit_time` | timestamp | When the page was visited |

**String methods:** `contains`, `startsWith`, `endsWith`, `matches` (regex), `size`

**Operators:** `==`, `!=`, `<`, `<=`, `>`, `>=`, `&&`, `||`, `!`, `in`

### Time shorthands

A positional argument serves as shorthand for `--since`:

| Format | Example |
|---|---|
| Keywords | `today`, `yesterday` |
| Compact | `7d`, `2w`, `3mo`, `1y`, `12h` |
| Relative keywords | `last-week` (7d), `last-month` (30d), `last-year` (365d) |
| Natural language | `"last friday"`, `"3 days ago"` |
| ISO 8601 | `2024-01-15`, `2024-01-15T10:30:00Z` |

## Library usage

bhdump can also be used as a Rust library:

```rust
use bhdump::browsers;
use bhdump::filter::FilterConfig;
use bhdump::format::{OutputFormat, write_entries};

let sources = browsers::discover();
let (entries, errors) = browsers::read_all(&sources, None, None, false);

let filter = FilterConfig::default();
let filtered = filter.apply(entries).unwrap();

let mut stdout = std::io::stdout().lock();
write_entries(&mut stdout, &filtered, OutputFormat::Json).unwrap();
```

## Comparison with similar tools

Several tools exist for extracting and querying browser history from the command line.
The table below compares the ones that are most similar in purpose to bhdump.

| | [bhdump] | [browser-history] | [bhgrep] |
|---|---|---|---|
| **Language** | Rust | Python | Rust |
| **Primary use case** | Export & filter | Export & library | Interactive search |
| **Browsers** | 11 | 14 | 4 |
| **Platforms** | macOS, Linux, Windows | macOS, Linux, Windows | macOS, Linux |
| **Output formats** | JSON, JSONL, CSV, TSV | JSON, CSV | JSON, plain text, URL-only |
| **Filter expressions** | CEL (`--where`) | - | Fuzzy, regex |
| **Natural language dates** | Yes (`today`, `7d`, `"last friday"`) | - | - |
| **Noise filtering** | Yes (auth, tracking, CDN) | - | - |
| **Safe reads (temp copy)** | Yes | - | - |
| **Interactive TUI** | - | - | Yes |
| **Bookmarks** | - | Yes | - |
| **Library usage** | Yes (Rust crate) | Yes (Python package) | - |
| **Individual visits** | Yes (`--visits`) | - | - |

[bhdump]: https://github.com/sagikazarmark/bhdump
[browser-history]: https://github.com/browser-history/browser-history
[bhgrep]: https://github.com/jondot/bhgrep

### When to use what

**bhdump** is designed for data export and pipeline use. If you need structured output with fine-grained filtering
(e.g. piping history into `jq`, loading into a database, or feeding into analysis scripts), bhdump is the best fit.
Its CEL expressions, noise filtering, and multiple output formats are built for this workflow.

**browser-history** is a good choice if you work in Python and want a lightweight library to integrate into your own scripts.
It also supports bookmark extraction, which bhdump does not. However, it lacks filtering, noise removal,
and only outputs basic JSON or CSV.

**bhgrep** is the right tool when you want to interactively search and browse your history from the terminal.
Its fuzzy matching, TUI, and clipboard integration make it a quick "where did I see that page?" tool
rather than a data export tool. It supports fewer browsers and has no structured export pipeline.

### Other tools

[**browser-gopher**](https://github.com/iansinnott/browser-gopher) (Go) takes a different approach:
it imports your history into its own SQLite database with a full-text index, then lets you search over the aggregated data.
This is useful if you want a persistent, searchable archive of your browsing history over time,
but it requires a separate `populate` step and is macOS-focused.

## License

The project is licensed under the [MIT License](LICENSE).
