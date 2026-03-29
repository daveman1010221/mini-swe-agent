//! The `ToolCall` enum is the single boundary between the agent's intentions
//! and the tool actor system. The agent emits `ToolCall` values; the
//! `ToolRouterActor` pattern-matches on the variant and dispatches to the
//! correct actor. The agent never knows which actor handled the call.
//!
//! `serde` handles the JSON boundary (LLM response → ToolCall).
//! `rkyv` handles the storage boundary (ToolCall → trajectory on disk).

use std::path::PathBuf;
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolCall {
    /// Execute a nushell command. Result is a structured `nu_protocol::Value`.
    Shell {
        command: String,
    },

    /// Replace `old` with `new` in `path`. Fails if `old` is not found
    /// exactly once — no silent partial edits.
    Edit {
        path: PathBuf,
        old: String,
        new: String,
    },

    /// Write `content` to `path`, creating or overwriting.
    Write {
        path: PathBuf,
        content: String,
    },

    /// Read the full content of `path`.
    Read {
        path: PathBuf,
    },

    /// Search for `query` in `path` (or working tree if None).
    Search {
        query: String,
        path: Option<PathBuf>,
        /// Treat `query` as a regex pattern.
        #[serde(default)]
        regex: bool,
    },

    /// Agent considers the task done. Triggers `AgentError::Submitted`.
    Submit {
        output: String,
    },
}

impl ToolCall {
    /// Short summary for logging and the live monitor display.
    pub fn summary(&self) -> String {
        match self {
            Self::Shell { command }    => format!("shell: {}", truncate(command, 60)),
            Self::Edit { path, .. }    => format!("edit: {}", path.display()),
            Self::Write { path, .. }   => format!("write: {}", path.display()),
            Self::Read { path }        => format!("read: {}", path.display()),
            Self::Search { query, .. } => format!("search: {}", truncate(query, 40)),
            Self::Submit { .. }        => "submit".to_string(),
        }
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_shell() {
        let call = ToolCall::Shell { command: "ls -la".into() };
        let json = serde_json::to_string(&call).unwrap();
        assert!(json.contains(r#""type":"shell""#));
        let back: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(call, back);
    }

    #[test]
    fn round_trip_edit() {
        let call = ToolCall::Edit {
            path: PathBuf::from("src/main.rs"),
            old: "foo".into(),
            new: "bar".into(),
        };
        let back: ToolCall = serde_json::from_str(&serde_json::to_string(&call).unwrap()).unwrap();
        assert_eq!(call, back);
    }

    #[test]
    fn search_regex_defaults_false() {
        let json = r#"{"type":"search","query":"TODO"}"#;
        let call: ToolCall = serde_json::from_str(json).unwrap();
        matches!(call, ToolCall::Search { regex: false, .. });
    }
}
