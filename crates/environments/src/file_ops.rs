//! File and search operations for the agent.
//!
//! These are plain `std::fs` + `rg` subprocess operations — no nu engine
//! involvement. They return `Observation` directly.
//!
//! `Edit` guarantees exact-match-once semantics: it fails if `old` appears
//! zero times or more than once in the file.

use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use mswea_core::observation::{Observation, SearchMatch};
use tracing::{debug, instrument};

/// Read the full content of a file.
#[instrument]
pub fn read_file(path: &str) -> Result<Observation> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Reading {path}"))?;
    let size_bytes = content.len();
    debug!(path, size_bytes, "File read");
    Ok(Observation::FileContent {
        path: path.to_string(),
        content,
        size_bytes,
    })
}

/// Write content to a file, creating or overwriting it.
#[instrument(skip(content))]
pub fn write_file(path: &str, content: &str) -> Result<Observation> {
    // Create parent directories if needed.
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Creating parent dirs for {path}"))?;
        }
    }
    let original_lines = if Path::new(path).exists() {
        std::fs::read_to_string(path)
            .map(|s| s.lines().count() as i64)
            .unwrap_or(0)
    } else {
        0
    };
    std::fs::write(path, content)
        .with_context(|| format!("Writing {path}"))?;
    let new_lines = content.lines().count() as i64;
    let lines_changed = new_lines - original_lines;
    debug!(path, lines_changed, "File written");
    Ok(Observation::FileWritten {
        path: path.to_string(),
        lines_changed,
    })
}

/// Replace exactly one occurrence of `old` with `new` in `path`.
///
/// Fails with an error if:
///   - `old` is not found (no silent no-ops)
///   - `old` appears more than once (no ambiguous edits)
#[instrument(skip(old, new))]
pub fn edit_file(path: &str, old: &str, new: &str) -> Result<Observation> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Reading {path} for edit"))?;

    let count = content.matches(old).count();
    match count {
        0 => bail!("edit: `old` string not found in {path}"),
        1 => {}
        n => bail!("edit: `old` string found {n} times in {path} — must be unique"),
    }

    let original_lines = content.lines().count() as i64;
    let updated = content.replacen(old, new, 1);
    let new_lines = updated.lines().count() as i64;

    std::fs::write(path, &updated)
        .with_context(|| format!("Writing edited {path}"))?;

    let lines_changed = new_lines - original_lines;
    debug!(path, lines_changed, "File edited");
    Ok(Observation::FileWritten {
        path: path.to_string(),
        lines_changed,
    })
}

/// Search using ripgrep (`rg`).
///
/// Returns structured match results. Falls back to a helpful error if `rg`
/// is not in PATH.
#[instrument]
pub fn search(query: &str, path: Option<&str>, regex: bool) -> Result<Observation> {
    let search_path = path.unwrap_or(".");

    let mut cmd = Command::new("rg");

    // Output format: path:line:col:text (NUL-separated for robustness)
    cmd.args(["--line-number", "--column", "--no-heading", "--color=never"]);

    if !regex {
        cmd.arg("--fixed-strings");
    }

    cmd.arg(query);
    cmd.arg(search_path);

    debug!(query, search_path, regex, "Running rg");

    let output = cmd
        .output()
        .context("Failed to run `rg` — is ripgrep installed?")?;

    // rg exit codes: 0 = matches found, 1 = no matches, 2 = error
    if output.status.code() == Some(2) {
        bail!(
            "rg error: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut matches = Vec::new();

    for line in stdout.lines() {
        // rg default format: path:line:col:content
        // Split on ':' but be careful of Windows paths (C:\...)
        if let Some(m) = parse_rg_line(line) {
            matches.push(m);
        }
    }

    debug!(match_count = matches.len(), "Search complete");

    Ok(Observation::SearchResults {
        matches,
        query: query.to_string(),
    })
}

/// Parse one line of rg output: `path:line:col:content`
fn parse_rg_line(line: &str) -> Option<SearchMatch> {
    // Find the first ':' that's followed by digits (the line number field).
    // This handles paths like `/foo/bar.rs:42:7:content`.
    let mut parts = line.splitn(4, ':');
    let path = parts.next()?;
    let line_number: u64 = parts.next()?.parse().ok()?;
    let column: u64 = parts.next()?.parse().ok()?;
    let content = parts.next()?.to_string();

    Some(SearchMatch {
        path: path.to_string(),
        line_number,
        line: content,
        column: Some(column),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn tmp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "{content}").unwrap();
        f
    }

    #[test]
    fn edit_replaces_unique_match() {
        let f = tmp("fn foo() {}\nfn bar() {}\n");
        let path = f.path().to_str().unwrap();
        edit_file(path, "fn foo()", "fn baz()").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("fn baz()"));
        assert!(!content.contains("fn foo()"));
    }

    #[test]
    fn edit_fails_on_zero_matches() {
        let f = tmp("fn foo() {}\n");
        let path = f.path().to_str().unwrap();
        let err = edit_file(path, "fn missing()", "fn x()").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn edit_fails_on_multiple_matches() {
        let f = tmp("foo\nfoo\n");
        let path = f.path().to_str().unwrap();
        let err = edit_file(path, "foo", "bar").unwrap_err();
        assert!(err.to_string().contains("2 times"));
    }

    #[test]
    fn write_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.txt").to_str().unwrap().to_string();
        write_file(&path, "hello").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
    }

    #[test]
    fn parse_rg_line_basic() {
        let m = parse_rg_line("src/main.rs:42:7:    fn main()").unwrap();
        assert_eq!(m.path, "src/main.rs");
        assert_eq!(m.line_number, 42);
        assert_eq!(m.column, Some(7));
        assert_eq!(m.line, "    fn main()");
    }
}
