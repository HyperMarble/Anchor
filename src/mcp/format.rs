//! Output formatting helpers for MCP tool responses.

use std::path::Path;

pub fn escape_graphql(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

pub fn format_symbol(output: &mut String, sym: &serde_json::Value) {
    let name = sym.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let kind = sym.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    let file = sym.get("file").and_then(|v| v.as_str()).unwrap_or("");
    let line = sym.get("line").and_then(|v| v.as_i64()).unwrap_or(0);

    let file_name = Path::new(file)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| file.to_string());

    output.push_str(&format!("{} {} {}:{}\n", name, kind, file_name, line));

    // Callers
    if let Some(callers) = sym.get("callers").and_then(|c| c.as_array()) {
        let mut names: Vec<&str> = callers.iter()
            .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
            .filter(|n| !is_file_name(n))
            .collect();
        names.sort();
        names.dedup();
        if !names.is_empty() {
            output.push_str(&format!("> {}\n", names.join(" ")));
        }
    }

    // Callees
    if let Some(callees) = sym.get("callees").and_then(|c| c.as_array()) {
        let mut names: Vec<&str> = callees.iter()
            .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
            .filter(|n| !is_file_name(n))
            .collect();
        names.sort();
        names.dedup();
        if !names.is_empty() {
            output.push_str(&format!("< {}\n", names.join(" ")));
        }
    }

    // Code
    if let Some(code) = sym.get("code").and_then(|c| c.as_str()) {
        output.push_str("---\n");
        output.push_str(code);
        output.push('\n');
    }
}

pub fn is_file_name(s: &str) -> bool {
    s.ends_with(".rs") || s.ends_with(".py") || s.ends_with(".js") || s.ends_with(".ts")
}

pub fn short_kind(kind: &str) -> &str {
    match kind {
        "function" => "fn",
        "method" => "m",
        "struct" => "st",
        "class" => "cl",
        "trait" => "tr",
        "interface" => "if",
        "enum" => "en",
        "constant" => "c",
        "module" => "mod",
        "type" => "ty",
        "variable" => "v",
        "impl" => "impl",
        _ => kind,
    }
}
