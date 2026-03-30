//! Configuration types loaded from YAML and/or CLI flags.
//! A `RunConfig` is built by recursively merging:
//!   builtin defaults → yaml file → CLI overrides
//! matching the Python codebase's `recursive_merge` pattern but type-safe.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
