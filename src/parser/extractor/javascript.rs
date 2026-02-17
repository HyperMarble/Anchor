//! JavaScript-specific symbol extraction (also used by TSX/JSX).

use tree_sitter::Node;

use super::helpers::{bounded_snippet, get_call_name, node_name, node_text};
use crate::graph::types::*;

pub fn extract_js_node(
    node: &Node,
    source: &[u8],
    kind: &str,
    current_scope: Option<&str>,
    symbols: &mut Vec<ExtractedSymbol>,
    imports: &mut Vec<ExtractedImport>,
    calls: &mut Vec<ExtractedCall>,
) {
    match kind {
        "function_declaration" => {
            if let Some(name) = node_name(node, source) {
                symbols.push(ExtractedSymbol {
                    name,
                    kind: NodeKind::Function,
                    line_start: node.start_position().row + 1,
                    line_end: node.end_position().row + 1,
                    code_snippet: bounded_snippet(node, source),
                    parent: current_scope.map(|s| s.to_string()),
                });
            }
        }
        "class_declaration" => {
            if let Some(name) = node_name(node, source) {
                symbols.push(ExtractedSymbol {
                    name,
                    kind: NodeKind::Class,
                    line_start: node.start_position().row + 1,
                    line_end: node.end_position().row + 1,
                    code_snippet: bounded_snippet(node, source),
                    parent: None,
                });
            }
        }
        "method_definition" => {
            if let Some(name) = node_name(node, source) {
                symbols.push(ExtractedSymbol {
                    name,
                    kind: NodeKind::Method,
                    line_start: node.start_position().row + 1,
                    line_end: node.end_position().row + 1,
                    code_snippet: bounded_snippet(node, source),
                    parent: current_scope.map(|s| s.to_string()),
                });
            }
        }
        "lexical_declaration" | "variable_declaration" => {
            extract_js_variable_declaration(node, source, current_scope, symbols);
        }
        "import_statement" => {
            extract_js_import(node, source, imports);
        }
        "export_statement" => {
            // Exports may contain declarations — let children handle extraction
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

/// Extract variable declarations that define functions or constants.
pub fn extract_js_variable_declaration(
    node: &Node,
    source: &[u8],
    current_scope: Option<&str>,
    symbols: &mut Vec<ExtractedSymbol>,
) {
    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(declarator) = node.child(i) {
            if declarator.kind() == "variable_declarator" {
                let name = node_name(&declarator, source);
                let value = declarator.child_by_field_name("value");

                if let (Some(name), Some(value)) = (name, value) {
                    let kind = match value.kind() {
                        "arrow_function" | "function" => NodeKind::Function,
                        _ => {
                            if name.chars().all(|c| c.is_uppercase() || c == '_') {
                                NodeKind::Constant
                            } else {
                                NodeKind::Variable
                            }
                        }
                    };

                    symbols.push(ExtractedSymbol {
                        name,
                        kind,
                        line_start: node.start_position().row + 1,
                        line_end: node.end_position().row + 1,
                        code_snippet: bounded_snippet(node, source),
                        parent: current_scope.map(|s| s.to_string()),
                    });
                }
            }
        }
    }
}

/// Extract JS/TS import statements.
pub fn extract_js_import(node: &Node, source: &[u8], imports: &mut Vec<ExtractedImport>) {
    let text = node_text(node, source);

    let path = text
        .rsplit("from")
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches(|c| c == '\'' || c == '"' || c == ';' || c == ' ')
        .to_string();

    let syms: Vec<String> = if text.contains('{') {
        text.split('{')
            .nth(1)
            .unwrap_or("")
            .split('}')
            .next()
            .unwrap_or("")
            .split(',')
            .map(|s| s.split(" as ").next().unwrap_or("").trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        Vec::new()
    };

    if !path.is_empty() {
        imports.push(ExtractedImport {
            path,
            symbols: syms,
            line: node.start_position().row + 1,
        });
    }
}
