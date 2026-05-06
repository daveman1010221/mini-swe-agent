#!/usr/bin/env nu
# task/evaluate-coverage-plan.nu — evaluate the current coverage plan against crate surface
#
# Compares the recorded coverage plan against what actually exists in the crate.
# Returns a structured gap report and an approved:true/false decision.
#
# The agent must iterate: evaluate → revise plan → evaluate again until approved:true.
#
# Approval gates (ALL must pass):
#   - Every file with Serialize/Deserialize has a planned serde roundtrip test
#   - Every file with rkyv Archive derive has a planned rkyv roundtrip test
#   - planned_count >= minimum_required (max of serde_files, rkyv_files)
#   - If proptest candidates exist, at least one proptest is planned
#   - If error types exist, at least one Display/Debug test is planned
#
# Usage:
#   nu tools/task/evaluate-coverage-plan.nu --crate-path /workspace/crates/core

def main [
    --crate-path: path,
] {
    if ($crate_path | is-empty) {
        return {
            ok: false,
            data: null,
            error: "missing required flag: --crate-path"
        }
    }

    let src_path = ($crate_path | path join "src")

    if not ($src_path | path exists) {
        return {
            ok: false,
            data: null,
            error: $"src/ directory not found at ($src_path)"
        }
    }

    # ── Get current plan from task state ──────────────────────────────────────

    let state_result = (mswea rpc task-state)

    if not $state_result.ok {
        return {
            ok: false,
            data: null,
            error: "could not read task state"
        }
    }

    let coverage_plan = ($state_result.data.coverage_plan? | default null)

    if $coverage_plan == null {
        return {
            ok: true,
            data: {
                approved: false,
                gaps: ["No coverage plan recorded — call task/write-coverage-plan first"],
                proptest_candidates: [],
                planned_count: 0,
                minimum_required: 0,
                quality_score: 0,
                serde_files: 0,
                rkyv_files: 0,
                error_files: 0,
                proptest_files: 0,
            },
            error: null
        }
    }

    let planned_tests = ($coverage_plan.planned_tests? | default [])
    let planned_count = ($planned_tests | length)

    # Flatten planned test names to lowercase for matching
    let planned_names = (
        $planned_tests
        | each {|t|
            if ($t | describe) == "string" { $t } else { $t.name? | default "" }
        }
        | str downcase
    )

    # ── Scan crate surface ────────────────────────────────────────────────────

    # Files with serde derives
    let serde_files = (
        try {
            rg -l "Serialize|Deserialize" $src_path
            | lines
            | where {|l| ($l | str length) > 0 }
            | length
        } catch { 0 }
    )

    # Files with rkyv derives
    let rkyv_files = (
        try {
            rg -l "Archive" $src_path
            | lines
            | where {|l| ($l | str length) > 0 }
            | length
        } catch { 0 }
    )

    # Files with proptest-worthy fields
    let proptest_files = (
        try {
            rg -l "pub.*String|pub.*Vec<|pub.*u32|pub.*u64|pub.*i64|pub.*usize" $src_path
            | lines
            | where {|l| ($l | str length) > 0 }
            | length
        } catch { 0 }
    )

    let proptest_candidates = (
        try {
            rg -l "pub.*String|pub.*Vec<|pub.*u32|pub.*u64|pub.*i64|pub.*usize" $src_path
            | lines
            | where {|l| ($l | str length) > 0 }
        } catch { [] }
    )

    # Files with error types
    let error_files = (
        try {
            rg -l "thiserror|#\[error" $src_path
            | lines
            | where {|l| ($l | str length) > 0 }
            | length
        } catch { 0 }
    )

    # ── Check coverage ────────────────────────────────────────────────────────

    let has_serde_tests = (
        $planned_names | any {|n| ("serde" in $n) or ("roundtrip" in $n) }
    )

    let has_rkyv_tests = (
        $planned_names | any {|n| "rkyv" in $n }
    )

    let has_proptests = (
        $planned_names | any {|n| ("prop" in $n) or ("proptest" in $n) or ("arbitrary" in $n) }
    )

    let has_error_tests = (
        $planned_names | any {|n| ("error" in $n) or ("display" in $n) or ("debug" in $n) }
    )

    # ── Compute gaps ──────────────────────────────────────────────────────────

    let mut gaps = []

    if $serde_files > 0 and not $has_serde_tests {
        gaps = ($gaps | append $"($serde_files) source files have serde derives but no serde roundtrip tests are planned. Add tests named like 'serde_roundtrip_<type>' for each type.")
    }

    if $rkyv_files > 0 and not $has_rkyv_tests {
        gaps = ($gaps | append $"($rkyv_files) source files have rkyv Archive derives but no rkyv roundtrip tests are planned. Add tests named like 'rkyv_roundtrip_<type>' for each type.")
    }

    if $proptest_files > 0 and not $has_proptests {
        gaps = ($gaps | append $"($proptest_files) source files have proptest-worthy fields (String, Vec, numeric) but no proptests are planned. Add at least one proptest using the proptest! macro.")
    }

    if $error_files > 0 and not $has_error_tests {
        gaps = ($gaps | append $"($error_files) source files have error types (thiserror) but no Display or Debug format tests are planned. Add tests that verify error messages.")
    }

    let minimum_required = (
        [$serde_files $rkyv_files] | math max
    )

    if $planned_count < $minimum_required {
        gaps = ($gaps | append $"planned_count ($planned_count) is less than minimum_required ($minimum_required). Need at least one test per public type with derives.")
    }

    # ── Quality score ─────────────────────────────────────────────────────────

    let checks = [
        ($serde_files == 0 or $has_serde_tests),
        ($rkyv_files == 0 or $has_rkyv_tests),
        ($proptest_files == 0 or $has_proptests),
        ($error_files == 0 or $has_error_tests),
        ($planned_count >= $minimum_required),
    ]

    let passed = ($checks | where {|c| $c } | length)
    let total = ($checks | length)
    let quality_score = ($passed * 100 / $total)

    let approved = (($gaps | length) == 0)

    {
        ok: true,
        data: {
            approved: $approved,
            gaps: $gaps,
            proptest_candidates: $proptest_candidates,
            planned_count: $planned_count,
            minimum_required: $minimum_required,
            quality_score: $quality_score,
            serde_files: $serde_files,
            rkyv_files: $rkyv_files,
            error_files: $error_files,
            proptest_files: $proptest_files,
        },
        error: null
    }
}
