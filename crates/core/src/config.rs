//! Configuration types loaded from YAML and/or CLI flags.
//! A `RunConfig` is built by recursively merging:
//!   builtin defaults → yaml file → CLI overrides
//! matching the Python codebase's `recursive_merge` pattern but type-safe.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Full configuration for one agent run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfig {
    pub agent: AgentConfig,
    pub model: ModelConfig,
    pub shell: ShellConfig,
    #[serde(default)]
    pub run: RunMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Maximum model query steps before stopping.
    #[serde(default = "defaults::step_limit")]
    pub step_limit: u32,
    /// Stop after exceeding this cost in USD.
    #[serde(default = "defaults::cost_limit")]
    pub cost_limit: f64,
    /// Path to save the rkyv trajectory archive.
    pub output_path: Option<String>,
    /// Path to the minijinja system prompt template.
    pub system_template: String,
    /// Path to the minijinja first-user-message template.
    pub instance_template: String,
    /// Path to agent-task.json mission briefing.
    /// When set, current_task drives the instance prompt and
    /// rules/tools/playbook are injected into the system prompt.
    pub task_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub model_name: String,
    #[serde(default)]
    pub backend: ModelBackend,
    #[serde(default)]
    pub extra_kwargs: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModelBackend {
    #[default]
    Litellm,
    OpenRouter,
    /// Deterministic scripted responses — used in tests.
    Deterministic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    #[serde(default = "defaults::cwd")]
    pub cwd: String,
    #[serde(default = "defaults::timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RunMeta {
    pub task: Option<String>,
}

// ── Task file types ───────────────────────────────────────────────────────────

/// Parsed agent-task.json — the agent's mission briefing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskFile {
    #[serde(default)]
    pub schema_version: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub workspace_root: String,
    #[serde(default)]
    pub taskfile: String,
    #[serde(default)]
    pub last_updated: String,
    #[serde(default)]
    pub schema_notes: String,
    #[serde(default)]
    pub rules: TaskRules,
    pub current_task: Option<CurrentTask>,
    #[serde(default)]
    pub tools: serde_json::Value,
    #[serde(default)]
    pub completed: Vec<serde_json::Value>,
    #[serde(default)]
    pub blocked: Vec<serde_json::Value>,
    #[serde(default)]
    pub pending: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskRules {
    #[serde(default)]
    pub never: Vec<String>,
    #[serde(default)]
    pub always: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentTask {
    pub crate_name: Option<String>,
    #[serde(rename = "crate")]
    pub crate_field: Option<String>,
    pub crate_path: Option<String>,
    pub op: Option<String>,
    pub scope: Option<String>,
    pub status: Option<String>,
    #[serde(default)]
    pub review: bool,
    pub next_action: Option<String>,
    pub success_condition: Option<String>,
    pub notes: Option<String>,
}

impl CurrentTask {
    /// Return the crate name from either field name variant.
    pub fn crate_name(&self) -> Option<&str> {
        self.crate_name.as_deref().or(self.crate_field.as_deref())
    }

    /// Format as a concise mission briefing string for the instance prompt.
    pub fn to_mission_briefing(&self) -> String {
        let mut out = String::new();
        if let Some(c) = self.crate_name() {
            out.push_str(&format!("Crate: {c}\n"));
        }
        if let Some(path) = &self.crate_path {
            out.push_str(&format!("Path: {path}\n"));
        }
        if let Some(op) = &self.op {
            out.push_str(&format!("Operation: {op}\n"));
        }
        if let Some(scope) = &self.scope {
            out.push_str(&format!("Scope: {scope}\n"));
        }
        if let Some(condition) = &self.success_condition {
            out.push_str(&format!("Success condition: {condition}\n"));
        }
        if let Some(next) = &self.next_action {
            out.push_str(&format!("Next action: {next}\n"));
        }
        if let Some(notes) = &self.notes {
            out.push_str(&format!("Notes: {notes}\n"));
        }
        out
    }
}

impl TaskFile {
    /// Load and parse a task file from disk.
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read task file {}: {e}", path.display()))?;
        let task_file: Self = serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse task file {}: {e}", path.display()))?;
        Ok(task_file)
    }

    /// Format the never/always rules as a string for prompt injection.
    pub fn rules_section(&self) -> String {
        let mut out = String::new();
        if !self.rules.never.is_empty() {
            out.push_str("## Standing Rules — NEVER\n\n");
            for rule in &self.rules.never {
                out.push_str(&format!("- {rule}\n"));
            }
            out.push('\n');
        }
        if !self.rules.always.is_empty() {
            out.push_str("## Standing Rules — ALWAYS\n\n");
            for rule in &self.rules.always {
                out.push_str(&format!("- {rule}\n"));
            }
            out.push('\n');
        }
        out
    }
}

mod defaults {
    pub fn step_limit() -> u32 {
        50
    }
    pub fn cost_limit() -> f64 {
        3.0
    }
    pub fn cwd() -> String {
        String::from("/")
    }
    pub fn timeout() -> u64 {
        30
    }
}
