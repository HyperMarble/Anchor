//! Symbol extraction from source code using tree-sitter ASTs.
//!
//! Walks the AST of a source file and extracts:
//! - Symbol definitions (functions, structs, classes, etc.)
//! - Import statements
//! - Function calls (for building call graphs)

mod generic;
mod helpers;
mod javascript;
mod python;
mod rust;
mod typescript;

use std::path::Path;
use tree_sitter::{Node, Parser};

use super::language::SupportedLanguage;
use crate::error::AnchorError;
use crate::graph::types::*;
use helpers::node_name;
use rust::get_rust_impl_name;

/// Extract all symbols, imports, and calls from a source file.
///
/// Returns an error if the file's language is unsupported, the parser
/// fails to initialize, or tree-sitter returns no parse tree.
pub fn extract_file(path: &Path, source: &str) -> crate::error::Result<FileExtractions> {
    let lang = SupportedLanguage::from_path(path)
        .ok_or_else(|| AnchorError::UnsupportedLanguage(path.to_path_buf()))?;

    let mut parser = Parser::new();
    parser
        .set_language(&lang.tree_sitter_language())
        .map_err(|e| AnchorError::ParserInitError(path.to_path_buf(), e.to_string()))?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| AnchorError::TreeSitterParseFailed(path.to_path_buf()))?;
    let root = tree.root_node();

    let mut symbols = Vec::new();
    let mut imports = Vec::new();
    let mut calls = Vec::new();

    extract_node(
        &root,
        source.as_bytes(),
        lang,
        None,
        &mut symbols,
        &mut imports,
        &mut calls,
    );

    Ok(FileExtractions {
        file_path: path.to_path_buf(),
        symbols,
        imports,
        calls,
    })
}

/// Recursively extract information from a tree-sitter node.
fn extract_node(
    node: &Node,
    source: &[u8],
    lang: SupportedLanguage,
    current_scope: Option<&str>,
    symbols: &mut Vec<ExtractedSymbol>,
    imports: &mut Vec<ExtractedImport>,
    calls: &mut Vec<ExtractedCall>,
) {
    let kind = node.kind();

    match lang {
        SupportedLanguage::Rust => {
            rust::extract_rust_node(node, source, kind, current_scope, symbols, imports, calls);
        }
        SupportedLanguage::Python => {
            python::extract_python_node(node, source, kind, current_scope, symbols, imports, calls);
        }
        SupportedLanguage::JavaScript | SupportedLanguage::Tsx => {
            javascript::extract_js_node(node, source, kind, current_scope, symbols, imports, calls);
        }
        SupportedLanguage::TypeScript => {
            typescript::extract_ts_node(node, source, kind, current_scope, symbols, imports, calls);
        }
        SupportedLanguage::Go => {
            generic::extract_generic_node(
                node, source, kind, current_scope, symbols, imports, calls,
                &["function_declaration", "method_declaration"],
                &["import_declaration"],
                &["call_expression"],
            );
        }
        SupportedLanguage::Java => {
            generic::extract_generic_node(
                node, source, kind, current_scope, symbols, imports, calls,
                &["method_declaration", "class_declaration", "interface_declaration"],
                &["import_declaration"],
                &["method_invocation"],
            );
        }
        SupportedLanguage::CSharp => {
            generic::extract_generic_node(
                node, source, kind, current_scope, symbols, imports, calls,
                &["method_declaration", "class_declaration", "interface_declaration"],
                &["using_directive"],
                &["invocation_expression"],
            );
        }
        SupportedLanguage::Ruby => {
            generic::extract_generic_node(
                node, source, kind, current_scope, symbols, imports, calls,
                &["method", "class", "module"],
                &["call"],
                &["call", "method_call"],
            );
        }
        SupportedLanguage::Cpp | SupportedLanguage::Swift => {
            generic::extract_generic_node(
                node, source, kind, current_scope, symbols, imports, calls,
                &["function_definition", "class_specifier"],
                &["preproc_include"],
                &["call_expression"],
            );
        }
    }

    // Determine if this node creates a new scope for children
    let new_scope = match lang {
        SupportedLanguage::Rust => match kind {
            "impl_item" => get_rust_impl_name(node, source),
            "function_item" => node_name(node, source),
            "struct_item" | "enum_item" | "trait_item" => node_name(node, source),
            _ => None,
        },
        SupportedLanguage::Python => match kind {
            "class_definition" | "function_definition" => node_name(node, source),
            _ => None,
        },
        SupportedLanguage::JavaScript | SupportedLanguage::Tsx | SupportedLanguage::TypeScript => {
            match kind {
                "class_declaration" | "function_declaration" => node_name(node, source),
                _ => None,
            }
        }
        SupportedLanguage::Go => match kind {
            "function_declaration" | "method_declaration" => node_name(node, source),
            _ => None,
        },
        SupportedLanguage::Java | SupportedLanguage::CSharp => match kind {
            "method_declaration" | "class_declaration" => node_name(node, source),
            _ => None,
        },
        SupportedLanguage::Ruby => match kind {
            "method" | "class" | "module" => node_name(node, source),
            _ => None,
        },
        SupportedLanguage::Cpp | SupportedLanguage::Swift => match kind {
            "function_definition" | "class_specifier" => node_name(node, source),
            _ => None,
        },
    };

    let scope = new_scope.as_deref().or(current_scope);

    // Recurse into children
    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            extract_node(&child, source, lang, scope, symbols, imports, calls);
        }
    }
}
