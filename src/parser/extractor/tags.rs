//
//  tags.rs
//  Anchor
//
//  Created by hak (tharun)
//

use tree_sitter::Node;

use super::helpers::node_text;
use crate::graph::types::*;
use crate::parser::language::SupportedLanguage;

/// Extract import statements by walking the AST.
pub fn extract_imports(
    root: &Node,
    source: &[u8],
    lang: SupportedLanguage,
) -> Vec<ExtractedImport> {
    let import_kinds: &[&str] = match lang {
        SupportedLanguage::Rust => &["use_declaration"],
        SupportedLanguage::Python => &["import_statement", "import_from_statement"],
        SupportedLanguage::JavaScript | SupportedLanguage::Tsx | SupportedLanguage::TypeScript => {
            &["import_statement"]
        }
        SupportedLanguage::Go => &["import_declaration"],
        SupportedLanguage::Java => &["import_declaration"],
        SupportedLanguage::CSharp => &["using_directive"],
        SupportedLanguage::Ruby => &["call", "command"],
        SupportedLanguage::Cpp => &["preproc_include"],
        SupportedLanguage::Swift => &["import_declaration"],
    };

    let mut imports = Vec::new();
    collect_imports(root, source, lang, import_kinds, &mut imports);
    imports
}

fn collect_imports(
    node: &Node,
    source: &[u8],
    lang: SupportedLanguage,
    kinds: &[&str],
    imports: &mut Vec<ExtractedImport>,
) {
    if kinds.contains(&node.kind()) {
        let text = node_text(node, source);
        let path = clean_import_path(&text, lang);
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
            collect_imports(&child, source, lang, kinds, imports);
        }
    }
}

fn clean_import_path(text: &str, lang: SupportedLanguage) -> String {
    let stripped = text
        .trim()
        .trim_start_matches("use ")
        .trim_start_matches("import ")
        .trim_start_matches("from ")
        .trim_start_matches("using ")
        .trim_start_matches("#include ")
        .trim_end_matches(';')
        .trim();

    if matches!(lang, SupportedLanguage::Ruby)
        && !(stripped.starts_with("require ") || stripped.starts_with("require_relative "))
    {
        return String::new();
    }

    if let Some(idx) = stripped.find(" from ") {
        return stripped[idx + 6..]
            .trim()
            .trim_matches(|c| c == '\'' || c == '"')
            .to_string();
    }

    stripped
        .trim_start_matches("require ")
        .trim_start_matches("require_relative ")
        .trim_matches(|c| c == '\'' || c == '"' || c == '<' || c == '>')
        .to_string()
}
