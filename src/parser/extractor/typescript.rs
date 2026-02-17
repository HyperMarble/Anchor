//! TypeScript-specific symbol extraction.
//! Delegates to JavaScript extractor, then adds TS-only nodes.

use tree_sitter::Node;

use super::helpers::{bounded_snippet, node_name};
use super::javascript::extract_js_node;
use crate::graph::types::*;

pub fn extract_ts_node(
    node: &Node,
    source: &[u8],
    kind: &str,
    current_scope: Option<&str>,
    symbols: &mut Vec<ExtractedSymbol>,
    imports: &mut Vec<ExtractedImport>,
    calls: &mut Vec<ExtractedCall>,
) {
    // TypeScript shares most node kinds with JavaScript
    extract_js_node(node, source, kind, current_scope, symbols, imports, calls);

    // TypeScript-specific nodes
    match kind {
        "interface_declaration" => {
            if let Some(name) = node_name(node, source) {
                symbols.push(ExtractedSymbol {
                    name,
                    kind: NodeKind::Interface,
                    line_start: node.start_position().row + 1,
                    line_end: node.end_position().row + 1,
                    code_snippet: bounded_snippet(node, source),
                    parent: None,
                });
            }
        }
        "type_alias_declaration" => {
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
        "enum_declaration" => {
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
        _ => {}
    }
}
