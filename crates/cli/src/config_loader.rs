//! Config loading and CLI-override merging.
//!
//! Resolution order (later wins):
//!   1. Built-in defaults (baked into `RunConfig`'s `#[serde(default)]` attrs)
//!   2. YAML file (`--config PATH` | `$MSWEA_CONFIG` | `~/.config/mswea/config.yaml`)
//!   3. CLI flags (only the flags that were explicitly provided)
//!
//! The final `RunConfig` is fully resolved — no `Option` fields bubble up
//! into the rest of the program.

use anyhow::{Context, Result};
use mswea_core::config::{AgentConfig, ModelConfig, ModelBackend, RunConfig, RunMeta, ShellConfig};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use crate::args::CliArgs;

/// Attempt to locate a config file, in priority order.
fn locate_config_file(explicit: Option<&Path>) -> Option<PathBuf> {
    // 1. Explicit flag / env var
    if let Some(p) = explicit {
        return Some(p.to_path_buf());
    }
    // 2. XDG-ish default: ~/.config/mswea/config.yaml
    if let Some(home) = dirs::config_dir() {
        let candidate = home.join("mswea").join("config.yaml");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Load YAML file into `RunConfig`, or return a pure-defaults config if
/// no file is found. Fails if a file was found but can't be parsed.
fn load_yaml(path: Option<&Path>) -> Result<RunConfig> {
    let Some(p) = path else {
        debug!("No config file found — using built-in defaults");
        return Ok(default_run_config());
    };

    info!(path = %p.display(), "Loading config file");
    let text = std::fs::read_to_string(p)
        .with_context(|| format!("Reading config file {}", p.display()))?;
    let cfg: RunConfig =
        serde_yaml::from_str(&text).with_context(|| format!("Parsing config file {}", p.display()))?;
    Ok(cfg)
}

/// Apply CLI overrides on top of a base `RunConfig`.
/// Only fields that were explicitly provided on the CLI are overwritten.
fn apply_cli_overrides(mut cfg: RunConfig, args: &CliArgs) -> RunConfig {
    if let Some(model) = &args.model {
        debug!(model, "CLI override: model");
        cfg.model.model_name = model.clone();
    }
    if let Some(n) = args.step_limit {
        debug!(n, "CLI override: step_limit");
        cfg.agent.step_limit = n;
    }
    if let Some(c) = args.cost_limit {
        debug!(c, "CLI override: cost_limit");
        cfg.agent.cost_limit = c;
    }
    if let Some(p) = &args.output {
        debug!(path = %p.display(), "CLI override: output");
        cfg.agent.output_path = Some(p.display().to_string());
    }
    if let Some(cwd) = &args.cwd {
        debug!(cwd = %cwd.display(), "CLI override: cwd");
        cfg.shell.cwd = cwd.display().to_string();
    }
    if let Some(task) = &args.task {
        cfg.run.task = Some(task.clone());
    }
    if let Some(tf) = &args.task_file {
        cfg.agent.task_file = Some(tf.clone());
    }
    if args.step_banner {
        cfg.agent.step_banner = true;
    }
    cfg
}

/// Full config resolution pipeline: locate → load YAML → apply CLI.
pub fn resolve_config(args: &CliArgs) -> Result<RunConfig> {
    let config_path = locate_config_file(args.config.as_deref());
    let base = load_yaml(config_path.as_deref())?;
    let cfg = apply_cli_overrides(base, args);

    if cfg.run.task.is_none() {
        warn!("No task set in config or CLI flags — will read from stdin");
    }

    Ok(cfg)
}

/// A sensible default `RunConfig` that works without any YAML file.
/// Template paths point to files that must exist in the repo; users
/// can override with their own config file.
fn default_run_config() -> RunConfig {
    RunConfig {
        agent: AgentConfig {
            step_limit: 1000,
            cost_limit: 3.0,
            output_path: None,
            system_template: "templates/system.j2".into(),
            instance_template: "templates/instance.j2".into(),
            task_file: None,
            step_banner: false,
        },
        model: ModelConfig {
            model_name: "claude-sonnet-4-5".into(),
            backend: ModelBackend::Litellm,
            extra_kwargs: serde_json::Value::Null,
        },
        shell: ShellConfig {
            cwd: "/workspace".into(),
            timeout_secs: 30,
            env: Default::default(),
        },
        run: RunMeta { task: None },
    }
}
