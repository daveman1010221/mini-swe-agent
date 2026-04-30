//! Toolbox types — shared between `ToolboxActor`, `OrchestratorActor`,
//! and `ToolRouterActor`.
//!
//! `ToolRegistry`    — index of all nushell tools in `tools/*/`
//! `PlaybookRegistry` — index of all playbooks in `tools/playbooks/`
//! `PreflightResult`  — snapshot produced by automated survey at task load

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// ── Tool registry ─────────────────────────────────────────────────────────────

// ── Tool registry ─────────────────────────────────────────────────────────────

/// A single flag parsed from a nushell tool's `def main [...]` block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFlag {
    /// Flag name as passed on CLI e.g. "crate-path"
    pub name: String,
    /// Nushell type annotation e.g. "path", "string", "int", "bool"
    pub flag_type: String,
    /// Default value if any — None means required
    pub default: Option<String>,
    /// Inline comment from the script e.g. "Path to the crate or directory"
    pub description: String,
}

impl ToolFlag {
    pub fn is_required(&self) -> bool {
        self.default.is_none()
    }

    /// Render as a compact signature string e.g. "--crate-path: path" or "[--pattern: string = \"*.rs\"]"
    pub fn render_signature(&self) -> String {
        let base = format!("--{}: {}", self.name, self.flag_type);
        match &self.default {
            None    => base,
            Some(d) => format!("[{base} = {d}]"),
        }
    }
}

/// A single registered nushell tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEntry {
    /// Full name e.g. "task/state"
    pub full_name: String,
    /// Namespace e.g. "task"
    pub namespace: String,
    /// Tool name within namespace e.g. "state"
    pub name: String,
    /// Absolute path to the .nu implementation file
    pub script_path: PathBuf,
    /// One-line description parsed from the script header comment
    pub description: String,
    /// Which OODA phase this tool belongs to
    pub ooda_phase: OodaPhase,
    /// Tags for fuzzy search
    pub tags: Vec<String>,
    /// Flags parsed from `def main [...]`
    pub flags: Vec<ToolFlag>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OodaPhase {
    Observe,
    Orient,
    Decide,
    Act,
    Any,
}

impl std::fmt::Display for OodaPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Observe => write!(f, "observe"),
            Self::Orient  => write!(f, "orient"),
            Self::Decide  => write!(f, "decide"),
            Self::Act     => write!(f, "act"),
            Self::Any     => write!(f, "any"),
        }
    }
}

/// Index of all registered nushell tools.
/// Keyed by full name ("task/state") for O(1) lookup by router.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolRegistry {
    /// full_name → ToolEntry
    pub tools: HashMap<String, ToolEntry>,
    /// Ordered list of namespaces for display
    pub namespaces: Vec<String>,
}

impl ToolRegistry {
    pub fn get(&self, full_name: &str) -> Option<&ToolEntry> {
        self.tools.get(full_name)
    }

    pub fn is_nushell_tool(tool_name: &str) -> bool {
        tool_name.contains('/')
            && !tool_name.starts_with("shell")
            && !tool_name.starts_with("read")
            && !tool_name.starts_with("write")
            && !tool_name.starts_with("edit")
            && !tool_name.starts_with("search")
            && !tool_name.starts_with("submit")
    }

    pub fn count(&self) -> usize {
        self.tools.len()
    }

    /// Render a compact tool listing for the system prompt.
    pub fn render_prompt_section(&self) -> String {
        if self.tools.is_empty() {
            return String::new();
        }
        let mut out = String::from("## Nushell Toolbox\n\n");
        out.push_str("Call these as: {\"type\": \"nushell_tool\", \"namespace\": \"<ns>\", \"tool\": \"<name>\", \"args\": {...}}\n");
        out.push_str("Use meta/help --tool <namespace/name> to get full usage docs for any tool.\n\n");

        let mut by_ns: HashMap<&str, Vec<&ToolEntry>> = HashMap::new();
        for entry in self.tools.values() {
            by_ns.entry(&entry.namespace).or_default().push(entry);
        }

        let mut namespaces: Vec<&str> = by_ns.keys().copied().collect();
        namespaces.sort();

        for ns in namespaces {
            out.push_str(&format!("### {ns}/\n\n"));
            let mut tools = by_ns[ns].clone();
            tools.sort_by_key(|t| &t.name);
            for tool in tools {
                let flags = tool.flags
                    .iter()
                    .map(|f| f.render_signature())
                    .collect::<Vec<_>>()
                    .join(" ");
                out.push_str(&format!(
                    "- **{full}** [{phase}] {flags}\n",
                    full  = tool.full_name,
                    phase = tool.ooda_phase,
                ));
            }
            out.push('\n');
        }
        out
    }
}

// ── Playbook registry ─────────────────────────────────────────────────────────

/// A single playbook step — mirrors the nushell record structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookStep {
    pub name: String,
    pub index: usize,
    pub description: String,
    pub budget: u32,
    pub on_budget_exhausted: String,
    pub approved_tools: Vec<String>,
    pub forbidden_tools: Vec<String>,
    pub orient_questions: Vec<String>,
    pub verification_gate: String,
    pub notes: Vec<String>,
    /// If true, ToolboxActor runs this step automatically at task load.
    #[serde(default)]
    pub automated: bool,
    #[serde(default)]
    pub automated_by: Option<String>,
}

/// A complete playbook for one task type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playbook {
    pub task_type: String,
    pub version: String,
    pub description: String,
    pub success_condition: String,
    pub preconditions: Vec<String>,
    pub steps: Vec<PlaybookStep>,
    /// Path to the .nu source file
    pub source_path: PathBuf,
}

impl Playbook {
    pub fn step_by_name(&self, name: &str) -> Option<&PlaybookStep> {
        self.steps.iter().find(|s| s.name == name)
    }

    pub fn step_by_index(&self, index: usize) -> Option<&PlaybookStep> {
        self.steps.get(index)
    }

    pub fn first_non_automated_step(&self) -> Option<&PlaybookStep> {
        self.steps.iter().find(|s| !s.automated)
    }
}

/// Index of all available playbooks, keyed by task type.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlaybookRegistry {
    /// task_type → Playbook
    pub playbooks: HashMap<String, Playbook>,
}

impl PlaybookRegistry {
    pub fn get(&self, task_type: &str) -> Option<&Playbook> {
        self.playbooks.get(task_type)
    }

    pub fn known_types(&self) -> Vec<String> {
        let mut types: Vec<String> = self.playbooks.keys().cloned().collect();
        types.sort();
        types
    }

    pub fn count(&self) -> usize {
        self.playbooks.len()
    }
}

// ── Preflight result ──────────────────────────────────────────────────────────

/// Results of the automated survey run by ToolboxActor at task load.
/// Injected into `ooda_section` so the agent starts with full situational
/// awareness without burning context on discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightResult {
    pub crate_name: String,
    pub crate_path: String,

    // Source shape
    pub source_file_count: usize,
    pub public_symbol_count: usize,
    pub actor_count: usize,
    pub is_actor_crate: bool,
    pub is_types_crate: bool,

    // Derive detection
    pub has_serde: bool,
    pub has_rkyv: bool,
    pub has_partial_eq: bool,

    // Existing tests
    pub has_tests_dir: bool,
    pub existing_test_count: usize,
    pub has_unit_tests: bool,
    pub has_prop_tests: bool,

    // Compile status
    pub compiles_clean: bool,
    pub compile_error_count: usize,
    pub compile_warning_count: usize,

    // Estimates
    pub estimated_tests_needed: usize,

    // Private fields check
    pub has_private_fields: bool,

    // Any blocking issues found during preflight
    pub blocking_issues: Vec<String>,
}

impl PreflightResult {
    /// Format as a human-readable context section for the system prompt.
    pub fn render_ooda_section(
        &self,
        task_type: &str,
        step_name: &str,
        step_index: usize,
        total_steps: usize,
        orient_questions: &[String],
        approved_tools: &[String],
        automated_steps_completed: &[String],
    ) -> String {
        let mut out = String::new();

        out.push_str("## Current Mission Context\n\n");
        out.push_str(&format!("**Task:** {task_type}\n"));
        out.push_str(&format!("**Crate:** {} ({})\n", self.crate_name, self.crate_path));
        out.push_str(&format!(
            "**Step:** {step_name} ({}/{total_steps})\n",
            step_index + 1
        ));

        if !automated_steps_completed.is_empty() {
            out.push_str(&format!(
                "**Automated steps completed:** {}\n",
                automated_steps_completed.join(", ")
            ));
        }

        out.push_str("\n## Pre-flight Survey\n\n");

        // Crate type
        let crate_type = match (self.is_actor_crate, self.is_types_crate) {
            (true, true)   => "actor + types crate",
            (true, false)  => "actor crate",
            (false, true)  => "types crate",
            (false, false) => "unknown",
        };
        out.push_str(&format!("- **Crate type:** {crate_type}\n"));
        out.push_str(&format!("- **Source files:** {}\n", self.source_file_count));
        out.push_str(&format!("- **Public symbols:** {}\n", self.public_symbol_count));

        if self.is_actor_crate {
            out.push_str(&format!("- **Actors found:** {}\n", self.actor_count));
        }

        // Derives
        let derives: Vec<&str> = [
            self.has_serde.then_some("serde"),
            self.has_rkyv.then_some("rkyv"),
            self.has_partial_eq.then_some("PartialEq"),
        ]
        .into_iter()
        .flatten()
        .collect();

        if derives.is_empty() {
            out.push_str("- **Derives:** none detected\n");
        } else {
            out.push_str(&format!("- **Derives:** {}\n", derives.join(", ")));
        }

        // Tests
        if self.existing_test_count == 0 {
            out.push_str("- **Existing tests:** 0 (no tests/ directory)\n");
        } else {
            out.push_str(&format!(
                "- **Existing tests:** {} (unit: {}, props: {})\n",
                self.existing_test_count,
                if self.has_unit_tests { "yes" } else { "no" },
                if self.has_prop_tests { "yes" } else { "no" },
            ));
        }

        // Compile status
        if self.compiles_clean {
            out.push_str("- **Compile:** clean ✓\n");
        } else {
            out.push_str(&format!(
                "- **Compile:** {} errors, {} warnings ✗\n",
                self.compile_error_count,
                self.compile_warning_count,
            ));
        }

        if self.has_private_fields {
            out.push_str("- **Private fields:** detected — use constructor methods, not struct literals\n");
        }

        out.push_str(&format!(
            "- **Estimated tests needed:** ~{}\n",
            self.estimated_tests_needed
        ));

        // Blocking issues
        if !self.blocking_issues.is_empty() {
            out.push_str("\n⚠️ **BLOCKING ISSUES — halt before proceeding:**\n");
            for issue in &self.blocking_issues {
                out.push_str(&format!("- {issue}\n"));
            }
        }

        // Current step guidance
        out.push_str(&format!("\n## Step: {step_name}\n\n"));

        if !orient_questions.is_empty() {
            out.push_str("**Orient questions:**\n");
            for q in orient_questions {
                out.push_str(&format!("- {q}\n"));
            }
            out.push('\n');
        }

        if !approved_tools.is_empty() {
            out.push_str("**Approved tools this step:**\n");
            out.push_str(&approved_tools.join(", "));
            out.push_str("\n\n");
        }

        out.push_str("**Standing orders:** OBSERVE → ORIENT → DECIDE → ACT → repeat.\n");
        out.push_str("Check task/state and meta/loop-detect before every action. No exceptions.\n");

        out
    }
}

// ── Shell policy ──────────────────────────────────────────────────────────────

/// Runtime-computed policy governing what the `shell` tool call variant may execute.
///
/// Built by `ToolboxActor` at boot by walking PATH and applying a blocklist.
/// Shared via `Arc<AsyncRwLock<ShellPolicy>>` between `ToolboxActor` (writer)
/// and `ToolRouterActor` (reader/enforcer). Also rendered into the system prompt
/// so the model knows exactly what it can use.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShellPolicy {
    /// Nushell builtin/keyword names that are allowed in shell tool calls.
    /// Derived from the full builtin list minus the blocked categories.
    pub allowed_builtins: Vec<String>,

    /// External binary names (basename only) allowed in shell tool calls.
    /// Computed at boot by walking PATH and subtracting BLOCKED_EXTERNALS.
    pub allowed_externals: Vec<String>,

    /// Human-readable reason why a command is blocked, keyed by command name
    /// or prefix pattern. Used to generate constructive feedback to the model.
    pub blocked_reasons: std::collections::HashMap<String, String>,
}

impl ShellPolicy {
    /// Check whether a shell command string is permitted.
    /// Parses the leading word and checks it against allowed sets.
    /// Returns Ok(()) if permitted, Err(message) with redirect guidance if not.
    pub fn check(&self, command: &str) -> Result<(), String> {
        let leading = command
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string();

        if leading.is_empty() {
            return Ok(());
        }

        // Builtins and keywords are always multi-word possible ("str replace", etc.)
        // Check prefix match against allowed_builtins first.
        let is_allowed_builtin = self.allowed_builtins.iter().any(|b| {
            // exact match OR the command starts with the builtin name followed by space
            command == b.as_str() || command.starts_with(&format!("{b} "))
        });
        if is_allowed_builtin {
            return Ok(());
        }

        // External: match leading word only
        if self.allowed_externals.contains(&leading) {
            return Ok(());
        }

        // Blocked — find the most specific reason
        let reason = self
            .blocked_reasons
            .get(&leading)
            .cloned()
            .unwrap_or_else(|| {
                "This command is not in the approved shell policy. \
                 Use the nushell toolbox tools for all workspace actions. \
                 The shell tool is for read-only inspection only."
                    .to_string()
            });

        Err(format!(
            "Shell policy violation: `{leading}` is not permitted. {reason}"
        ))
    }

    /// Render as a system prompt section so the model knows exactly what is available.
    pub fn render_prompt_section(&self) -> String {
        if self.allowed_externals.is_empty() && self.allowed_builtins.is_empty() {
            return String::new();
        }

        let mut out = String::from("## Shell Tool Policy\n\n");
        out.push_str(
            "The `shell` tool is for **read-only inspection only**. \
             It may not be used to compile, write files, run tests, mutate the \
             filesystem, or invoke interpreters. All workspace actions go through \
             the approved nushell toolbox.\n\n",
        );

        if !self.allowed_externals.is_empty() {
            out.push_str("**Allowed external commands:**\n");
            let mut sorted = self.allowed_externals.clone();
            sorted.sort();
            out.push_str(&sorted.join(", "));
            out.push_str("\n\n");
        }

        out.push_str(
            "**Allowed nushell builtins:** all `filters`, `strings`, `math`, \
             `formats`, `path`, `date`, `conversions`, `hash`, `generators` categories; \
             `core` keywords and control flow; `ls`, `open`, `glob`, `du` (read-only); \
             `ps`, `sys`, `which`, `complete`, `uname`, `whoami`.\n\n",
        );

        out.push_str(
            "**Blocked:** `save`, `rm`, `mkdir`, `mv`, `cp`, `touch`, `cd`, \
             `run-external`, `exec`, `source`, `http *`, `job *`, `stor *`, \
             `config *`, `overlay *`, `plugin *`, and all compiler/toolchain/VCS \
             externals. Violations return a policy error — use the toolbox instead.\n",
        );

        out
    }
}

// ── Toolbox update messages ───────────────────────────────────────────────────

/// Sent from ToolboxActor to OrchestratorActor when toolbox state changes.
#[derive(Debug, Clone)]
pub struct ToolboxUpdate {
    pub tool_registry:     ToolRegistry,
    pub playbook_registry: PlaybookRegistry,
    pub skills:            String,
    pub preflight:         Option<PreflightResult>,
    pub current_step:      Option<PlaybookStep>,
    pub shell_policy:      ShellPolicy,          // ← new
}
