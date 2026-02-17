//! Shared helper functions for all language extractors.

use tree_sitter::Node;

/// Get the name of a node from its "name" field.
pub fn node_name(node: &Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.to_string())
}

/// Get the full text of a node.
pub fn node_text(node: &Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").to_string()
}

/// Extract full code snippet from a node (no truncation).
/// Slicing happens at display time using graph knowledge.
pub fn bounded_snippet(node: &Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").to_string()
}

/// Get the function name from a call_expression node (Rust/JS/TS/generic).
pub fn get_call_name(node: &Node, source: &[u8]) -> Option<String> {
    let func_node = node.child_by_field_name("function")?;
    let text = func_node.utf8_text(source).ok()?;

    // Handle method calls: obj.method() -> "method"
    // Handle simple calls: func() -> "func"
    // Handle namespaced: mod::func() -> "func"
    let name = text.rsplit(['.', ':']).next().unwrap_or(text).trim();

    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}
