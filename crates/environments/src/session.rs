//! Stateful embedded Nushell session.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use nu_command::tls::CRYPTO_PROVIDER;
use nu_engine::get_eval_block_with_early_return;
use nu_parser::parse;
use nu_plugin::create_plugin_signature;
use nu_plugin_engine::{add_plugin_to_working_set, PluginDeclaration};
use nu_protocol::{
    engine::{EngineState, Stack, StateWorkingSet},
    PipelineData, PluginIdentity, Span, Value,
};
use tracing::{debug, instrument, warn};

pub struct NushellSession {
    engine: Arc<EngineState>,
    stack: Stack,
    seq: u64,
    cwd: String,
    env: std::collections::HashMap<String, String>,
}

impl NushellSession {
    pub fn new(cwd: &str, env: &std::collections::HashMap<String, String>) -> Result<Self> {
        let engine = create_engine(cwd)?;
        let mut stack = Stack::new();

        if let Ok(path) = std::env::var("PATH") {
            stack.add_env_var("PATH".into(), Value::string(path, Span::unknown()));
        }
        stack.add_env_var("PWD".into(), Value::string(cwd, Span::unknown()));

        for (k, v) in env {
            // Set in both the nushell stack AND the OS environment
            std::env::set_var(k, v);
            stack.add_env_var(k.clone(), Value::string(v.clone(), Span::unknown()));
        }

        Ok(Self {
            engine: Arc::new(engine),
            stack,
            seq: 0,
            cwd: cwd.to_string(),
            env: env.clone(),
        })
    }

    #[instrument(skip(self), fields(seq = self.seq))]
    pub fn eval(&mut self, command: &str) -> Result<(Value, i64)> {
        self.seq += 1;
        let source_name = format!("mswea_{}", self.seq);

        debug!(command, "Evaluating");

        // ── Parse ─────────────────────────────────────────────────────────
        let mut working_set = StateWorkingSet::new(&self.engine);
        let block = parse(
            &mut working_set,
            Some(&source_name),
            command.as_bytes(),
            false,
        );

        if !working_set.parse_errors.is_empty() {
            // Surface parse errors directly rather than attempting eval,
            // which would produce the confusing "Can't evaluate block in IR mode" error.
            let errs: Vec<String> = working_set
                .parse_errors
                .iter()
                .map(|e| format!("{e}"))
                .collect();
            let msg = errs.join("; ");
            debug!(errors = %msg, "Parse errors — aborting eval");
            return Err(anyhow!("Parse error: {msg}"));
        }

        // In nu 0.111, IR is the default evaluator. parse() builds the AST
        // Block but does NOT compile it to IR. We must compile explicitly
        // before merging the delta, otherwise eval fails with
        // "Can't evaluate block in IR mode".
        if let Err(e) = nu_engine::compile(&working_set, &block) {
            // Compilation errors are soft — log and continue. The AST
            // evaluator path handles uncompiled blocks.
            debug!(error = %e, "IR compile warning");
        }

        let delta = working_set.render();

        let engine_mut = Arc::make_mut(&mut self.engine);
        engine_mut.merge_delta(delta)?;

        // ── Evaluate ──────────────────────────────────────────────────────
        // Use the engine-aware helper which picks IR or AST evaluator based
        // on engine configuration.
        let eval_fn = get_eval_block_with_early_return(&self.engine);
        let result = eval_fn(&self.engine, &mut self.stack, &block, PipelineData::empty());

        match result {
            Ok(exec_data) => {
                let value = exec_data.body.into_value(Span::unknown())?;
                let exit_code = self
                    .stack
                    .get_env_var(&self.engine, "LAST_EXIT_CODE")
                    .and_then(|v| v.as_int().ok())
                    .unwrap_or(0);
                Ok((value, exit_code))
            }
            Err(shell_err) => Err(anyhow!("{shell_err}")),
        }
    }

    pub fn reset_stack(&mut self) {
        warn!("Resetting shell stack");
        self.stack = Stack::new();
        self.stack.add_env_var("PWD".into(), Value::string(&self.cwd, Span::unknown()));
        // Re-inject PATH — without this, external commands like cargo are not found
        // after a stack reset because PATH is not in self.env.
        if let Ok(path) = std::env::var("PATH") {
            self.stack.add_env_var("PATH".into(), Value::string(path, Span::unknown()));
        }
        for (k, v) in &self.env {
            std::env::set_var(k, v);
            self.stack.add_env_var(k.clone(), Value::string(v.clone(), Span::unknown()));
            warn!(key = %k, value = %v, "Re-injecting env var after stack reset");
        }
    }

    pub fn call_tool(&mut self, script_path: &std::path::Path, flags: &str) -> Result<(Value, i64)> {
        let script = std::fs::read_to_string(script_path)
            .map_err(|e| anyhow!("Failed to read script {}: {e}", script_path.display()))?;
        let command = if flags.is_empty() {
            format!("{script}\nmain")
        } else {
            format!("{script}\nmain {flags}")
        };
        let _ = std::env::set_current_dir(&self.cwd);
        self.eval_ephemeral(&command)
    }

    fn eval_ephemeral(&mut self, command: &str) -> Result<(Value, i64)> {
        self.stack.remove_env_var(&self.engine, "LAST_EXIT_CODE");
        self.seq += 1;
        let source_name = format!("mswea_tool_{}", self.seq);

        let mut working_set = StateWorkingSet::new(&self.engine);
        let block = parse(&mut working_set, Some(&source_name), command.as_bytes(), false);

        if !working_set.parse_errors.is_empty() {
            let msg = working_set.parse_errors.iter()
                .map(|e| format!("{e}"))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(anyhow!("Parse error: {msg}"));
        }

        if let Err(e) = nu_engine::compile(&working_set, &block) {
            debug!(error = %e, "IR compile warning");
        }

        // ── Key difference: do NOT merge delta into persistent engine ──
        // This prevents def main from accumulating across tool calls.

        // We still need a temporary engine with the delta for eval
        let mut temp_engine = (*self.engine).clone();
        let delta = working_set.render();
        temp_engine.merge_delta(delta)?;
        let temp_engine = Arc::new(temp_engine);

        let eval_fn = get_eval_block_with_early_return(&temp_engine);
        let result = eval_fn(&temp_engine, &mut self.stack, &block, PipelineData::empty());

        match result {
            Ok(exec_data) => {
                let value = exec_data.body.into_value(Span::unknown())?;
                let exit_code = self.stack
                    .get_env_var(&temp_engine, "LAST_EXIT_CODE")
                    .and_then(|v| v.as_int().ok())
                    .unwrap_or(0);
                Ok((value, exit_code))
            }
            Err(shell_err) => Err(anyhow!("{shell_err}")),
        }
    }

    pub fn engine(&self) -> Arc<EngineState> {
        self.engine.clone()
    }

    /// Evaluate a nushell record file and return the resulting Value.
    /// Used by ToolboxActor to parse playbook files without subprocess overhead.
    pub fn parse_record_file(&mut self, path: &std::path::Path) -> anyhow::Result<nu_protocol::Value> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {e}", path.display()))?;
        let (value, _) = self.eval_ephemeral(&content)?;
        Ok(value)
    }

    /// Register the nu_plugin_mswea binary into the engine so that
    /// `mswea rpc record-orient` etc. are available as native commands.
    ///
    /// Must be called after `new()` and before any tool scripts run.
    /// The plugin binary path must be absolute.
    ///
    /// IMPORTANT: This registers the plugin signatures without spawning the
    /// plugin process. The plugin process is spawned lazily by nushell on
    /// first use, inheriting the environment (including MSWEA_CLUSTER_ADDR
    /// and MSWEA_CLUSTER_COOKIE) set during session construction.
    pub fn register_mswea_plugin(&mut self, plugin_binary: &std::path::Path) -> Result<()> {
        let identity = PluginIdentity::new(plugin_binary, None)
            .map_err(|e| anyhow!("Invalid plugin path {}: {e}", plugin_binary.display()))?;

        let engine_mut = Arc::make_mut(&mut self.engine);
        let mut working_set = StateWorkingSet::new(engine_mut);

        let plugin = add_plugin_to_working_set(&mut working_set, &identity)
            .map_err(|e| anyhow!("Failed to register plugin: {e}"))?;

        // Register each command signature from the plugin
        let mswea_plugin = nu_plugin_mswea::MsweaPlugin::new(None, None,
            tokio::runtime::Handle::try_current()
                .unwrap_or_else(|_| tokio::runtime::Runtime::new()
                    .expect("tokio runtime")
                    .handle()
                    .clone())
        );

        for command in nu_plugin::Plugin::commands(&mswea_plugin) {
            let sig = create_plugin_signature(command.as_ref());
            let decl = PluginDeclaration::new(plugin.clone(), sig);
            working_set.add_decl(Box::new(decl));
        }

        let delta = working_set.render();
        engine_mut.merge_delta(delta)
            .map_err(|e| anyhow!("Failed to merge plugin delta: {e}"))?;

        tracing::info!(path = %plugin_binary.display(), "Registered mswea plugin commands");
        Ok(())
    }
}

fn create_engine(cwd: &str) -> Result<EngineState> {
    CRYPTO_PROVIDER.default();
    let engine = nu_cmd_lang::create_default_context();
    let engine = nu_command::add_shell_command_context(engine);
    let mut engine = engine;
    engine.add_env_var("PWD".into(), Value::string(cwd, Span::unknown()));
    Ok(engine)
}
