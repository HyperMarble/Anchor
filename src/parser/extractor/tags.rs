//
//  tags.rs
//  Anchor
//
//  Created by hak (tharun)
//

use std::collections::HashSet;

use tree_sitter::{Language, Node, Query, QueryCursor, StreamingIterator, Tree};
use tracing::warn;

use super::helpers::{bounded_snippet, node_text};
use crate::graph::types::*;
use crate::parser::language::SupportedLanguage;

/// Map a tags capture name to NodeKind.
fn capture_to_kind(name: &str) -> Option<NodeKind> {
    match name {
        "definition.function" => Some(NodeKind::Function),
        "definition.method" => Some(NodeKind::Method),
        "definition.class" => Some(NodeKind::Class),
        "definition.interface" => Some(NodeKind::Interface),
        "definition.module" => Some(NodeKind::Module),
        "definition.macro" => Some(NodeKind::Function),
        "definition.constant" => Some(NodeKind::Constant),
        "definition.type" => Some(NodeKind::Type),
        "definition.property" => Some(NodeKind::Variable),
        "reference.implementation" => Some(NodeKind::Impl),
        _ => None,
    }
}

/// Refine NodeKind using the actual AST node type.
/// Tags queries often map different constructs to the same capture
/// (e.g. Rust struct/enum/union all → @definition.class). This restores precision.
fn precise_kind(node_kind: &str, capture_kind: NodeKind) -> NodeKind {
    match node_kind {
        // Rust
        "struct_item" | "struct_specifier" => NodeKind::Struct,
        "enum_item" | "enum_specifier" => NodeKind::Enum,
        "trait_item" => NodeKind::Trait,
        "impl_item" => NodeKind::Impl,
        "type_item" => NodeKind::Type,
        "const_item" | "static_item" => NodeKind::Constant,

        // TypeScript
        "interface_declaration" => NodeKind::Interface,
        "type_alias_declaration" => NodeKind::Type,
        "enum_declaration" => NodeKind::Enum,

        // General
        "mod_item" | "module" => NodeKind::Module,

        _ => capture_kind,
    }
}

/// Extract symbols and calls from a parsed tree using a tags query.
pub fn extract_with_tags(
    tree: &Tree,
    source: &[u8],
    query_src: &str,
    ts_lang: &Language,
) -> (Vec<ExtractedSymbol>, Vec<ExtractedCall>) {
    let query = match Query::new(ts_lang, query_src) {
        Ok(q) => q,
        Err(e) => {
            warn!("failed to compile tags query: {e}");
            return (Vec::new(), Vec::new());
        }
    };

    let capture_names: Vec<&str> = query.capture_names().to_vec();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source);

    let mut symbols = Vec::new();
    let mut calls = Vec::new();
    let mut seen_defs: HashSet<usize> = HashSet::new();

    while let Some(m) = matches.next() {
        let mut name_text: Option<String> = None;
        let mut name_node: Option<Node> = None;
        let mut def_kind: Option<NodeKind> = None;
        let mut def_node: Option<Node> = None;
        let mut is_call = false;

        for capture in m.captures {
            let cap_name = capture_names[capture.index as usize];

            if cap_name == "name" {
                name_text = capture.node.utf8_text(source).ok().map(|s| s.to_string());
                name_node = Some(capture.node);
            } else if let Some(kind) = capture_to_kind(cap_name) {
                def_kind = Some(kind);
                def_node = Some(capture.node);
            } else if cap_name.starts_with("reference.call") || cap_name == "reference.send" {
                is_call = true;
            }
        }

        // Handle definitions
        if let (Some(ref name), Some(kind), Some(node)) = (&name_text, def_kind, def_node) {
            let node_id = node.id();
            if !seen_defs.contains(&node_id) {
                seen_defs.insert(node_id);
                let refined = precise_kind(node.kind(), kind);
                symbols.push(ExtractedSymbol {
                    name: name.clone(),
                    kind: refined,
                    line_start: node.start_position().row + 1,
                    line_end: node.end_position().row + 1,
                    code_snippet: bounded_snippet(&node, source),
                    parent: None,
                });
            }
        }

        // Handle calls
        if let (Some(ref name), true) = (&name_text, is_call) {
            let walk_node = name_node.unwrap_or(tree.root_node());
            if let Some(caller) = find_enclosing_scope(walk_node, source) {
                let line = walk_node.start_position().row + 1;
                let line_end = walk_node.end_position().row + 1;
                calls.push(ExtractedCall {
                    callee: name.clone(),
                    caller,
                    line,
                    line_end,
                });
            }
        }
    }

    resolve_parents(&mut symbols);

    (symbols, calls)
}

/// Walk up from a node to find the enclosing scope's name.
fn find_enclosing_scope(node: Node, source: &[u8]) -> Option<String> {
    let mut current = node.parent();
    while let Some(parent) = current {
        if is_scope_kind(parent.kind()) {
            // Try "name" field first (functions, classes, methods)
            if let Some(name_node) = parent.child_by_field_name("name") {
                if let Ok(name) = name_node.utf8_text(source) {
                    return Some(name.to_string());
                }
            }
            // Rust impl blocks: use "type" field
            if let Some(type_node) = parent.child_by_field_name("type") {
                if let Ok(name) = type_node.utf8_text(source) {
                    return Some(name.to_string());
                }
            }
        }
        current = parent.parent();
    }
    None
}

/// AST node kinds that create a scope for child symbols.
fn is_scope_kind(kind: &str) -> bool {
    matches!(
        kind,
        "function_item"
            | "function_definition"
            | "function_declaration"
            | "method_declaration"
            | "method_definition"
            | "method"
            | "class_definition"
            | "class_declaration"
            | "class_specifier"
            | "class"
            | "impl_item"
            | "trait_item"
            | "mod_item"
            | "module"
    )
}

/// Set parent for each symbol based on line-range containment.
/// If symbol B is fully inside container A, A is B's parent.
fn resolve_parents(symbols: &mut [ExtractedSymbol]) {
    let containers: Vec<(String, usize, usize)> = symbols
        .iter()
        .filter(|s| is_container(s.kind))
        .map(|s| (s.name.clone(), s.line_start, s.line_end))
        .collect();

    for sym in symbols.iter_mut() {
        if is_container(sym.kind) {
            continue;
        }
        // Find the smallest container that fully contains this symbol.
        let mut best: Option<&(String, usize, usize)> = None;
        for c in &containers {
            if c.1 <= sym.line_start && c.2 >= sym.line_end && c.0 != sym.name {
                match best {
                    Some(prev) if (c.2 - c.1) < (prev.2 - prev.1) => best = Some(c),
                    None => best = Some(c),
                    _ => {}
                }
            }
        }
        if let Some(parent) = best {
            sym.parent = Some(parent.0.clone());
        }
    }
}

fn is_container(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Class
            | NodeKind::Struct
            | NodeKind::Interface
            | NodeKind::Trait
            | NodeKind::Impl
            | NodeKind::Module
    )
}

/// Extract import statements by walking the AST.
/// Tags queries don't capture imports, so we handle them separately
/// with a simple per-language list of import node kinds.
pub fn extract_imports(
    root: &Node,
    source: &[u8],
    lang: SupportedLanguage,
) -> Vec<ExtractedImport> {
    let import_kinds: &[&str] = match lang {
        SupportedLanguage::Rust => &["use_declaration"],
        SupportedLanguage::Python => &["import_statement", "import_from_statement"],
        SupportedLanguage::JavaScript
        | SupportedLanguage::Tsx
        | SupportedLanguage::TypeScript => &["import_statement"],
        SupportedLanguage::Go => &["import_declaration"],
        SupportedLanguage::Java => &["import_declaration"],
        SupportedLanguage::CSharp => &["using_directive"],
        SupportedLanguage::Ruby => &[],
        SupportedLanguage::Cpp => &["preproc_include"],
        SupportedLanguage::Swift => &["import_declaration"],
    };

    let mut imports = Vec::new();
    collect_imports(root, source, import_kinds, &mut imports);
    imports
}

fn collect_imports(
    node: &Node,
    source: &[u8],
    kinds: &[&str],
    imports: &mut Vec<ExtractedImport>,
) {
    if kinds.contains(&node.kind()) {
        let text = node_text(node, source);
        let path = clean_import_path(&text);
        if !path.is_empty() {
            imports.push(ExtractedImport {
                path,
                symbols: Vec::new(),
                line: node.start_position().row + 1,
            });
        }
        return;
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_imports(&child, source, kinds, imports);
        }
    }
}

/// Strip language-specific import syntax to get the import path.
fn clean_import_path(text: &str) -> String {
    let stripped = text
        .trim_start_matches("use ")
        .trim_start_matches("import ")
        .trim_start_matches("from ")
        .trim_start_matches("using ")
        .trim_start_matches("#include ")
        .trim_end_matches(';')
        .trim();

    // JS/TS: "import { X } from 'path'" → extract path after "from"
    if let Some(idx) = stripped.find(" from ") {
        return stripped[idx + 6..]
            .trim()
            .trim_matches(|c| c == '\'' || c == '"')
            .to_string();
    }

    stripped.to_string()
}
