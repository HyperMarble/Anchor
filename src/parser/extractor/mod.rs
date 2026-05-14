//
//  mod.rs
//  Anchor
//
//  Created by hak (tharun)
//

mod ast;
mod helpers;
mod tags;

use std::path::Path;

use tree_sitter::Parser;

use super::language::SupportedLanguage;
use crate::error::AnchorError;
use crate::graph::types::*;

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

    let file_str = path.to_string_lossy();
    let (symbols, calls) =
        ast::extract_symbols_and_calls(&tree.root_node(), source.as_bytes(), lang, &file_str);
    let imports = tags::extract_imports(&tree.root_node(), source.as_bytes(), lang);
    let api_endpoints = crate::parser::queries::api::extract_api_endpoints(
        &tree.root_node(),
        source.as_bytes(),
        lang,
        path,
    );

    Ok(FileExtractions {
        file_path: path.to_path_buf(),
        symbols,
        imports,
        calls,
        api_endpoints,
    })
}
