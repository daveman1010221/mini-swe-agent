#!/usr/bin/env nu
# task/write-coverage-plan.nu — record coverage plan via mswea plugin
#
# Called during ORIENT phase before writing any tests.
# Documents the agent's reasoning about what needs to be tested.
#
# Usage:
#   nu tools/task/write-coverage-plan.nu \
#     --public-interfaces '["MyStruct"]' \
#     --serde-required true \
#     --rkyv-required true \
#     --existing-tests 0 \
#     --planned-tests '[{"name":"test_foo","type":"serde_roundtrip","rationale":"..."}]'

def main [
    --taskfile: path = "",
    --public-interfaces: string = "[]",
    --failure-modes: string = "[]",
    --boundary-conditions: string = "[]",
    --serde-required,
    --rkyv-required,
    --existing-tests: int = 0,
    --planned-tests: string = "[]",
] {
    let planned = (
        try { $planned_tests | from json } catch { [] }
        | each {|t|
            if ($t | describe) == "string" {
                {name: $t, type: "unit", rationale: "auto-coerced from string"}
            } else {
                $t
            }
        }
    )

    if ($planned | length) == 0 {
        return { ok: false, data: null, error: "planned-tests cannot be empty" }
    }

    let plan = {
        public_interfaces: (try { $public_interfaces | from json } catch { [] }),
        failure_modes: (try { $failure_modes | from json } catch { [] }),
        boundary_conditions: (try { $boundary_conditions | from json } catch { [] }),
        serde_required: $serde_required,
        rkyv_required: $rkyv_required,
        existing_tests: $existing_tests,
        planned_tests: $planned,
    }

    let result = (mswea rpc write-coverage-plan $plan)

    if not $result.ok {
        return { ok: false, data: null, error: $result.error }
    }

    {
        ok: true,
        data: {
            plan_recorded: $result.plan_recorded,
            planned_count: $result.planned_count,
            serde_tests: (if $serde_required { $planned | length } else { 0 }),
            rkyv_tests: (if $rkyv_required { $planned | length } else { 0 }),
        },
        error: null
    }
}
