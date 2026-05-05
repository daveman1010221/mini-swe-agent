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
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Creating parent dirs for {path}"))?;
        }
    }

    let original = if Path::new(path).exists() {
        std::fs::read_to_string(path).unwrap_or_default()
    } else {
        String::new()
    };

    if original == content {
        bail!("write: content is identical to existing file — no changes made");
    }

    let original_lines = original.lines().count() as i64;
    std::fs::write(path, content)
        .with_context(|| format!("Writing {path}"))?;
    let new_lines = content.lines().count() as i64;
    let lines_changed = new_lines - original_lines;
    debug!(path, lines_changed, "File written");
    Ok(Observation::FileWritten {
        path: path.to_string(),
        lines_changed,
        feedback: None,
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
        feedback: None,
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
