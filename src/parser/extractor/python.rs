//! Python-specific symbol extraction.

use tree_sitter::Node;

use super::helpers::{bounded_snippet, node_name, node_text};
use crate::graph::types::*;

pub fn extract_python_node(
    node: &Node,
    source: &[u8],
    kind: &str,
    current_scope: Option<&str>,
    symbols: &mut Vec<ExtractedSymbol>,
    imports: &mut Vec<ExtractedImport>,
    calls: &mut Vec<ExtractedCall>,
) {
    match kind {
        "function_definition" => {
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
        "class_definition" => {
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
        "import_statement" => {
            let text = node_text(node, source);
            let path = text.trim_start_matches("import ").trim().to_string();
            imports.push(ExtractedImport {
                path,
                symbols: Vec::new(),
                line: node.start_position().row + 1,
            });
        }
        "import_from_statement" => {
            let text = node_text(node, source);
            let path = text
                .split("import")
                .next()
                .unwrap_or("")
                .trim_start_matches("from ")
                .trim()
                .to_string();
            let syms: Vec<String> = text
                .split("import")
                .nth(1)
                .unwrap_or("")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            imports.push(ExtractedImport {
                path,
                symbols: syms,
                line: node.start_position().row + 1,
            });
        }
        "call" => {
            if let Some(callee_name) = get_python_call_name(node, source) {
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

/// Get the function name from a Python call node.
fn get_python_call_name(node: &Node, source: &[u8]) -> Option<String> {
    let func_node = node.child_by_field_name("function")?;
    let text = func_node.utf8_text(source).ok()?;

    let name = text.rsplit('.').next().unwrap_or(text).trim();

    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}
