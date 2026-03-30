//! Observations are what tool actors return to the agent.
//!
//! `Observation::Structured` carries a `nu_protocol::Value` directly — no
//! string serialization inside the actor system. The JSON conversion happens
//! exactly once in `to_llm_content()` when building the LLM context window.
//!
//! `ObservationArchive` is a rkyv-able mirror type used only for trajectory
//! storage, since `nu_protocol::Value` does not implement `Archive`.

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

/// A structured result returned by a tool actor to the agent.
#[derive(Debug, Clone)]
pub enum Observation {
    /// Result of `ToolCall::Shell` — structured nushell output.
    Structured {
        value: nu_protocol::Value,
        exit_code: i64,
    },
    /// Result of `ToolCall::Read`.
    FileContent {
        path: String,
        content: String,
        size_bytes: usize,
    },
    /// Result of `ToolCall::Edit` or `ToolCall::Write`.
    FileWritten { path: String, lines_changed: i64 },
    /// Result of `ToolCall::Search`.
    SearchResults {
        matches: Vec<SearchMatch>,
        query: String,
    },
    /// Tool actor encountered an error — agent decides how to proceed.
    Error {
        message: String,
        exit_code: Option<i64>,
        tool_call_summary: String,
    },
    /// Agent emitted `ToolCall::Submit`.
    Submitted,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct SearchMatch {
    pub path: String,
    pub line_number: u64,
    pub line: String,
    pub column: Option<u64>,
}

impl Observation {
    /// Serialize to JSON for the LLM context window.
    /// This is the only place `nu_protocol::Value` → JSON conversion occurs.
    pub fn to_llm_content(&self) -> serde_json::Value {
        match self {
            Self::Structured { value, exit_code } => serde_json::json!({
                "exit_code": exit_code,
                "output": nu_value_to_json(value),
            }),
            Self::FileContent {
                path,
                content,
                size_bytes,
            } => serde_json::json!({
                "path": path.to_string(),
                "content": content,
                "size_bytes": size_bytes,
            }),
            Self::FileWritten {
                path,
                lines_changed,
            } => serde_json::json!({
                "path": path.to_string(),
                "lines_changed": lines_changed,
                "status": "ok",
            }),
            Self::SearchResults { matches, query } => serde_json::json!({
                "query": query,
                "match_count": matches.len(),
                "matches": matches.iter().map(|m| serde_json::json!({
                    "path": m.path.to_string(),
                    "line": m.line_number,
                    "content": m.line,
                })).collect::<Vec<_>>(),
            }),
            Self::Error {
                message,
                exit_code,
                tool_call_summary,
            } => serde_json::json!({
                "error": message,
                "exit_code": exit_code,
                "tool": tool_call_summary,
            }),
            Self::Submitted => serde_json::json!({ "status": "submitted" }),
        }
    }

    /// Convert to the rkyv-archivable mirror type for trajectory storage.
    pub fn to_archive(&self) -> ObservationArchive {
        match self {
            Self::Structured { value, exit_code } => ObservationArchive::Structured {
                value_json: nu_value_to_json(value).to_string(),
                exit_code: *exit_code,
            },
            Self::FileContent {
                path,
                content,
                size_bytes,
            } => ObservationArchive::FileContent {
                path: path.to_string(),
                content: content.clone(),
                size_bytes: *size_bytes,
            },
            Self::FileWritten {
                path,
                lines_changed,
            } => ObservationArchive::FileWritten {
                path: path.to_string(),
                lines_changed: *lines_changed,
            },
            Self::SearchResults { matches, query } => ObservationArchive::SearchResults {
                matches: matches.clone(),
                query: query.clone(),
            },
            Self::Error {
                message,
                exit_code,
                tool_call_summary,
            } => ObservationArchive::Error {
                message: message.clone(),
                exit_code: *exit_code,
                tool_call_summary: tool_call_summary.clone(),
            },
            Self::Submitted => ObservationArchive::Submitted,
        }
    }
}

/// rkyv-archivable mirror of `Observation`.
/// Used only for trajectory storage — never for live actor communication.
#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub enum ObservationArchive {
    /// nu Value stored as a JSON string since `nu_protocol::Value` isn't Archive.
    Structured {
        value_json: String,
        exit_code: i64,
    },
    FileContent {
        path: String,
        content: String,
        size_bytes: usize,
    },
    FileWritten {
        path: String,
        lines_changed: i64,
    },
    SearchResults {
        matches: Vec<SearchMatch>,
        query: String,
    },
    Error {
        message: String,
        exit_code: Option<i64>,
        tool_call_summary: String,
    },
    Submitted,
}

/// Recursively convert `nu_protocol::Value` → `serde_json::Value`.
/// Handles all variants the agent is likely to produce.
pub fn nu_value_to_json(val: &nu_protocol::Value) -> serde_json::Value {
    use nu_protocol::Value;
    match val {
        Value::Int { val, .. } => serde_json::json!(val),
        Value::Float { val, .. } => serde_json::json!(val),
        Value::String { val, .. } => serde_json::json!(val),
        Value::Bool { val, .. } => serde_json::json!(val),
        Value::Nothing { .. } => serde_json::Value::Null,
        Value::List { vals, .. } => {
            serde_json::Value::Array(vals.iter().map(nu_value_to_json).collect())
        }
        Value::Record { val, .. } => {
            // nu 0.111: Record uses SharedCow<Record<String, Value>>
            // iter() yields (&String, &Value)
            let map: serde_json::Map<String, serde_json::Value> = val
                .iter()
                .map(|(k, v)| (k.clone(), nu_value_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
        Value::Binary { val, .. } => {
            serde_json::json!({ "binary": format!("<{} bytes>", val.len()) })
        }
        Value::Date { val, .. } => serde_json::json!(val.to_rfc3339()),
        Value::Duration { val, .. } => serde_json::json!({ "duration_ns": val }),
        Value::Filesize { val, .. } => serde_json::json!({ "bytes": val }),
        Value::Error { error, .. } => serde_json::json!({ "error": error.to_string() }),
        Value::CellPath { val, .. } => serde_json::json!(val.to_string()),
        Value::Custom { .. } => serde_json::json!("<custom>"),
        // Catch-all: nushell's own display string
        other => serde_json::json!(other),
    }
}
