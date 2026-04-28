#!/usr/bin/env nu
# compile/fix-hint.nu
#
# Given a specific compiler error code, returns structured fix guidance.
# Interprets common Rust error codes into actionable steps.
# Does NOT make the fix — tells the agent exactly what to do.
# Call this after compile/check returns errors.
#
# Usage:
#   nu tools/compile/fix-hint.nu --error-code E0308 --message "expected `String`, found `&str`"
#   nu tools/compile/fix-hint.nu --error-code E0433 --message "cannot find module `anyhow`"

def main [
    --error-code: string,    # e.g. "E0308"
    --message: string = "",  # compiler error message for additional context
    --context: string = ""   # surrounding code if available
] {
    if ($error_code | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --error-code" }
    }

    # Known error patterns with fix guidance
    let hint = match $error_code {
        "E0308" => {
            pattern: "type-mismatch",
            description: "Type mismatch — expected one type, found another",
            fix_steps: [
                "Read the full error message to identify expected vs found types",
                "Use .to_string() if converting &str → String",
                "Use .as_str() or .as_deref() if converting String → &str",
                "Use .into() if the types have a From impl",
                "Use .clone() if you need an owned copy",
                "Check if you need to dereference with *",
            ],
            tools_to_use: ["extract/range to read the problematic line and context"],
            example: "let s: String = my_str.to_string();  // &str → String\nlet s: &str = &my_string;  // String → &str",
        }
        "E0433" | "E0432" => {
            pattern: "missing-import",
            description: "Cannot find module, type, or crate in scope",
            fix_steps: [
                "Check if the crate is in Cargo.toml dependencies",
                "If missing from Cargo.toml — check workspace Cargo.toml first, use workspace = true form",
                "Add the correct use statement at the top of the file",
                "Check the exact module path with locate/symbols",
            ],
            tools_to_use: ["locate/deps to check Cargo.toml", "locate/symbols to find the correct path"],
            example: "use mswea_core::config::TaskFile;  // correct use path",
        }
        "E0277" => {
            pattern: "trait-not-implemented",
            description: "The trait bound is not satisfied",
            fix_steps: [
                "Identify which trait is missing from the error message",
                "Check if the type needs a #[derive(TraitName)] added",
                "For Archive/rkyv — serde_json::Value cannot be archived, use String instead",
                "For Send/Sync — check if you're crossing async boundaries",
                "For Clone — add #[derive(Clone)] to the type",
            ],
            tools_to_use: ["extract/symbol to read the type definition", "locate/derives to check current derives"],
            example: "#[derive(Debug, Clone, Serialize, Deserialize)]  // add missing derives",
        }
        "E0382" => {
            pattern: "use-after-move",
            description: "Value used after it was moved",
            fix_steps: [
                "Add .clone() before the first use if you need the value in both places",
                "Use a reference &value instead of moving",
                "Restructure code so the value is only used once",
                "Use Arc<T> if sharing across threads",
            ],
            tools_to_use: ["extract/range to read the full context around the error"],
            example: "let cloned = value.clone();  // clone before first use",
        }
        "E0499" | "E0502" | "E0505" => {
            pattern: "borrow-conflict",
            description: "Cannot borrow as mutable/immutable — conflicting borrows",
            fix_steps: [
                "Ensure mutable and immutable borrows don't overlap",
                "Use a block scope to limit the lifetime of borrows",
                "Consider using .clone() to avoid the conflict",
                "Use RefCell<T> for interior mutability if needed",
            ],
            tools_to_use: ["extract/range to read surrounding borrow context"],
            example: "{\n    let x = &data;  // immutable borrow\n    use_x(x);\n}  // borrow ends here\ndata.mutate();  // now safe",
        }
        "E0063" => {
            pattern: "missing-struct-field",
            description: "Missing field in struct initializer",
            fix_steps: [
                "Read the error to find which field is missing",
                "Add the missing field to the struct literal",
                "Use extract/symbol to read the full struct definition and find all fields",
                "If the field has a Default impl, consider using ..Default::default()",
            ],
            tools_to_use: ["extract/symbol to read the full struct definition"],
            example: "MyStruct { existing_field: val, missing_field: default_val }",
        }
        "E0425" => {
            pattern: "unresolved-name",
            description: "Cannot find value, function, or module in scope",
            fix_steps: [
                "Check the spelling and case of the name",
                "Verify the correct use statement is present",
                "Check if the item needs to be pub to be accessible",
                "Use locate/symbols to find the exact name and path",
            ],
            tools_to_use: ["locate/symbols", "extract/file to check use statements at top"],
            example: "use crate::module::MyType;  // add missing use",
        }
        "E0560" => {
            pattern: "unknown-struct-field",
            description: "Struct has no field with this name",
            fix_steps: [
                "Use extract/symbol to read the actual struct definition",
                "Check spelling — Rust field names are case-sensitive",
                "The field may have been renamed in a recent change",
                "Check if you're constructing the wrong struct",
            ],
            tools_to_use: ["extract/symbol to read the exact struct definition with all fields"],
            example: "// Read the struct definition first:\n// extract/symbol --symbol MyStruct --kind struct",
        }
        _ => {
            pattern: "unknown",
            description: $"Error code ($error_code) — check rustc --explain ($error_code) for details",
            fix_steps: [
                $"Run: rustc --explain ($error_code) for the official explanation",
                "Read the full error message carefully — it usually contains the fix",
                "Use extract/range to read the code context around the error",
                "Check the Rust reference or error index for this code",
            ],
            tools_to_use: ["extract/range to read error context"],
            example: "",
        }
    }

    {
        ok: true,
        data: {
            error_code: $error_code,
            message_context: $message,
            pattern: ($hint | get pattern),
            description: ($hint | get description),
            fix_steps: ($hint | get fix_steps),
            tools_to_use: ($hint | get tools_to_use),
            example: ($hint | get example),
        },
        error: null
    }
}
