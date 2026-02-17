//! Generic symbol extraction for languages without dedicated extractors.
//! Used by: Go, Java, C#, Ruby, C++, Swift.

use tree_sitter::Node;

use super::helpers::{bounded_snippet, get_call_name, node_name, node_text};
use crate::graph::types::*;

#[allow(clippy::too_many_arguments)]
pub fn extract_generic_node(
    node: &Node,
    source: &[u8],
    kind: &str,
    current_scope: Option<&str>,
    symbols: &mut Vec<ExtractedSymbol>,
    imports: &mut Vec<ExtractedImport>,
    calls: &mut Vec<ExtractedCall>,
    func_kinds: &[&str],
    import_kinds: &[&str],
    call_kinds: &[&str],
) {
    if func_kinds.contains(&kind) {
        if let Some(name) = node_name(node, source) {
            let sym_kind = if current_scope.is_some() {
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
                parent: current_scope.map(|s| s.to_string()),
            });
        }
    }

    if import_kinds.contains(&kind) {
        let text = node_text(node, source);
        imports.push(ExtractedImport {
            path: text.trim().to_string(),
            symbols: Vec::new(),
            line: node.start_position().row + 1,
        });
    }

    if call_kinds.contains(&kind) {
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
}
