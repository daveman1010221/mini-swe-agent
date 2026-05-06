//! Command-line argument definitions.
//!
//! Priority: CLI flags > YAML config file > built-in defaults.
//! Every field here is `Option<T>` so we can detect "was this flag actually
//! provided?" and only override the config where the user was explicit.

use clap::Parser;
use std::path::PathBuf;

/// mini-swe-agent — an autonomous coding agent.
#[derive(Debug, Parser)]
#[command(
    name = "mswea",
    version,
    about,
    long_about = "Runs an LLM-powered coding agent inside a nushell environment."
)]
pub struct CliArgs {
    // ── Task ────────────────────────────────────────────────────────────────

    /// Task description. Reads from stdin if omitted and --task-file is not set.
    ///
    /// Can also be set via the MSWEA_TASK environment variable.
    #[arg(
        short,
        long,
        value_name = "TEXT",
        env = "MSWEA_TASK",
        help = "Task for the agent to solve"
    )]
    pub task: Option<String>,

    /// Path to an agent-task.json file.
    ///
    /// When set, the agent reads `current_task` from the file as its mission
    /// briefing. The task file's `rules`, `tools`, and active playbook are
    /// injected into the system prompt via minijinja templates.
    ///
    /// Mutually exclusive with --task.
    #[arg(
        long,
        value_name = "PATH",
        env = "MSWEA_TASK_FILE",
        help = "Path to agent-task.json mission briefing"
    )]
    pub task_file: Option<PathBuf>,

    // ── Config file ──────────────────────────────────────────────────────────

    /// Path to a YAML config file.
    ///
    /// Defaults (in order): $MSWEA_CONFIG → ~/.config/mswea/config.yaml
    #[arg(
        short,
        long,
        value_name = "PATH",
        env = "MSWEA_CONFIG",
        help = "Path to YAML config file"
    )]
    pub config: Option<PathBuf>,

    // ── Model overrides ──────────────────────────────────────────────────────

    /// Model name, e.g. `claude-sonnet-4-5` or `gpt-4o`.
    #[arg(long, value_name = "NAME", env = "MSWEA_MODEL", help = "LLM model name")]
    pub model: Option<String>,

    // ── Agent overrides ──────────────────────────────────────────────────────

    /// Maximum agent steps before stopping.
    #[arg(long, value_name = "N", help = "Maximum steps (default: 50)")]
    pub step_limit: Option<u32>,

    /// Maximum spend in USD before stopping.
    #[arg(long, value_name = "USD", help = "Cost ceiling in USD (default: 3.0)")]
    pub cost_limit: Option<f64>,

    /// Where to write the rkyv trajectory archive.
    #[arg(long, value_name = "PATH", help = "Output path for trajectory archive")]
    pub output: Option<PathBuf>,

    /// Inject a step context banner into every observation response.
    /// Helps models that lose track of their current playbook step.
    /// Can also be enabled via MSWEA_STEP_BANNER=1.
    #[arg(
        long,
        env = "MSWEA_STEP_BANNER",
        help = "Inject step context banner into every observation (default: off)"
    )]
    pub step_banner: bool,

    // ── Shell overrides ──────────────────────────────────────────────────────

    /// Working directory for shell commands.
    #[arg(
        long,
        value_name = "DIR",
        env = "MSWEA_CWD",
        help = "Working directory for shell commands"
    )]
    pub cwd: Option<PathBuf>,

    // ── Observability ────────────────────────────────────────────────────────

    /// Log verbosity. Repeat for more detail: -v = debug, -vv = trace.
    #[arg(short, long, action = clap::ArgAction::Count, help = "Increase log verbosity")]
    pub verbose: u8,

    /// Emit logs as JSON (for structured log ingestion).
    #[arg(long, help = "Emit structured JSON logs")]
    pub json_logs: bool,
}
