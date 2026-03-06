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
| `visit_count` | int | Aggregate visit count |
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
| Named ranges | `last-week`, `last-month`, `last-year` |
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

## License

The project is licensed under the [MIT License](LICENSE).
