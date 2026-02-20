//
//  mod.rs
//  Anchor
//
//  Created by hak (tharun)
//

mod helpers;
mod tags;

use std::path::Path;

use tree_sitter::Parser;

use super::language::SupportedLanguage;
use crate::error::AnchorError;
use crate::graph::types::*;

/// Build the full tags query for a language.
/// Combines the grammar's TAGS_QUERY with supplementary patterns
/// we need for complete call graph extraction.
fn build_query(lang: SupportedLanguage) -> String {
    let base = base_tags_query(lang);
    let extra = supplementary_patterns(lang);
    if extra.is_empty() {
        base
    } else {
        format!("{base}\n{extra}")
    }
}

/// Get the base tags query from the grammar crate.
fn base_tags_query(lang: SupportedLanguage) -> String {
    match lang {
        SupportedLanguage::Rust => tree_sitter_rust::TAGS_QUERY.into(),
        SupportedLanguage::Python => tree_sitter_python::TAGS_QUERY.into(),
        SupportedLanguage::JavaScript => tree_sitter_javascript::TAGS_QUERY.into(),
        SupportedLanguage::TypeScript | SupportedLanguage::Tsx => {
            // TS/TSX grammars extend JS — combine both queries.
            format!(
                "{}\n{}",
                tree_sitter_javascript::TAGS_QUERY,
                tree_sitter_typescript::TAGS_QUERY
            )
        }
        SupportedLanguage::Go => tree_sitter_go::TAGS_QUERY.into(),
        SupportedLanguage::Java => tree_sitter_java::TAGS_QUERY.into(),
        SupportedLanguage::CSharp => CSHARP_TAGS_QUERY.into(),
        SupportedLanguage::Ruby => tree_sitter_ruby::TAGS_QUERY.into(),
        SupportedLanguage::Cpp => tree_sitter_cpp::TAGS_QUERY.into(),
        SupportedLanguage::Swift => tree_sitter_swift::TAGS_QUERY.into(),
    }
}

/// Extra patterns the base tags query misses.
fn supplementary_patterns(lang: SupportedLanguage) -> &'static str {
    match lang {
        // Scoped calls: Config::new(), HashMap::from()
        SupportedLanguage::Rust => {
            "(call_expression function: (scoped_identifier name: (identifier) @name)) @reference.call"
        }
        // TS tags query lacks type aliases and enums
        SupportedLanguage::TypeScript | SupportedLanguage::Tsx => concat!(
            "(type_alias_declaration name: (type_identifier) @name) @definition.type\n",
            "(enum_declaration name: (identifier) @name) @definition.type",
        ),
        // No call patterns in base C++ tags query
        SupportedLanguage::Cpp => concat!(
            "(call_expression function: (identifier) @name) @reference.call\n",
            "(call_expression function: (field_expression field: (field_identifier) @name)) @reference.call",
        ),
        // Direct calls: DoSomething() (base only has member access calls)
        SupportedLanguage::CSharp => {
            "(invocation_expression function: (identifier) @name) @reference.call"
        }
        // No call patterns in base Swift tags query
        SupportedLanguage::Swift => {
            "(call_expression function: (simple_identifier) @name) @reference.call"
        }
        _ => "",
    }
}

/// C# tags query — the crate comments out TAGS_QUERY, so we embed it.
const CSHARP_TAGS_QUERY: &str = r#"
(class_declaration name: (identifier) @name) @definition.class
(interface_declaration name: (identifier) @name) @definition.interface
(method_declaration name: (identifier) @name) @definition.method
(namespace_declaration name: (identifier) @name) @definition.module
(invocation_expression function: (member_access_expression name: (identifier) @name)) @reference.call
(object_creation_expression type: (identifier) @name) @reference.class
"#;

/// Extract all symbols, imports, and calls from a source file.
pub fn extract_file(path: &Path, source: &str) -> crate::error::Result<FileExtractions> {
    let lang = SupportedLanguage::from_path(path)
        .ok_or_else(|| AnchorError::UnsupportedLanguage(path.to_path_buf()))?;

    let mut parser = Parser::new();
    let ts_lang = lang.tree_sitter_language();
    parser
        .set_language(&ts_lang)
        .map_err(|e| AnchorError::ParserInitError(path.to_path_buf(), e.to_string()))?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| AnchorError::TreeSitterParseFailed(path.to_path_buf()))?;

    let query_src = build_query(lang);
    let (symbols, calls) = tags::extract_with_tags(&tree, source.as_bytes(), &query_src, &ts_lang);
    let imports = tags::extract_imports(&tree.root_node(), source.as_bytes(), lang);

    Ok(FileExtractions {
        file_path: path.to_path_buf(),
        symbols,
        imports,
        calls,
    })
}
