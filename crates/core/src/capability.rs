//! Capability discovery protocol.
//!
//! Every tool actor publishes `Capability` to `OutputPort<Capability>` on
//! startup. The `OrchestratorActor` subscribes, maintains a live
//! `CapabilityMap`, and regenerates the system prompt section automatically.
//! No tool description is ever hardcoded in a prompt template.

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct ArgSpec {
    pub name: String,
    pub description: String,
    pub required: bool,
    /// Describes the type: "string", "path", "int", "bool", etc.
    pub arg_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct CommandCapability {
    /// Matches a `ToolCall` variant name: "shell", "edit", "read", etc.
    pub tool_call_variant: String,
    pub description: String,
    pub args: Vec<ArgSpec>,
    /// Concrete JSON examples shown verbatim to the LLM.
    pub examples: Vec<String>,
}

/// Full capability announcement from a single actor.
#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct Capability {
    pub actor_id: String,
    pub actor_name: String,
    pub commands: Vec<CommandCapability>,
}

/// Live aggregated capabilities — maintained by the OrchestratorActor.
#[derive(Debug, Clone, Default)]
pub struct CapabilityMap {
    /// actor_id → Capability
    pub actors: HashMap<String, Capability>,
}

impl CapabilityMap {
    pub fn update(&mut self, cap: Capability) {
        self.actors.insert(cap.actor_id.clone(), cap);
    }

    pub fn total_commands(&self) -> usize {
        self.actors.values().map(|c| c.commands.len()).sum()
    }

    pub fn actor_count(&self) -> usize {
        self.actors.len()
    }

    /// Render the tools section of the system prompt.
    /// Called every time the capability map is updated.
    /// This is the ONLY place tool descriptions are ever generated.
    pub fn render_system_prompt_section(&self) -> String {
        let mut out = String::from("## Available tools\n\n");
        let mut actors: Vec<_> = self.actors.values().collect();
        // Deterministic ordering so prompt diffs are stable
        actors.sort_by_key(|a| &a.actor_name);

        for cap in actors {
            out.push_str(&format!("### {}\n\n", cap.actor_name));
            for cmd in &cap.commands {
                out.push_str(&format!(
                    "**{}** — {}\n\n",
                    cmd.tool_call_variant, cmd.description
                ));
                if !cmd.args.is_empty() {
                    out.push_str("Arguments:\n");
                    for arg in &cmd.args {
                        let req = if arg.required { "required" } else { "optional" };
                        out.push_str(&format!(
                            "- `{}` ({}, {}): {}\n",
                            arg.name, arg.arg_type, req, arg.description
                        ));
                    }
                    out.push('\n');
                }
                if !cmd.examples.is_empty() {
                    out.push_str("Examples:\n");
                    for ex in &cmd.examples {
                        out.push_str(&format!("```json\n{ex}\n```\n\n"));
                    }
                }
            }
        }
        out
    }
}

/// Built-in capability announcements for the standard tool actors.
/// Each actor calls its function in its `pre_start` / `on_start` handler.
pub mod builtins {
    use super::*;

    pub fn shell_capabilities(actor_id: impl Into<String>) -> Capability {
        Capability {
            actor_id: actor_id.into(),
            actor_name: "shell".into(),
            commands: vec![CommandCapability {
                tool_call_variant: "shell".into(),
                description: "Execute a nushell command. \
                    Output is structured data (lists, records, tables), not a raw string."
                    .into(),
                args: vec![ArgSpec {
                    name: "command".into(),
                    description: "The nushell command to execute.".into(),
                    required: true,
                    arg_type: "string".into(),
                }],
                examples: vec![
                    r#"{"type":"shell","command":"ls | where size > 1mb | get name"}"#.into(),
                    r#"{"type":"shell","command":"cargo test 2>&1 | lines | last 20"}"#.into(),
                    r#"{"type":"shell","command":"open Cargo.toml | get package.version"}"#.into(),
                ],
            }],
        }
    }

    pub fn file_capabilities(actor_id: impl Into<String>) -> Capability {
        Capability {
            actor_id: actor_id.into(),
            actor_name: "file".into(),
            commands: vec![
                CommandCapability {
                    tool_call_variant: "read".into(),
                    description: "Read the full content of a file.".into(),
                    args: vec![ArgSpec {
                        name: "path".into(),
                        description: "File path.".into(),
                        required: true,
                        arg_type: "path".into(),
                    }],
                    examples: vec![r#"{"type":"read","path":"src/main.rs"}"#.into()],
                },
                CommandCapability {
                    tool_call_variant: "write".into(),
                    description: "Write content to a file, creating or overwriting it.".into(),
                    args: vec![
                        ArgSpec { name: "path".into(), description: "File path.".into(), required: true, arg_type: "path".into() },
                        ArgSpec { name: "content".into(), description: "Full file content.".into(), required: true, arg_type: "string".into() },
                    ],
                    examples: vec![r#"{"type":"write","path":"src/lib.rs","content":"pub fn hello() {}"}"#.into()],
                },
                CommandCapability {
                    tool_call_variant: "edit".into(),
                    description: "Replace an exact string in a file. \
                        Fails if `old` is not found or is not unique."
                        .into(),
                    args: vec![
                        ArgSpec { name: "path".into(), description: "File path.".into(), required: true, arg_type: "path".into() },
                        ArgSpec { name: "old".into(), description: "Exact string to replace.".into(), required: true, arg_type: "string".into() },
                        ArgSpec { name: "new".into(), description: "Replacement string.".into(), required: true, arg_type: "string".into() },
                    ],
                    examples: vec![r#"{"type":"edit","path":"src/lib.rs","old":"fn foo()","new":"fn bar()"}"#.into()],
                },
            ],
        }
    }

    pub fn search_capabilities(actor_id: impl Into<String>) -> Capability {
        Capability {
            actor_id: actor_id.into(),
            actor_name: "search".into(),
            commands: vec![CommandCapability {
                tool_call_variant: "search".into(),
                description: "Search for a pattern in files using ripgrep. \
                    Returns structured match results."
                    .into(),
                args: vec![
                    ArgSpec { name: "query".into(), description: "Search query or regex.".into(), required: true, arg_type: "string".into() },
                    ArgSpec { name: "path".into(), description: "Directory or file to search. Defaults to working tree.".into(), required: false, arg_type: "path".into() },
                    ArgSpec { name: "regex".into(), description: "Treat query as a regex pattern.".into(), required: false, arg_type: "bool".into() },
                ],
                examples: vec![
                    r#"{"type":"search","query":"fn execute","regex":false}"#.into(),
                    r#"{"type":"search","query":"TODO|FIXME","path":"src","regex":true}"#.into(),
                ],
            }],
        }
    }
}
