//! bhdump - Export browser history in JSON, CSV, and other formats.
//!
//! This library provides programmatic access to browser history databases
//! across all major browsers and platforms.
//!
//! # Example
//!
//! ```no_run
//! use bhdump::browsers;
//! use bhdump::filter::FilterConfig;
//! use bhdump::format::{OutputFormat, write_entries};
//!
//! // Discover all browsers on this system
//! let sources = browsers::discover();
//!
//! // Read history from all sources
//! let (entries, errors) = browsers::read_all(&sources, None, None, false);
//!
//! // Apply filters
//! let filter = FilterConfig::default();
//! let filtered = filter.apply(entries).unwrap();
//!
//! // Write to stdout as JSON
//! let mut stdout = std::io::stdout().lock();
//! write_entries(&mut stdout, &filtered, OutputFormat::Json).unwrap();
//! ```

pub mod browsers;
pub mod error;
pub mod filter;
pub mod format;
pub mod timestamp;
