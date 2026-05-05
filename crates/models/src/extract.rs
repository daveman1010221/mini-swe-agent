//! Extract a `ToolCall` from raw model output.
//!
//! Models emit reasoning text followed by a JSON tool call, e.g.:
//!
//! ```text
//! I'll check the directory structure first.
//! {"type":"shell","command":"ls -la"}
//! ```
//!
//! Strategy: scan for all JSON objects in the text and attempt to deserialize
//! each against the `ToolCall` schema. Return the *last* successful parse —
//! models reason before acting, so the call comes at the end.

use mswea_core::ToolCall;

/// All failure modes when trying to extract a `ToolCall` from text.
#[derive(Debug, thiserror::Error)]
pub enum ExtractionError {
    #[error("no JSON object found in model output")]
    NoJson,
    #[error("JSON found but does not match any ToolCall variant: {0}")]
    ParseFailed(String),
}

/// Extract the last valid `ToolCall` from a model response string.
pub fn extract_tool_call(text: &str) -> Result<ToolCall, ExtractionError> {
    let candidates = find_json_objects(text);
    if candidates.is_empty() {
        return Err(ExtractionError::NoJson);
    }
    let mut last_error = String::new();
    for candidate in candidates.iter().rev() {
        let normalized = normalize_submit(candidate);
        let normalized = normalize_builtin_misrouting(&normalized);
        match serde_json::from_str::<ToolCall>(&normalized) {
            Ok(call) => return Ok(call),
            Err(e) => last_error = e.to_string(),
        }
    }
    Err(ExtractionError::ParseFailed(last_error))
}

/// Normalize misrouted builtin calls.
/// Models emit {"type":"nushell_tool","namespace":"read|write|edit|search|shell",...}
/// instead of the correct {"type":"read|write|edit|search|shell",...}.
fn normalize_builtin_misrouting(json: &str) -> String {
    const BUILTIN_NAMESPACES: &[&str] = &["read", "write", "edit", "search", "shell"];
    
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                let is_nushell_tool = obj.get("type")
                    .and_then(|t| t.as_str()) == Some("nushell_tool");
                if is_nushell_tool {
                    if let Some(ns) = obj.get("namespace")
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string())
                    {
                        if BUILTIN_NAMESPACES.contains(&ns.as_str()) {
                            obj.insert("type".into(), serde_json::json!(ns));
                            obj.remove("namespace");
                            obj.remove("tool");
                        }
                    }
                }
            }
            v.to_string()
        }
        Err(_) => json.to_string(),
    }
}

/// Normalize submit tool calls that use non-standard field names.
/// Models occasionally produce {"type":"submit","result":...} or {"answer":...}
/// instead of the required {"type":"submit","output":...}.
fn normalize_submit(json: &str) -> String {
    // Quick check — only process objects that look like submit calls.
    if !json.contains(r#""submit""#) {
        return json.to_string();
    }
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                if obj.get("type").and_then(|t| t.as_str()) == Some("submit") {
                    // If "output" is missing but "result" or "answer" is present,
                    // rename it to "output".
                    if !obj.contains_key("output") {
                        let val = obj
                            .remove("result")
                            .or_else(|| obj.remove("answer"))
                            .or_else(|| obj.remove("content"));
                        if let Some(val) = val {
                            obj.insert("output".into(), val);
                        }
                    }
                }
            }
            v.to_string()
        }
        Err(_) => json.to_string(),
    }
}

/// Find all top-level JSON object substrings `{...}` in `text`.
/// Handles nested braces correctly. Does not handle JSON arrays at top level
/// (tool calls are always objects).
fn find_json_objects(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut results = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'{' {
            let start = i;
            let mut depth = 0usize;
            let mut in_string = false;
            let mut escape_next = false;

            while i < bytes.len() {
                let b = bytes[i];

                if escape_next {
                    escape_next = false;
                } else if in_string {
                    match b {
                        b'\\' => escape_next = true,
                        b'"'  => in_string = false,
                        _     => {}
                    }
                } else {
                    match b {
                        b'"' => in_string = true,
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                results.push(&text[start..=i]);
                                i += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    results
}
