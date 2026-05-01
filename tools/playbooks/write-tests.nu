# playbook/write-tests.nu
#
# Playbook for writing test coverage for a Rust crate.
# Covers actor crates (ractor) and pure types crates (serde/rkyv).
#
# OODA standing orders apply at every step boundary:
#   OBSERVE:  task/state, meta/loop-detect, meta/trajectory-summary
#   ORIENT:   meta/orient-report — answer all orient questions explicitly
#   DECIDE:   playbook/current-step, tools/check-approved — one decision
#   ACT:      exactly one approved tool call
#
# Steps:
#   1. survey   — understand the crate before touching anything
#   2. orient   — build the coverage plan, document decisions
#   3. scaffold — create test infrastructure if needed
#   4. write    — write tests against the coverage plan
#   5. verify   — run tests, verify coverage plan fulfilled
#   6. finalize — fmt, final compile check, advance task state

{
  type: "write-tests",
  version: "1.0",
  description: "Write test coverage for a Rust crate — actors, types, or both.",
  success_condition: "All planned tests pass. zero failures. coverage plan fulfilled. task advanced.",
  global_approved_tools: [
    "meta/help", "meta/orient-report", "meta/loop-detect",
    "meta/trajectory-summary", "meta/step-verify", "meta/session-stats",
    "task/state", "task/record-attempt", "task/halt",
    "playbook/current-step"
  ],

  preconditions: [
    "current_task.crate is set and non-null",
    "current_task.crate_path is set and non-null",
    "workspace_root is accessible",
    "compile/check passes before we begin — never write tests for broken code"
  ],

  steps: [

    {
      name: "survey",
      index: 0,
      description: "Understand the crate completely. No file creation. No edits. Read only.",
      budget: 3,
      on_budget_exhausted: "halt",

      approved_tools: [
        "locate/files", "locate/actors", "locate/symbols",
        "locate/derives", "locate/tests", "locate/deps",
        "extract/file", "extract/range", "extract/symbol",
        "extract/actor", "extract/cargo-toml",
        "compile/check",
        "task/state", "meta/loop-detect", "meta/trajectory-summary",
        "meta/orient-report", "playbook/current-step", "tools/check-approved"
      ],

      forbidden_tools: [
        "create/*", "fmt/*", "lint/*",
        "task/advance", "task/write-coverage-plan"
      ],

      orient_questions: [
        "Is this an actor crate, a types crate, or both?",
        "How many public symbols exist?",
        "Which derive macros are present — serde? rkyv? both?",
        "Do existing tests exist? How many and what do they cover?",
        "Does compile/check pass cleanly right now?",
        "Are there private fields that prevent struct literal construction?",
        "Are there TcpClientActor or TLS dependencies requiring MockActor?"
      ],

      verification_gate: "All orient questions answered. compile/check passed. Crate structure understood.",

      notes: "If compile/check fails — halt immediately. Do not write tests for broken code."
    },

    {
      name: "orient",
      index: 1,
      description: "Write the coverage plan. Every decision documented. This is the contract for task/advance.",
      budget: 2,
      on_budget_exhausted: "halt",

      approved_tools: [
        "task/write-coverage-plan", "task/advance",
        "task/state", "meta/loop-detect", "meta/trajectory-summary",
        "meta/orient-report", "playbook/current-step", "tools/check-approved"
      ],

      forbidden_tools: [
        "locate/*", "extract/*", "create/*",
        "compile/*", "test/*", "fmt/*", "lint/*",
        "task/halt", "task/block"
      ],

      orient_questions: [
        "What are all public interfaces that need tests?",
        "For each interface — what are the failure modes?",
        "For each interface — what are the boundary conditions?",
        "Which types need serde roundtrip tests?",
        "Which types need rkyv roundtrip tests?",
        "Which actors need mailbox tests?",
        "Which actors need MockActor dependencies?",
        "How many total tests are planned?"
      ],

      verification_gate: "task/write-coverage-plan called. coverage_plan non-null in task state. planned_count > 0.",

      notes: "The coverage plan is a contract. Every planned test must have a name, type, and rationale. Do not advance until written."
    },

    {
      name: "scaffold",
      index: 2,
      description: "Create test infrastructure — tests/ dir, Cargo.toml entries, empty test files. No test bodies yet.",
      budget: 3,
      on_budget_exhausted: "halt",

      approved_tools: [
        "create/tests-dir", "create/test-file",
        "create/cargo-test-entry", "create/dev-dep",
        "compile/check", "locate/tests", "extract/cargo-toml",
        "task/state", "meta/loop-detect", "meta/trajectory-summary",
        "meta/orient-report", "playbook/current-step", "tools/check-approved"
      ],

      forbidden_tools: [
        "test/*", "fmt/*", "lint/*", "task/advance"
      ],

      orient_questions: [
        "Does tests/ directory exist?",
        "Does Cargo.toml declare all required [[test]] entries?",
        "Are all required dev-dependencies present?",
        "Do the empty test files compile cleanly?"
      ],

      verification_gate: "compile/check passes on scaffolded files. locate/tests confirms tests/ dir and Cargo.toml entries.",

      notes: "Scaffold only — no test bodies. Empty file with imports is correct. compile/check must pass before advancing."
    },

    {
      name: "write",
      index: 3,
      description: "Write test bodies against the coverage plan. One test at a time. compile/check after each.",
      budget: 5,
      on_budget_exhausted: "halt",

      approved_tools: [
        "extract/file", "extract/range", "extract/symbol", "extract/actor",
        "compile/check", "compile/fix-hint",
        "locate/symbols", "locate/derives",
        "task/state", "meta/loop-detect", "meta/trajectory-summary",
        "meta/orient-report", "playbook/current-step", "tools/check-approved"
      ],

      forbidden_tools: [
        "test/*", "create/*", "fmt/*", "lint/*",
        "task/advance", "task/write-coverage-plan"
      ],

      orient_questions: [
        "How many planned tests written so far? How many remain?",
        "Does the last written test compile cleanly?",
        "Am I appending to existing tests or rewriting? (must always append)",
        "Has loop-detect flagged any repeated compile errors?"
      ],

      verification_gate: "compile/check passes. All coverage plan tests written. No assert!(true) as sole assertion.",

      notes: [
        "One test at a time. compile/check after each.",
        "Never rewrite a file to fix a compile error — fix the specific line.",
        "Never assert!(true) or assert!(false) as the only assertion.",
        "Never std::env::set_var — use from_lookup pattern.",
        "Spawn mock actors with None name to avoid ActorAlreadyRegistered.",
        "If loop-detect fires on compile errors — compile/fix-hint, then halt if still stuck."
      ]
    },

    {
      name: "verify",
      index: 4,
      description: "Run the tests. Verify coverage plan fulfilled. Zero failures required to advance.",
      budget: 3,
      on_budget_exhausted: "halt",

      approved_tools: [
        "test/run", "test/count", "test/verify-coverage",
        "compile/check", "extract/range",
        "task/state", "meta/loop-detect", "meta/trajectory-summary",
        "meta/orient-report", "meta/step-verify",
        "playbook/current-step", "tools/check-approved"
      ],

      forbidden_tools: [
        "create/*", "fmt/*",
        "task/advance", "task/write-coverage-plan"
      ],

      orient_questions: [
        "How many tests passed? How many failed?",
        "Does test/verify-coverage confirm all planned tests exist and pass?",
        "Is failed == 0?",
        "Is gate_passed == true from verify-coverage?"
      ],

      verification_gate: "test/run: failed == 0. test/verify-coverage: gate_passed == true.",

      notes: [
        "Any failure — return to write step. Do NOT advance.",
        "Same test failing 3 times — halt and surface to human.",
        "Never advance if failed > 0.",
        "Success condition is gate_passed from verify-coverage — not a test count."
      ]
    },

    {
      name: "finalize",
      index: 5,
      description: "Format, final compile and test check, advance task state.",
      budget: 2,
      on_budget_exhausted: "halt",

      approved_tools: [
        "fmt/apply", "fmt/check",
        "compile/check", "test/run", "lint/check",
        "task/advance", "task/state",
        "meta/loop-detect", "meta/trajectory-summary",
        "meta/step-verify", "meta/session-stats",
        "playbook/current-step", "tools/check-approved"
      ],

      forbidden_tools: [
        "create/*", "task/write-coverage-plan",
        "task/halt", "task/block"
      ],

      orient_questions: [
        "Does fmt/check show unformatted files?",
        "Does compile/check still pass after fmt?",
        "Does test/run still show zero failures after fmt?",
        "Does this task have review:true? If so — write summary and stop before task/advance."
      ],

      verification_gate: "fmt/apply done. compile/check clean. test/run: failed == 0. task/advance called.",

      notes: [
        "fmt/apply then compile/check immediately — fmt can occasionally introduce issues.",
        "If review:true — write human-readable summary before task/advance.",
        "task/advance is the LAST call in this step. Not the first."
      ]
    }
  ]
}
