#!/usr/bin/env nu
# task/write-coverage-plan.nu
#
# Called during ORIENT phase before writing any tests.
# Documents the agent's reasoning about what needs to be tested.
# Becomes the verification contract for task/advance in finalize step.
# Do not advance from orient step without calling this first.
#
# Usage:
#   nu tools/task/write-coverage-plan.nu \
#     --taskfile /workspace/agent-task.json \
#     --public-interfaces '["MyStruct", "MyEnum", "my_fn"]' \
#     --serde-required true \
#     --rkyv-required true \
#     --existing-tests 0 \
#     --planned-tests '[{"name":"test_my_struct_serde","type":"serde_roundtrip","rationale":"MyStruct derives Serialize/Deserialize"}]'

def main [
    --taskfile: path = "",
    --public-interfaces: string = "[]",   # JSON array of interface names
    --failure-modes: string = "[]",        # JSON array of {interface, modes}
    --boundary-conditions: string = "[]",  # JSON array of strings
    --serde-required: bool = false,
    --rkyv-required: bool = false,
    --existing-tests: int = 0,
    --planned-tests: string = "[]",        # JSON array of {name, type, rationale}
] {
    let tf_path = if ($taskfile | str length) > 0 {
        $taskfile
    } else if ("TASKFILE" in $env) {
        $env.TASKFILE
    } else {
        ""
    }

    if ($tf_path | str length) == 0 {
        return { ok: false, data: null, error: "no taskfile path" }
    }

    let tf = (
        try { open $tf_path | from json }
        catch {|err| return { ok: false, data: null, error: $"failed to parse taskfile: ($err.msg)" }}
    )

    let current = ($tf | get current_task? | default null)
    if $current == null {
        return { ok: false, data: null, error: "no current task" }
    }

    # Parse JSON arrays
    let interfaces = (try { $public_interfaces | from json } catch { [] })
    let failure_modes_parsed = (try { $failure_modes | from json } catch { [] })
    let boundary = (try { $boundary_conditions | from json } catch { [] })
    let planned = (try { $planned_tests | from json } catch { [] })

    if ($planned | length) == 0 {
        return {
            ok: false,
            data: null,
            error: "planned-tests cannot be empty — coverage plan requires at least one planned test"
        }
    }

    let coverage_plan = {
        public_interfaces: $interfaces,
        failure_modes: $failure_modes_parsed,
        boundary_conditions: $boundary,
        serde_required: $serde_required,
        rkyv_required: $rkyv_required,
        existing_tests: $existing_tests,
        planned_tests: $planned,
        written_at: (date now | format date "%Y-%m-%dT%H:%M:%SZ"),
    }

    let updated_current = $current | upsert coverage_plan $coverage_plan

    let updated_tf = $tf
        | upsert current_task $updated_current
        | upsert last_updated (date now | format date "%Y-%m-%dT%H:%M:%SZ")

    try {
        $updated_tf | to json | save --force $tf_path
    } catch {|err|
        return { ok: false, data: null, error: $"failed to write taskfile: ($err.msg)" }
    }

    {
        ok: true,
        data: {
            plan_recorded: true,
            planned_count: ($planned | length),
            serde_tests: (if $serde_required { $interfaces | length } else { 0 }),
            rkyv_tests: (if $rkyv_required { $interfaces | length } else { 0 }),
        },
        error: null
    }
}
