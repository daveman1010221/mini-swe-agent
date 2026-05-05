//! `ToolboxActor` — owns the nushell tool registry, playbook registry,
//! skills, and automated preflight survey.
//!
//! # Responsibilities
//!
//! - Scans `tools/*/` at boot to build `ToolRegistry`
//! - Scans `tools/playbooks/` at boot to build `PlaybookRegistry`
//! - Loads `skills/*.md` at boot
//! - Runs automated preflight survey when a task is loaded
//! - Sends `OrchestratorMsg::UpdateToolbox` whenever state changes
//! - Responds to `ReloadAll` / `ReloadTools` / `ReloadSkills` messages
//!
//! # Preflight Survey
//!
//! When a task is loaded (`ToolboxMsg::TaskLoaded`), ToolboxActor runs
//! the automated survey steps from the playbook:
//!   - locate/files, locate/actors, locate/symbols, locate/derives
//!   - locate/tests, compile/check
//!
//! Results are packaged into `PreflightResult` and sent to
//! `OrchestratorActor` via `UpdateToolbox`, which regenerates the
//! system prompt with full situational awareness before step 1.
//!
//! Steps marked `automated: true` in the playbook are skipped by the
//! agent — ToolboxActor already ran them.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ractor::{Actor, ActorProcessingErr, ActorRef};
use ractor_cluster::RactorMessage;
use tokio::sync::RwLock;
use tracing::{info, warn};

use mswea_core::{
    ShellPolicy,
    config::CurrentTask,
    toolbox::{
        OodaPhase, Playbook, PlaybookRegistry, PlaybookStep, PreflightResult,
        ToolEntry, ToolRegistry, ToolboxUpdate,
    },
};

use crate::event_bus::EventBus;
use crate::orchestrator::OrchestratorMsg;

// ── Messages ──────────────────────────────────────────────────────────────────

#[derive(Debug, RactorMessage)]
pub enum ToolboxMsg {
    /// Re-scan tools/, playbooks/, and skills/.
    ReloadAll,
    /// Re-scan tools/ only.
    ReloadTools,
    /// Re-scan skills/ only.
    ReloadSkills,
    // Trigger a policy recompute.
    ReloadPolicy,
    /// A new task has been loaded — run preflight survey.
    TaskLoaded(CurrentTask),
}

// ── Arguments ─────────────────────────────────────────────────────────────────
pub struct ToolboxArgs {
    pub event_bus: EventBus,
    /// Reference to OrchestratorActor to push updates.
    pub orchestrator: ActorRef<OrchestratorMsg>,
    /// Root directory of the mswea repo (where tools/ and skills/ live).
    pub mswea_root: PathBuf,
    /// Shell worker for running nushell scripts during preflight.
    pub shell: Arc<RwLock<environments::ShellWorker>>,
    pub tool_registry: Arc<RwLock<ToolRegistry>>,
    pub shell_policy: Arc<RwLock<ShellPolicy>>,
}

// ── State ─────────────────────────────────────────────────────────────────────

pub struct ToolboxState {
    event_bus: EventBus,
    orchestrator: ActorRef<OrchestratorMsg>,
    mswea_root: PathBuf,
    shell: Arc<RwLock<environments::ShellWorker>>,
    tool_registry: ToolRegistry,
    shared_tool_registry: Arc<RwLock<ToolRegistry>>,
    playbook_registry: PlaybookRegistry,
    skills: String,
    shell_policy: ShellPolicy,
    shared_shell_policy: Arc<RwLock<ShellPolicy>>,
}
// ── Actor ─────────────────────────────────────────────────────────────────────

pub struct ToolboxActor;

impl Actor for ToolboxActor {
    type Msg = ToolboxMsg;
    type State = ToolboxState;
    type Arguments = ToolboxArgs;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        info!("ToolboxActor starting");

        let tools_dir    = args.mswea_root.join("tools");
        let skills_dir   = args.mswea_root.join("skills");
        let playbook_dir = args.mswea_root.join("tools").join("playbooks");

        let tool_registry     = scan_tools(&tools_dir);
        let playbook_registry = scan_playbooks(&playbook_dir);
        let skills            = load_skills(&skills_dir);
        let shell_policy      = build_shell_policy();

        *args.shell_policy.write().await = shell_policy.clone();

        // Write into the shared Arc so ToolRouterActor can see the registry
        *args.tool_registry.write().await = tool_registry.clone();

        info!(
            tools     = tool_registry.count(),
            playbooks = playbook_registry.count(),
            skills_bytes = skills.len(),
            "ToolboxActor: initial scan complete"
        );

        // Push initial state to OrchestratorActor
        let update = ToolboxUpdate {
            tool_registry:     tool_registry.clone(),
            playbook_registry: playbook_registry.clone(),
            skills:            skills.clone(),
            preflight:         None,
            current_step:      None,
            shell_policy:      shell_policy.clone(),
            global_approved_tools: playbook_registry
                .playbooks
                .values()
                .flat_map(|p| p.global_approved_tools.clone())
                .collect(),
        };
        args.orchestrator
            .cast(OrchestratorMsg::UpdateToolbox(update))
            .map_err(|e| ActorProcessingErr::from(format!("Failed to send toolbox update: {e}")))?;

        Ok(ToolboxState {
            event_bus: args.event_bus,
            orchestrator: args.orchestrator,
            mswea_root: args.mswea_root,
            shell: args.shell,
            tool_registry,
            shared_tool_registry: args.tool_registry,
            playbook_registry,
            skills,
            shell_policy,
            shared_shell_policy: args.shell_policy,
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        msg: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match msg {
            ToolboxMsg::ReloadAll => {
                let tools_dir    = state.mswea_root.join("tools");
                let skills_dir   = state.mswea_root.join("skills");
                let playbook_dir = state.mswea_root.join("tools").join("playbooks");

                state.tool_registry     = scan_tools(&tools_dir);
                state.playbook_registry = scan_playbooks(&playbook_dir);
                state.skills            = load_skills(&skills_dir);

                *state.shared_tool_registry.write().await = state.tool_registry.clone();

                info!(
                    tools     = state.tool_registry.count(),
                    playbooks = state.playbook_registry.count(),
                    "ToolboxActor: reloaded all"
                );
                self.push_update(state, None, None).await?;
            }

            ToolboxMsg::ReloadTools => {
                let tools_dir    = state.mswea_root.join("tools");
                let playbook_dir = state.mswea_root.join("tools").join("playbooks");

                state.tool_registry     = scan_tools(&tools_dir);
                state.playbook_registry = scan_playbooks(&playbook_dir);

                *state.shared_tool_registry.write().await = state.tool_registry.clone();

                info!(tools = state.tool_registry.count(), "ToolboxActor: reloaded tools");
                self.push_update(state, None, None).await?;
            }

            ToolboxMsg::ReloadSkills => {
                let skills_dir = state.mswea_root.join("skills");
                state.skills = load_skills(&skills_dir);
                info!(bytes = state.skills.len(), "ToolboxActor: reloaded skills");
                self.push_update(state, None, None).await?;
            }

            ToolboxMsg::ReloadPolicy => {
                state.shell_policy = build_shell_policy();
                *state.shared_shell_policy.write().await = state.shell_policy.clone();
                info!("ToolboxActor: shell policy reloaded");
                self.push_update(state, None, None).await?;
            }

            ToolboxMsg::TaskLoaded(task) => {
                info!(
                    crate_name = task.crate_name().unwrap_or("unknown"),
                    op = task.op.as_deref().unwrap_or("unknown"),
                    "ToolboxActor: running preflight survey"
                );

                let op = task.op.as_deref().unwrap_or("write-tests");
                let playbook = state.playbook_registry.get(op).cloned();

                let (preflight, current_step) = if let Some(pb) = playbook {
                    let pf = run_preflight(&task, &pb, &state.shell).await;
                    let step = pb.first_non_automated_step().cloned();
                    (Some(pf), step)
                } else {
                    warn!(op, "No playbook found for task type — agent will halt");
                    (None, None)
                };

                self.push_update(state, preflight, current_step).await?;
            }
        }
        Ok(())
    }
}

impl ToolboxActor {
    async fn push_update(
        &self,
        state: &ToolboxState,
        preflight: Option<PreflightResult>,
        current_step: Option<PlaybookStep>,
    ) -> Result<(), ActorProcessingErr> {
        let update = ToolboxUpdate {
            tool_registry:     state.tool_registry.clone(),
            playbook_registry: state.playbook_registry.clone(),
            skills:            state.skills.clone(),
            preflight,
            current_step,
            shell_policy: state.shell_policy.clone(),
            global_approved_tools: state.playbook_registry
                .playbooks
                .values()
                .flat_map(|p| p.global_approved_tools.clone())
                .collect(),
        };
        state.orchestrator
            .cast(OrchestratorMsg::UpdateToolbox(update))
            .map_err(|e| ActorProcessingErr::from(format!("Failed to push toolbox update: {e}")))
    }
}

/// Build the shell policy by walking PATH at runtime and applying blocklists.
fn build_shell_policy() -> ShellPolicy {
    // ── Blocked external prefixes/names ──────────────────────────────────────
    // Anything matching these is blocked regardless of what's on PATH.
    // Values are the constructive redirect message shown to the model.
    let blocked_externals: &[(&str, &str)] = &[
        // Compiler toolchains — use compile/* tools
        ("cargo",   "Use the compile/check, compile/fix-hint, test/run, or fmt/apply toolbox tools instead."),
        ("rustc",   "Direct compiler invocation is not permitted. Use compile/* toolbox tools."),
        ("rustfmt", "Use the fmt/apply or fmt/check toolbox tools instead."),
        ("rust-",   "Rust toolchain binaries are not permitted. Use toolbox tools."),
        ("gcc",     "Compiler invocation is not permitted. Use toolbox tools."),
        ("g++",     "Compiler invocation is not permitted. Use toolbox tools."),
        ("cc",      "Compiler invocation is not permitted. Use toolbox tools."),
        ("c++",     "Compiler invocation is not permitted. Use toolbox tools."),
        ("clang",   "Compiler invocation is not permitted. Use toolbox tools."),
        ("make",    "Build system invocation is not permitted. Use toolbox tools."),
        ("cmake",   "Build system invocation is not permitted. Use toolbox tools."),
        ("ld",      "Linker invocation is not permitted. Use toolbox tools."),
        // VCS — not permitted at all currently
        ("git",     "VCS operations are not permitted via the shell tool."),
        // Shell/interpreter escape hatches
        ("bash",    "Shell interpreter escape is not permitted. Use nushell builtins or toolbox tools."),
        ("sh",      "Shell interpreter escape is not permitted. Use nushell builtins or toolbox tools."),
        ("fish",    "Shell interpreter escape is not permitted. Use nushell builtins or toolbox tools."),
        ("nu",      "Spawning a nushell subprocess is not permitted. Use nushell builtins directly."),
        ("python",  "Python interpreter is not permitted. Use nushell builtins or toolbox tools."),
        ("python3", "Python interpreter is not permitted. Use nushell builtins or toolbox tools."),
        ("awk",     "Use nushell builtins (where, select, each, parse) instead."),
        ("sed",     "Use nushell str replace, parse, or str trim instead."),
        ("perl",    "Perl interpreter is not permitted. Use nushell builtins."),
        // Filesystem mutation
        ("cd", "The working directory is already set to /workspace for every shell command. `cd` is never needed — use absolute paths instead."),
        ("rm",      "File deletion is not permitted via the shell tool."),
        ("rmdir",   "Directory removal is not permitted via the shell tool."),
        ("mv",      "File move is not permitted via the shell tool. Use create/* toolbox tools."),
        ("cp",      "File copy is not permitted via the shell tool. Use create/* toolbox tools."),
        ("mkdir",   "Directory creation is not permitted via the shell tool. Use create/tests-dir."),
        ("touch",   "File creation is not permitted via the shell tool. Use create/* toolbox tools."),
        ("install", "File installation is not permitted via the shell tool."),
        ("patch",   "patch is not permitted — it mutates files. Use the edit tool or create/* toolbox tools."),
        ("ln",      "Symlink creation is not permitted via the shell tool."),
        ("dd",      "dd is not permitted via the shell tool."),
        ("shred",   "shred is not permitted via the shell tool."),
        ("truncate","truncate is not permitted via the shell tool."),
        // Package/env managers
        ("nix",     "Nix operations are not permitted via the shell tool."),
        ("nix-",    "Nix operations are not permitted via the shell tool."),
        // Process control
        ("kill",    "Process termination is not permitted via the shell tool."),
        ("pkill",   "Process termination is not permitted via the shell tool."),
        // Interactive/UI tools
        ("nvim",    "Interactive editors are not permitted via the shell tool. Use the edit tool."),
        ("vim",     "Interactive editors are not permitted via the shell tool. Use the edit tool."),
        ("just",    "just is not permitted — it invokes toolchain commands outside the approved toolbox."),
        // Network
        ("curl",    "Network access is not permitted via the shell tool."),
        ("wget",    "Network access is not permitted via the shell tool."),
        ("ssh",     "Remote access is not permitted via the shell tool."),
        ("rsync",   "Remote sync is not permitted via the shell tool."),
    ];

    let blocked_map: std::collections::HashMap<String, String> = blocked_externals
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    // ── Walk PATH and collect allowed externals ───────────────────────────────
    let path_var = std::env::var("PATH").unwrap_or_default();
    let mut allowed_externals: std::collections::HashSet<String> = std::collections::HashSet::new();

    for dir in path_var.split(':') {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

            // Must be executable
            let Ok(meta) = entry.metadata() else { continue };
            use std::os::unix::fs::PermissionsExt;
            if meta.permissions().mode() & 0o111 == 0 { continue; }

            // Skip if blocked by any prefix or exact match
            let is_blocked = blocked_map.contains_key(&name)
                || blocked_externals.iter().any(|(prefix, _)| {
                    prefix.ends_with('-') && name.starts_with(*prefix)
                });

            if !is_blocked {
                allowed_externals.insert(name);
            }
        }
    }

    let mut allowed_externals: Vec<String> = allowed_externals.into_iter().collect();
    allowed_externals.sort();

    // ── Allowed nushell builtins ──────────────────────────────────────────────
    // Entire allowed categories: filters, strings, math, formats, path,
    // date, conversions, hash, generators, bits, bytes, debug (non-mutating).
    // Selected commands from core, filesystem (read-only), system.
    // This list is the policy of record — enforced by prefix match in ShellPolicy::check().
    let allowed_builtins: Vec<String> = vec![
        // core — keywords and safe builtins
        "if", "else", "for", "while", "loop", "break", "continue", "return",
        "def", "alias", "use", "module", "export", "const", "let", "mut", "match",
        "do", "try", "collect", "where",
        "describe", "echo", "error make", "help", "ignore", "is-admin",
        "scope", "version",
        // filesystem — read-only subset
        "ls", "open", "glob", "du",
        // system — inspection only
        "complete", "ps", "sys", "uname", "which", "whoami",
        // filters — entire category
        "all", "any", "append", "chunk-by", "chunks", "columns", "compact",
        "default", "drop", "each", "each while", "enumerate", "every",
        "filter", "find", "first", "flatten", "get", "group-by", "headers",
        "insert", "interleave", "is-empty", "is-not-empty", "items", "join",
        "last", "length", "lines", "merge", "merge deep", "move", "par-each",
        "prepend", "reduce", "reject", "rename", "reverse", "roll", "rotate",
        "select", "shuffle", "skip", "slice", "sort", "sort-by", "split list",
        "take", "tee", "transpose", "uniq", "uniq-by", "update", "update cells",
        "upsert", "values", "window", "wrap", "zip",
        // strings — entire category
        "char", "decode", "detect", "encode", "format", "nu-check", "nu-highlight",
        "parse", "print", "split", "str", "url decode", "url encode",
        // math — entire category
        "math",
        // formats — entire category
        "from", "to",
        // path — entire category
        "path",
        // date — entire category
        "date",
        // conversions
        "fill", "format bits", "format number", "into",
        // hash
        "hash",
        // generators
        "cal", "generate", "seq",
        // bits
        "bits",
        // bytes
        "bytes",
        // debug — non-mutating inspection
        "ast", "debug", "explain", "inspect", "metadata", "timeit", "view",
        // viewers
        "grid", "table",
        // random (useful for test data generation)
        "random",
    ].into_iter().map(String::from).collect();

    ShellPolicy {
        allowed_builtins,
        allowed_externals,
        blocked_reasons: blocked_map,
    }
}

// ── Tool scanning ─────────────────────────────────────────────────────────────

/// Parse flags from a nushell script's `def main [...]` block.
/// Extracts name, type, default value, and inline comment for each flag.
fn parse_tool_flags(script_path: &std::path::Path) -> Vec<mswea_core::toolbox::ToolFlag> {
    let content = match std::fs::read_to_string(script_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    // Find the def main [...] block
    let start = match content.find("def main [") {
        Some(i) => i,
        None => return Vec::new(),
    };
    let rest = &content[start..];
    let end = match rest.find("] {") {
        Some(i) => i,
        None => return Vec::new(),
    };
    let block = &rest[..end];

    let mut flags = Vec::new();

    for line in block.lines() {
        let trimmed = line.trim();
        // Flag lines start with --
        if !trimmed.starts_with("--") { continue; }

        // Split off inline comment
        let (flag_part, description) = match trimmed.split_once('#') {
            Some((f, c)) => (f.trim(), c.trim().to_string()),
            None         => (trimmed, String::new()),
        };

        // Parse: --flag-name: type = default
        // or:    --flag-name: type,
        let flag_part = flag_part.trim_end_matches(',').trim();

        let (name_type, default) = match flag_part.split_once('=') {
            Some((nt, d)) => (nt.trim(), Some(d.trim().trim_matches('"').to_string())),
            None          => (flag_part, None),
        };

        let (raw_name, flag_type) = match name_type.split_once(':') {
            Some((n, t)) => (n.trim(), t.trim().to_string()),
            None => {
                // No type annotation — check if it's a bare switch (no default either)
                if default.is_none() {
                    (name_type, "switch".to_string())
                } else {
                    (name_type, "string".to_string())
                }
            }
        };

        let name = raw_name.trim_start_matches('-').to_string();

        flags.push(mswea_core::toolbox::ToolFlag {
            name,
            flag_type,
            default,
            description,
        });
    }

    flags
}

/// Scan `tools/*/` for namespace directories and parse tool entries.
/// Each namespace dir that has an `interfaces.nu` is registered.
/// Each `<tool>.nu` file (not interfaces.nu) is a tool implementation.
fn scan_tools(tools_dir: &Path) -> ToolRegistry {
    let mut registry = ToolRegistry::default();

    let entries = match std::fs::read_dir(tools_dir) {
        Ok(e) => e,
        Err(e) => {
            warn!(path = %tools_dir.display(), error = %e, "ToolboxActor: cannot read tools dir");
            return registry;
        }
    };

    let mut namespaces = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() { continue; }

        let ns = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        // Skip playbooks/ — handled separately
        if ns == "playbooks" { continue; }

        namespaces.push(ns.clone());

        // Scan for implementation files — anything ending in .nu
        // that isn't interfaces.nu
        let impl_entries = match std::fs::read_dir(&path) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for impl_entry in impl_entries.flatten() {
            let impl_path = impl_entry.path();
            let file_name = match impl_path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            if file_name == "interfaces.nu" { continue; }
            if impl_path.extension().and_then(|e| e.to_str()) != Some("nu") { continue; }

            let tool_name = file_name.trim_end_matches(".nu").to_string();
            let full_name = format!("{ns}/{tool_name}");

            // Infer OODA phase from namespace
            let ooda_phase = infer_ooda_phase(&ns, &tool_name);

            let flags = parse_tool_flags(&impl_path);

            let entry = ToolEntry {
                full_name:   full_name.clone(),
                namespace:   ns.clone(),
                name:        tool_name,
                script_path: impl_path,
                description: String::new(),
                ooda_phase,
                tags:        vec![ns.clone()],
                flags,
            };

            registry.tools.insert(full_name, entry);
        }
    }

    namespaces.sort();
    registry.namespaces = namespaces;
    registry
}

fn infer_ooda_phase(namespace: &str, _tool: &str) -> OodaPhase {
    match namespace {
        "meta" | "task"                           => OodaPhase::Observe,
        "playbook" | "discovery"                  => OodaPhase::Orient,
        "compile" | "test" | "lint" | "fmt"
        | "create" | "locate" | "extract"         => OodaPhase::Act,
        _                                         => OodaPhase::Any,
    }
}

// ── Playbook scanning ─────────────────────────────────────────────────────────

/// Scan `tools/playbooks/` for .nu files and parse them using the embedded
/// nushell engine. Each file is evaluated as a nushell record and walked
/// directly via nu_protocol::Value — no regex, no hardcoded stubs.
fn scan_playbooks(playbook_dir: &Path) -> PlaybookRegistry {
    let mut registry = PlaybookRegistry::default();

    let entries = match std::fs::read_dir(playbook_dir) {
        Ok(e) => e,
        Err(e) => {
            warn!(path = %playbook_dir.display(), error = %e, "ToolboxActor: cannot read playbooks dir");
            return registry;
        }
    };

    // Create a temporary session just for playbook parsing.
    // Empty env is fine — playbooks are pure data records with no side effects.
    let mut session = match environments::NushellSession::new("/workspace", &Default::default()) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "Failed to create NushellSession for playbook parsing — registry will be empty");
            return registry;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("nu") { continue; }

        let file_stem = match path.file_stem().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        match parse_playbook_file(&path, &file_stem, &mut session) {
            Ok(playbook) => {
                info!(task_type = %file_stem, steps = playbook.steps.len(), "Loaded playbook");
                registry.playbooks.insert(file_stem, playbook);
            }
            Err(e) => {
                warn!(path = %path.display(), error = %e, "Failed to parse playbook — skipping");
            }
        }
    }

    registry
}

/// Parse a playbook .nu file by evaluating it as a nushell record
/// and walking the resulting Value tree.
fn parse_playbook_file(
    path: &Path,
    task_type: &str,
    session: &mut environments::NushellSession,
) -> anyhow::Result<Playbook> {
    let value = session.parse_record_file(path)?;

    let description = nu_str(&value, "description")
        .unwrap_or_else(|| format!("Playbook for {task_type}"));
    let version = nu_str(&value, "version")
        .unwrap_or_else(|| "1.0".to_string());
    let success_condition = nu_str(&value, "success_condition")
        .unwrap_or_default();
    let preconditions = nu_str_list(&value, "preconditions");
    let global_approved_tools = nu_str_list(&value, "global_approved_tools");

    let steps = nu_list(&value, "steps")
        .into_iter()
        .enumerate()
        .filter_map(|(i, step_val)| parse_step_value(&step_val, i))
        .collect();

    Ok(Playbook {
        task_type: task_type.to_string(),
        version,
        description,
        success_condition,
        preconditions,
        global_approved_tools,
        steps,
        source_path: path.to_path_buf(),
    })
}

/// Parse a single step Value into a PlaybookStep.
fn parse_step_value(value: &nu_protocol::Value, index: usize) -> Option<PlaybookStep> {
    let name = nu_str(value, "name")?;
    let description = nu_str(value, "description").unwrap_or_default();
    let verification_gate = nu_str(value, "verification_gate").unwrap_or_default();
    let on_budget_exhausted = nu_str(value, "on_budget_exhausted")
        .unwrap_or_else(|| "halt".to_string());
    let budget = nu_int(value, "budget").unwrap_or(3) as u32;
    let approved_tools = nu_str_list(value, "approved_tools");
    let forbidden_tools = nu_str_list(value, "forbidden_tools");
    let orient_questions = nu_str_list(value, "orient_questions");
    let notes = nu_notes(value);

    let automated = name == "survey";
    let automated_by = if automated {
        Some("ToolboxActor::preflight".to_string())
    } else {
        None
    };

    Some(PlaybookStep {
        name,
        index,
        description,
        budget,
        on_budget_exhausted,
        approved_tools,
        forbidden_tools,
        orient_questions,
        verification_gate,
        notes,
        automated,
        automated_by,
    })
}

// ── nu_protocol::Value helpers ────────────────────────────────────────────────

fn nu_str(value: &nu_protocol::Value, key: &str) -> Option<String> {
    let record = value.as_record().ok()?;
    record.get(key)?.as_str().ok().map(|s| s.to_string())
}

fn nu_int(value: &nu_protocol::Value, key: &str) -> Option<i64> {
    let record = value.as_record().ok()?;
    record.get(key)?.as_int().ok()
}

fn nu_list(value: &nu_protocol::Value, key: &str) -> Vec<nu_protocol::Value> {
    let Ok(record) = value.as_record() else { return Vec::new() };
    let Some(val) = record.get(key) else { return Vec::new() };
    val.as_list().ok().map(|l| l.to_vec()).unwrap_or_default()
}

fn nu_str_list(value: &nu_protocol::Value, key: &str) -> Vec<String> {
    nu_list(value, key)
        .into_iter()
        .filter_map(|v| v.as_str().ok().map(|s| s.to_string()))
        .collect()
}

/// Extract notes — handles both single string and list of strings.
fn nu_notes(value: &nu_protocol::Value) -> Vec<String> {
    let Ok(record) = value.as_record() else { return Vec::new() };
    let Some(val) = record.get("notes") else { return Vec::new() };
    match val.as_list() {
        Ok(list) => list.iter()
            .filter_map(|v| v.as_str().ok().map(|s| s.to_string()))
            .collect(),
        Err(_) => val.as_str().ok()
            .map(|s| vec![s.to_string()])
            .unwrap_or_default(),
    }
}

// ── Skills loading ─────────────────────────────────────────────────────────────

fn load_skills(skills_dir: &Path) -> String {
    let mut skills = String::new();

    let mut entries: Vec<_> = match std::fs::read_dir(skills_dir) {
        Ok(e) => e.flatten().collect(),
        Err(e) => {
            warn!(path = %skills_dir.display(), error = %e, "ToolboxActor: cannot read skills dir");
            return skills;
        }
    };
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") { continue; }
        if let Ok(content) = std::fs::read_to_string(&path) {
            info!(path = %path.display(), "Loaded skill");
            skills.push_str(&content);
            skills.push('\n');
        }
    }

    skills
}

// ── Preflight survey ──────────────────────────────────────────────────────────

/// Run the automated survey steps for a task.
/// Executes locate/* and compile/check via nushell to produce a PreflightResult.
async fn run_preflight(
    task: &CurrentTask,
    _playbook: &Playbook,
    shell: &Arc<RwLock<environments::ShellWorker>>,
) -> PreflightResult {
    let crate_name = task.crate_name().unwrap_or("unknown").to_string();
    let crate_path = task.crate_path.clone().unwrap_or_default();

    // Default result — will be populated by nushell tool calls
    let mut result = PreflightResult {
        crate_name:             crate_name.clone(),
        crate_path:             crate_path.clone(),
        source_file_count:      0,
        public_symbol_count:    0,
        actor_count:            0,
        is_actor_crate:         false,
        is_types_crate:         false,
        has_serde:              false,
        has_rkyv:               false,
        has_partial_eq:         false,
        has_tests_dir:          false,
        existing_test_count:    0,
        has_unit_tests:         false,
        has_prop_tests:         false,
        compiles_clean:         false,
        compile_error_count:    0,
        compile_warning_count:  0,
        estimated_tests_needed: 0,
        has_private_fields:     false,
        blocking_issues:        Vec::new(),
    };

    let shell = shell.read().await;

    // ── locate/files ──────────────────────────────────────────────────────────
    let files_cmd = format!(
        "fd . {crate_path} --type f --extension rs | lines | length"
    );
    if let Ok(obs) = shell.exec(&files_cmd).await {
        if let mswea_core::observation::Observation::Structured { value, .. } = obs {
            if let Some(n) = nu_value_as_i64(&value) {
                result.source_file_count = n as usize;
            }
        }
    }

    // ── locate/actors ─────────────────────────────────────────────────────────
    let actors_cmd = format!(
        "rg -l 'impl Actor for' {crate_path} --type rust | lines | length"
    );
    if let Ok(obs) = shell.exec(&actors_cmd).await {
        if let mswea_core::observation::Observation::Structured { value, .. } = obs {
            if let Some(n) = nu_value_as_i64(&value) {
                result.actor_count = n as usize;
                result.is_actor_crate = n > 0;
            }
        }
    }

    // ── locate/derives ────────────────────────────────────────────────────────
    let derives_cmd = format!(
        "rg 'derive\\([^)]*\\)' {crate_path} --type rust -o | str join ' '"
    );
    if let Ok(obs) = shell.exec(&derives_cmd).await {
        if let mswea_core::observation::Observation::Structured { value, .. } = obs {
            let s = format!("{value:?}");
            result.has_serde     = s.contains("Serialize") || s.contains("Deserialize");
            result.has_rkyv      = s.contains("Archive");
            result.has_partial_eq = s.contains("PartialEq");
            result.is_types_crate = result.has_serde || result.has_rkyv;
        }
    }

    // ── locate/tests ──────────────────────────────────────────────────────────
    let tests_dir_cmd = format!("ls {crate_path}/tests | length");
    if let Ok(obs) = shell.exec(&tests_dir_cmd).await {
        if let mswea_core::observation::Observation::Structured { exit_code, .. } = obs {
            result.has_tests_dir = exit_code == 0;
        }
    }

    if result.has_tests_dir {
        let test_count_cmd = format!(
            "rg '^(async )?fn test_' {crate_path}/tests --type rust | lines | length"
        );
        if let Ok(obs) = shell.exec(&test_count_cmd).await {
            if let mswea_core::observation::Observation::Structured { value, .. } = obs {
                if let Some(n) = nu_value_as_i64(&value) {
                    result.existing_test_count = n as usize;
                }
            }
        }

        let unit_cmd = format!("ls {crate_path}/tests/unit.rs | length");
        result.has_unit_tests = shell.exec(&unit_cmd).await
            .map(|o| matches!(o, mswea_core::observation::Observation::Structured { exit_code, .. } if exit_code == 0))
            .unwrap_or(false);

        let props_cmd = format!("ls {crate_path}/tests/props.rs | length");
        result.has_prop_tests = shell.exec(&props_cmd).await
            .map(|o| matches!(o, mswea_core::observation::Observation::Structured { exit_code, .. } if exit_code == 0))
            .unwrap_or(false);
    }

    // ── compile/check ─────────────────────────────────────────────────────────
    let check_cmd = format!(
        "cd $WORKSPACE_ROOT; cargo check --package {crate_name} 2>&1 | complete"
    );
    if let Ok(obs) = shell.exec(&check_cmd).await {
        if let mswea_core::observation::Observation::Structured { value, exit_code, .. } = obs {
            result.compiles_clean = exit_code == 0;
            if exit_code != 0 {
                let output = format!("{value:?}");
                result.compile_error_count = output.matches("error[").count();
                result.compile_warning_count = output.matches("warning[").count();
                result.blocking_issues.push(format!(
                    "compile/check failed with {} errors — fix before writing tests",
                    result.compile_error_count
                ));
            }
        }
    }

    // ── private fields check ──────────────────────────────────────────────────
    let private_cmd = format!(
        "rg 'pub struct' {crate_path}/src --type rust -l | lines | length"
    );
    if let Ok(obs) = shell.exec(&private_cmd).await {
        if let mswea_core::observation::Observation::Structured { value, .. } = obs {
            if let Some(n) = nu_value_as_i64(&value) {
                // Heuristic: if there are pub structs, some may have private fields
                result.has_private_fields = n > 0;
            }
        }
    }

    // ── estimates ─────────────────────────────────────────────────────────────
    let serde_tests = if result.has_serde { result.public_symbol_count.max(1) } else { 0 };
    let rkyv_tests  = if result.has_rkyv  { result.public_symbol_count.max(1) } else { 0 };
    let actor_tests = result.actor_count * 3; // ~3 tests per actor
    result.estimated_tests_needed = (serde_tests + rkyv_tests + actor_tests)
        .saturating_sub(result.existing_test_count);

    result
}

fn nu_value_as_i64(value: &nu_protocol::Value) -> Option<i64> {
    match value {
        nu_protocol::Value::Int { val, .. } => Some(*val),
        _ => None,
    }
}
