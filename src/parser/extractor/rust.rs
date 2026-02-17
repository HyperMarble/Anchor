//! Rust-specific symbol extraction.

use tree_sitter::Node;

use super::helpers::{bounded_snippet, get_call_name, node_name, node_text};
use crate::graph::types::*;

pub fn extract_rust_node(
    node: &Node,
    source: &[u8],
    kind: &str,
    current_scope: Option<&str>,
    symbols: &mut Vec<ExtractedSymbol>,
    imports: &mut Vec<ExtractedImport>,
    calls: &mut Vec<ExtractedCall>,
) {
    match kind {
        "function_item" => {
            if let Some(name) = node_name(node, source) {
                let parent_scope = current_scope.map(|s| s.to_string());
                let sym_kind = if parent_scope.is_some() {
                    NodeKind::Method
                } else {
                    NodeKind::Function
                };

                symbols.push(ExtractedSymbol {
                    name,
                    kind: sym_kind,
                    line_start: node.start_position().row + 1,
                    line_end: node.end_position().row + 1,
                    code_snippet: bounded_snippet(node, source),
                    parent: parent_scope,
                });
            }
        }
        "struct_item" => {
            if let Some(name) = node_name(node, source) {
                symbols.push(ExtractedSymbol {
                    name,
                    kind: NodeKind::Struct,
                    line_start: node.start_position().row + 1,
                    line_end: node.end_position().row + 1,
                    code_snippet: bounded_snippet(node, source),
                    parent: None,
                });
            }
        }
        "enum_item" => {
            if let Some(name) = node_name(node, source) {
                symbols.push(ExtractedSymbol {
                    name,
                    kind: NodeKind::Enum,
                    line_start: node.start_position().row + 1,
                    line_end: node.end_position().row + 1,
                    code_snippet: bounded_snippet(node, source),
                    parent: None,
                });
            }
        }
        "trait_item" => {
            if let Some(name) = node_name(node, source) {
                symbols.push(ExtractedSymbol {
                    name,
                    kind: NodeKind::Trait,
                    line_start: node.start_position().row + 1,
                    line_end: node.end_position().row + 1,
                    code_snippet: bounded_snippet(node, source),
                    parent: None,
                });
            }
        }
        "impl_item" => {
            if let Some(name) = get_rust_impl_name(node, source) {
                symbols.push(ExtractedSymbol {
                    name,
                    kind: NodeKind::Impl,
                    line_start: node.start_position().row + 1,
                    line_end: node.end_position().row + 1,
                    code_snippet: bounded_snippet(node, source),
                    parent: None,
                });
            }
        }
        "const_item" | "static_item" => {
            if let Some(name) = node_name(node, source) {
                symbols.push(ExtractedSymbol {
                    name,
                    kind: NodeKind::Constant,
                    line_start: node.start_position().row + 1,
                    line_end: node.end_position().row + 1,
                    code_snippet: bounded_snippet(node, source),
                    parent: current_scope.map(|s| s.to_string()),
                });
            }
        }
        "type_item" => {
            if let Some(name) = node_name(node, source) {
                symbols.push(ExtractedSymbol {
                    name,
                    kind: NodeKind::Type,
                    line_start: node.start_position().row + 1,
                    line_end: node.end_position().row + 1,
                    code_snippet: bounded_snippet(node, source),
                    parent: None,
                });
            }
        }
        "mod_item" => {
            if let Some(name) = node_name(node, source) {
                symbols.push(ExtractedSymbol {
                    name,
                    kind: NodeKind::Module,
                    line_start: node.start_position().row + 1,
                    line_end: node.end_position().row + 1,
                    code_snippet: bounded_snippet(node, source),
                    parent: None,
                });
            }
        }
        "use_declaration" => {
            let text = node_text(node, source);
            let path = text
                .trim_start_matches("use ")
                .trim_end_matches(';')
                .trim()
                .to_string();

            imports.push(ExtractedImport {
                path,
                symbols: Vec::new(),
                line: node.start_position().row + 1,
            });
        }
        "call_expression" => {
            if let Some(callee_name) = get_call_name(node, source) {
                if let Some(caller) = current_scope {
                    calls.push(ExtractedCall {
                        callee: callee_name,
                        caller: caller.to_string(),
                        line: node.start_position().row + 1,
                        line_end: node.end_position().row + 1,
                    });
                }
            }
        }
        _ => {}
    }
}

/// Get the type name from a Rust impl block.
/// Handles `impl Foo` and `impl Trait for Foo`.
pub fn get_rust_impl_name(node: &Node, source: &[u8]) -> Option<String> {
    if let Some(type_node) = node.child_by_field_name("type") {
        return type_node.utf8_text(source).ok().map(|s| s.to_string());
    }
    let text = node_text(node, source);
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() >= 2 {
        if parts.contains(&"for") {
            parts
                .iter()
                .position(|&p| p == "for")
                .and_then(|i| parts.get(i + 1))
                .map(|s| s.trim_end_matches('{').trim().to_string())
        } else {
            Some(
                parts[1]
                    .trim_end_matches('{')
                    .trim_end_matches('<')
                    .to_string(),
            )
        }
    } else {
        None
    }
}
