//! The `ToolCall` enum is the single boundary between the agent's intentions
//! and the tool actor system.

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolCall {
    /// Execute a nushell command.
    Shell { command: String },

    /// Replace `old` with `new` in `path`.
    Edit { path: String, old: String, new: String },

    /// Write `content` to `path`.
    Write { path: String, content: String },

    /// Read the full content of `path`.
    Read { path: String },

    /// Search for `query` in `path`.
    Search {
        query: String,
        path: Option<String>,
        #[serde(default)]
        regex: bool,
    },

    /// Call a nushell tool from the toolbox.
    /// The router looks up `tools/<namespace>/<tool>.nu` and executes it
    /// via ShellWorker with `args` serialized as named flags.
    NushellTool {
        namespace: String,
        tool: String,
        #[serde(default, deserialize_with = "deserialize_args")]
        args: String,
    },

    /// Agent considers the task done.
    Submit { output: String },
}

fn deserialize_args<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::String(s) => Ok(s),
        other => Ok(other.to_string()),
    }
}

impl ToolCall {
    pub fn summary(&self) -> String {
        match self {
            Self::Shell { command }          => format!("shell: {}", crate::truncate(command, 60)),
            Self::Edit { path, .. }          => format!("edit: {path}"),
            Self::Write { path, .. }         => format!("write: {path}"),
            Self::Read { path }              => format!("read: {path}"),
            Self::Search { query, .. }       => format!("search: {}", crate::truncate(query, 40)),
            ToolCall::NushellTool { namespace, tool, args } => {
                // Include the primary target arg in the summary so loop detection
                // can distinguish between calls to the same tool with different targets.
                // e.g. "extract/file:/workspace/crates/core/src/error.rs" vs
                //      "extract/file:/workspace/crates/core/src/lib.rs"
                let args_val: serde_json::Value = serde_json::from_str(args).unwrap_or_default();
                let target = args_val.as_object()
                    .and_then(|o| {
                        // Look for common "target" arg names in priority order
                        o.get("file")
                            .or_else(|| o.get("path"))
                            .or_else(|| o.get("crate-path"))
                            .or_else(|| o.get("crate"))
                            .or_else(|| o.get("tool"))
                    })
                    .and_then(|v| v.as_str())
                    .map(|s| format!(":{}", crate::truncate(s, 40)))
                    .unwrap_or_default();
                format!("{namespace}/{tool}{target}")
            }
            Self::Submit { .. }              => "submit".to_string(),
        }
    }
}
