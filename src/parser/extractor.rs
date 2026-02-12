//! Symbol extraction from source code using tree-sitter ASTs.
//!
//! Walks the AST of a source file and extracts:
//! - Symbol definitions (functions, structs, classes, etc.)
//! - Import statements
//! - Function calls (for building call graphs)

use std::path::Path;
use tree_sitter::{Node, Parser};

use super::language::SupportedLanguage;
use crate::error::AnchorError;
use crate::graph::types::*;

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
            extract_rust_node(node, source, kind, current_scope, symbols, imports, calls);
        }
        SupportedLanguage::Python => {
            extract_python_node(node, source, kind, current_scope, symbols, imports, calls);
        }
        SupportedLanguage::JavaScript | SupportedLanguage::Tsx => {
            extract_js_node(node, source, kind, current_scope, symbols, imports, calls);
        }
        SupportedLanguage::TypeScript => {
            extract_ts_node(node, source, kind, current_scope, symbols, imports, calls);
        }
        SupportedLanguage::Go => {
            extract_generic_node(
                node,
                source,
                kind,
                current_scope,
                symbols,
                imports,
                calls,
                &["function_declaration", "method_declaration"],
                &["import_declaration"],
                &["call_expression"],
            );
        }
        SupportedLanguage::Java => {
            extract_generic_node(
                node,
                source,
                kind,
                current_scope,
                symbols,
                imports,
                calls,
                &[
                    "method_declaration",
                    "class_declaration",
                    "interface_declaration",
                ],
                &["import_declaration"],
                &["method_invocation"],
            );
        }
        SupportedLanguage::CSharp => {
            extract_generic_node(
                node,
                source,
                kind,
                current_scope,
                symbols,
                imports,
                calls,
                &[
                    "method_declaration",
                    "class_declaration",
                    "interface_declaration",
                ],
                &["using_directive"],
                &["invocation_expression"],
            );
        }
        SupportedLanguage::Ruby => {
            extract_generic_node(
                node,
                source,
                kind,
                current_scope,
                symbols,
                imports,
                calls,
                &["method", "class", "module"],
                &["call"],
                &["call", "method_call"],
            );
        }
        SupportedLanguage::Cpp | SupportedLanguage::Swift => {
            extract_generic_node(
                node,
                source,
                kind,
                current_scope,
                symbols,
                imports,
                calls,
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

// ─── Rust Extraction ────────────────────────────────────────────

fn extract_rust_node(
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
            // Parse "use foo::bar::Baz;" into path
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

// ─── Python Extraction ──────────────────────────────────────────

fn extract_python_node(
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
            // "from foo import bar, baz"
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

// ─── JavaScript Extraction ──────────────────────────────────────

fn extract_js_node(
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
            // Handle: const foo = () => {} or const FOO = "bar"
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

// ─── TypeScript Extraction ──────────────────────────────────────

fn extract_ts_node(
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

// ─── Generic Extraction (for new languages) ─────────────────────

/// Generic node extraction for languages without dedicated extractors.
#[allow(clippy::too_many_arguments)]
fn extract_generic_node(
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
    // Extract functions/methods
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

    // Extract imports
    if import_kinds.contains(&kind) {
        let text = node_text(node, source);
        imports.push(ExtractedImport {
            path: text.trim().to_string(),
            symbols: Vec::new(),
            line: node.start_position().row + 1,
        });
    }

    // Extract calls
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

// ─── Helper Functions ───────────────────────────────────────────

/// Get the name of a node from its "name" field.
fn node_name(node: &Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.to_string())
}

/// Get the full text of a node.
fn node_text(node: &Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").to_string()
}

/// Extract full code snippet from a node (no truncation).
/// Slicing happens at display time using graph knowledge.
fn bounded_snippet(node: &Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").to_string()
}

/// Get the type name from a Rust impl block.
/// Handles `impl Foo` and `impl Trait for Foo`.
fn get_rust_impl_name(node: &Node, source: &[u8]) -> Option<String> {
    // Try "type" field first (impl Type)
    if let Some(type_node) = node.child_by_field_name("type") {
        return type_node.utf8_text(source).ok().map(|s| s.to_string());
    }
    // Try "body" sibling for the type name
    let text = node_text(node, source);
    // Parse "impl Foo {" or "impl Trait for Foo {"
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() >= 2 {
        if parts.contains(&"for") {
            // impl Trait for Type
            parts
                .iter()
                .position(|&p| p == "for")
                .and_then(|i| parts.get(i + 1))
                .map(|s| s.trim_end_matches('{').trim().to_string())
        } else {
            // impl Type
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

/// Get the function name from a call_expression node (Rust/JS/TS).
fn get_call_name(node: &Node, source: &[u8]) -> Option<String> {
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

/// Get the function name from a Python call node.
fn get_python_call_name(node: &Node, source: &[u8]) -> Option<String> {
    let func_node = node.child_by_field_name("function")?;
    let text = func_node.utf8_text(source).ok()?;

    // Handle: obj.method() -> "method"
    // Handle: func() -> "func"
    let name = text.rsplit('.').next().unwrap_or(text).trim();

    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Extract variable declarations that define functions or constants (JS/TS).
fn extract_js_variable_declaration(
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
                            // Check if it's a constant (ALL_CAPS name)
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
fn extract_js_import(node: &Node, source: &[u8], imports: &mut Vec<ExtractedImport>) {
    let text = node_text(node, source);

    // Extract the module path from: import { x } from 'path'
    // or: import x from 'path'
    let path = text
        .rsplit("from")
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches(|c| c == '\'' || c == '"' || c == ';' || c == ' ')
        .to_string();

    // Extract imported symbols
    let syms: Vec<String> = if text.contains('{') {
        text.split('{')
            .nth(1)
            .unwrap_or("")
            .split('}')
            .next()
            .unwrap_or("")
            .split(',')
            .map(|s| {
                // Handle "x as y" -> take "x"
                s.split(" as ").next().unwrap_or("").trim().to_string()
            })
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
