use crate::browsers::HistoryEntry;
use crate::error::Result;
use std::io::Write;
use std::str::FromStr;

/// Output format for history entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    /// Pretty-printed JSON array.
    #[default]
    Json,
    /// One JSON object per line (newline-delimited JSON).
    JsonLines,
    /// Comma-separated values.
    Csv,
    /// Tab-separated values.
    Tsv,
}

impl FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "json" => Ok(OutputFormat::Json),
            "jsonl" | "jsonlines" | "ndjson" => Ok(OutputFormat::JsonLines),
            "csv" => Ok(OutputFormat::Csv),
            "tsv" => Ok(OutputFormat::Tsv),
            _ => Err(format!(
                "unknown format: '{s}' (expected: json, jsonl, csv, tsv)"
            )),
        }
    }
}

/// Write history entries to the given writer in the specified format.
pub fn write_entries<W: Write>(
    writer: &mut W,
    entries: &[HistoryEntry],
    format: OutputFormat,
) -> Result<()> {
    match format {
        OutputFormat::Json => write_json(writer, entries),
        OutputFormat::JsonLines => write_jsonl(writer, entries),
        OutputFormat::Csv => write_delimited(writer, entries, b','),
        OutputFormat::Tsv => write_delimited(writer, entries, b'\t'),
    }
}

fn write_json<W: Write>(writer: &mut W, entries: &[HistoryEntry]) -> Result<()> {
    serde_json::to_writer_pretty(&mut *writer, entries)?;
    writeln!(writer)?;
    Ok(())
}

fn write_jsonl<W: Write>(writer: &mut W, entries: &[HistoryEntry]) -> Result<()> {
    for entry in entries {
        serde_json::to_writer(&mut *writer, entry)?;
        writeln!(writer)?;
    }
    Ok(())
}

fn write_delimited<W: Write>(
    writer: &mut W,
    entries: &[HistoryEntry],
    delimiter: u8,
) -> Result<()> {
    let mut csv_writer = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(writer);

    // Write header
    csv_writer.write_record([
        "url",
        "title",
        "visit_time",
        "visit_count",
        "visit_duration_ms",
        "browser",
        "profile",
    ])?;

    for entry in entries {
        csv_writer.write_record([
            &entry.url,
            entry.title.as_deref().unwrap_or(""),
            &entry.visit_time.to_rfc3339(),
            &entry.visit_count.map(|c| c.to_string()).unwrap_or_default(),
            &entry
                .visit_duration_ms
                .map(|d| d.to_string())
                .unwrap_or_default(),
            entry.browser.as_str(),
            &entry.profile,
        ])?;
    }

    csv_writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browsers::BrowserKind;
    use chrono::TimeZone;

    fn sample_entries() -> Vec<HistoryEntry> {
        vec![
            HistoryEntry {
                url: "https://example.com".to_string(),
                title: Some("Example".to_string()),
                visit_time: chrono::Utc
                    .with_ymd_and_hms(2024, 1, 15, 10, 30, 0)
                    .unwrap(),
                visit_count: Some(5),
                visit_duration_ms: None,
                browser: BrowserKind::Chrome,
                profile: "Default".to_string(),
            },
            HistoryEntry {
                url: "https://rust-lang.org".to_string(),
                title: None,
                visit_time: chrono::Utc.with_ymd_and_hms(2024, 1, 14, 8, 0, 0).unwrap(),
                visit_count: Some(1),
                visit_duration_ms: Some(30000),
                browser: BrowserKind::Firefox,
                profile: "default-release".to_string(),
            },
        ]
    }

    #[test]
    fn test_json_output() {
        let entries = sample_entries();
        let mut buf = Vec::new();
        write_entries(&mut buf, &entries, OutputFormat::Json).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("\"url\": \"https://example.com\""));
        assert!(output.contains("\"browser\": \"chrome\""));
        assert!(output.contains("\"profile\": \"Default\""));
    }

    #[test]
    fn test_jsonl_output() {
        let entries = sample_entries();
        let mut buf = Vec::new();
        write_entries(&mut buf, &entries, OutputFormat::JsonLines).unwrap();
        let output = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = output.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);
        // Each line should be valid JSON
        for line in &lines {
            serde_json::from_str::<serde_json::Value>(line).unwrap();
        }
    }

    #[test]
    fn test_csv_output() {
        let entries = sample_entries();
        let mut buf = Vec::new();
        write_entries(&mut buf, &entries, OutputFormat::Csv).unwrap();
        let output = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = output.trim().split('\n').collect();
        assert!(lines[0].contains("url,title,visit_time"));
        assert!(lines[1].contains("https://example.com"));
        assert!(lines[1].contains("chrome"));
        assert!(lines[1].contains("Default"));
    }

    #[test]
    fn test_format_parse() {
        assert_eq!(OutputFormat::from_str("json").unwrap(), OutputFormat::Json);
        assert_eq!(
            OutputFormat::from_str("jsonl").unwrap(),
            OutputFormat::JsonLines
        );
        assert_eq!(
            OutputFormat::from_str("ndjson").unwrap(),
            OutputFormat::JsonLines
        );
        assert_eq!(OutputFormat::from_str("csv").unwrap(), OutputFormat::Csv);
        assert_eq!(OutputFormat::from_str("tsv").unwrap(), OutputFormat::Tsv);
        assert!(OutputFormat::from_str("xml").is_err());
    }
}
