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
use tokio::sync::RwLock;
use tracing::{info, warn};

use mswea_core::{
    config::CurrentTask,
    toolbox::{
        OodaPhase, Playbook, PlaybookRegistry, PlaybookStep, PreflightResult,
        ToolEntry, ToolRegistry, ToolboxUpdate,
    },
};

use crate::event_bus::EventBus;
use crate::orchestrator::OrchestratorMsg;
use tokio::sync::RwLock as AsyncRwLock;

// ── Messages ──────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ToolboxMsg {
    /// Re-scan tools/, playbooks/, and skills/.
    ReloadAll,
    /// Re-scan tools/ only.
    ReloadTools,
    /// Re-scan skills/ only.
    ReloadSkills,
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
    pub tool_registry: Arc<AsyncRwLock<ToolRegistry>>,
}

// ── State ─────────────────────────────────────────────────────────────────────

pub struct ToolboxState {
    event_bus: EventBus,
    orchestrator: ActorRef<OrchestratorMsg>,
    mswea_root: PathBuf,
    shell: Arc<RwLock<environments::ShellWorker>>,
    tool_registry: ToolRegistry,
    shared_tool_registry: Arc<AsyncRwLock<ToolRegistry>>,
    playbook_registry: PlaybookRegistry,
    skills: String,
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
        };
        state.orchestrator
            .cast(OrchestratorMsg::UpdateToolbox(update))
            .map_err(|e| ActorProcessingErr::from(format!("Failed to push toolbox update: {e}")))
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
            None         => (name_type, "string".to_string()),
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

/// Scan `tools/playbooks/` for .nu files and parse them as playbooks.
/// For now we parse a minimal subset — task_type, version, description,
/// step names. Full structured parse comes when we implement the registry tools.
fn scan_playbooks(playbook_dir: &Path) -> PlaybookRegistry {
    let mut registry = PlaybookRegistry::default();

    let entries = match std::fs::read_dir(playbook_dir) {
        Ok(e) => e,
        Err(e) => {
            warn!(path = %playbook_dir.display(), error = %e, "ToolboxActor: cannot read playbooks dir");
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

        // Derive task_type from filename: "write-tests.nu" → "write-tests"
        let task_type = file_stem.clone();

        // Parse the playbook from the nushell record file.
        // For now, build a stub from the filename — full parse TBD.
        let playbook = parse_playbook_file(&path, &task_type);
        registry.playbooks.insert(task_type, playbook);
    }

    registry
}

/// Parse a playbook .nu file into a `Playbook` struct.
/// Currently builds a minimal stub — full structured parse is a future task.
fn parse_playbook_file(path: &Path, task_type: &str) -> Playbook {
    // Read the file to extract what we can from comments and structure
    let content = std::fs::read_to_string(path).unwrap_or_default();

    // Extract description from the file header comment
    let description = content
        .lines()
        .find(|l| l.starts_with("# Playbook for") || l.starts_with("# playbook/"))
        .map(|l| l.trim_start_matches('#').trim().to_string())
        .unwrap_or_else(|| format!("Playbook for {task_type}"));

    // Build stub steps from known write-tests structure
    // Full parse will come when we implement nu evaluation here
    let steps = build_stub_steps(task_type);

    Playbook {
        task_type:         task_type.to_string(),
        version:           "1.0".to_string(),
        description,
        success_condition: String::new(),
        preconditions:     Vec::new(),
        steps,
        source_path:       path.to_path_buf(),
    }
}

/// Build stub steps for known playbook types.
/// This is temporary until we implement full nu record parsing.
fn build_stub_steps(task_type: &str) -> Vec<PlaybookStep> {
    match task_type {
        "write-tests" => vec![
            PlaybookStep {
                name: "survey".into(),
                index: 0,
                description: "Understand the crate completely. Read only.".into(),
                budget: 3,
                on_budget_exhausted: "halt".into(),
                approved_tools: vec![
                    "locate/files".into(), "locate/actors".into(),
                    "locate/symbols".into(), "locate/derives".into(),
                    "locate/tests".into(), "compile/check".into(),
                    "extract/cargo-toml".into(),
                ],
                forbidden_tools: vec!["create/*".into(), "task/advance".into()],
                orient_questions: vec![
                    "Is this an actor crate, a types crate, or both?".into(),
                    "Which derive macros are present — serde? rkyv? both?".into(),
                    "Do existing tests exist? How many and what do they cover?".into(),
                    "Does compile/check pass cleanly right now?".into(),
                    "Are there private fields that prevent struct literal construction?".into(),
                ],
                verification_gate: "compile/check passed. Crate structure understood.".into(),
                notes: vec![],
                automated: true,
                automated_by: Some("ToolboxActor::preflight".into()),
            },
            PlaybookStep {
                name: "orient".into(),
                index: 1,
                description: "Write the coverage plan. Document every decision.".into(),
                budget: 2,
                on_budget_exhausted: "halt".into(),
                approved_tools: vec![
                    "task/write-coverage-plan".into(),
                    "task/state".into(),
                    "meta/loop-detect".into(),
                    "meta/orient-report".into(),
                ],
                forbidden_tools: vec![
                    "locate/*".into(), "extract/*".into(), "create/*".into(),
                    "compile/*".into(), "test/*".into(),
                ],
                orient_questions: vec![
                    "What are all public interfaces that need tests?".into(),
                    "Which types need serde roundtrip tests?".into(),
                    "Which types need rkyv roundtrip tests?".into(),
                    "Which actors need mailbox tests?".into(),
                    "How many total tests are planned?".into(),
                ],
                verification_gate: "task/write-coverage-plan called. planned_count > 0.".into(),
                notes: vec!["The coverage plan is a contract.".into()],
                automated: false,
                automated_by: None,
            },
            PlaybookStep {
                name: "scaffold".into(),
                index: 2,
                description: "Create test infrastructure. No test bodies yet.".into(),
                budget: 3,
                on_budget_exhausted: "halt".into(),
                approved_tools: vec![
                    "create/tests-dir".into(), "create/test-file".into(),
                    "create/cargo-test-entry".into(), "create/dev-dep".into(),
                    "compile/check".into(),
                ],
                forbidden_tools: vec!["test/*".into(), "task/advance".into()],
                orient_questions: vec![
                    "Does tests/ directory exist?".into(),
                    "Does Cargo.toml declare all required [[test]] entries?".into(),
                    "Do the empty test files compile cleanly?".into(),
                ],
                verification_gate: "compile/check passes on scaffolded files.".into(),
                notes: vec![],
                automated: false,
                automated_by: None,
            },
            PlaybookStep {
                name: "write".into(),
                index: 3,
                description: "Write test bodies. One test at a time. compile/check after each.".into(),
                budget: 5,
                on_budget_exhausted: "halt".into(),
                approved_tools: vec![
                    "extract/file".into(), "extract/range".into(),
                    "extract/symbol".into(), "extract/actor".into(),
                    "compile/check".into(), "compile/fix-hint".into(),
                ],
                forbidden_tools: vec!["test/*".into(), "task/advance".into()],
                orient_questions: vec![
                    "How many planned tests written so far? How many remain?".into(),
                    "Does the last written test compile cleanly?".into(),
                    "Has loop-detect flagged any repeated compile errors?".into(),
                ],
                verification_gate: "compile/check passes. All coverage plan tests written.".into(),
                notes: vec![
                    "One test at a time. compile/check after each.".into(),
                    "Never rewrite a file to fix a compile error.".into(),
                ],
                automated: false,
                automated_by: None,
            },
            PlaybookStep {
                name: "verify".into(),
                index: 4,
                description: "Run tests. Zero failures required.".into(),
                budget: 3,
                on_budget_exhausted: "halt".into(),
                approved_tools: vec![
                    "test/run".into(), "test/count".into(),
                    "test/verify-coverage".into(), "compile/check".into(),
                ],
                forbidden_tools: vec!["create/*".into(), "task/advance".into()],
                orient_questions: vec![
                    "How many tests passed? How many failed?".into(),
                    "Is failed == 0?".into(),
                    "Is gate_passed == true from verify-coverage?".into(),
                ],
                verification_gate: "test/run: failed == 0. gate_passed == true.".into(),
                notes: vec!["Never advance if failed > 0.".into()],
                automated: false,
                automated_by: None,
            },
            PlaybookStep {
                name: "finalize".into(),
                index: 5,
                description: "Format, final checks, advance task state.".into(),
                budget: 2,
                on_budget_exhausted: "halt".into(),
                approved_tools: vec![
                    "fmt/apply".into(), "fmt/check".into(),
                    "compile/check".into(), "test/run".into(),
                    "task/advance".into(),
                ],
                forbidden_tools: vec!["create/*".into(), "task/halt".into()],
                orient_questions: vec![
                    "Does fmt/check show unformatted files?".into(),
                    "Does test/run still show zero failures after fmt?".into(),
                ],
                verification_gate: "fmt/apply done. compile/check clean. task/advance called.".into(),
                notes: vec!["task/advance is the LAST call. Not the first.".into()],
                automated: false,
                automated_by: None,
            },
        ],
        _ => Vec::new(),
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
