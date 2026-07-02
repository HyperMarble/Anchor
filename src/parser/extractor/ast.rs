//
//  ast.rs
//  Anchor
//
//  Created by hak (tharun)
//

use tree_sitter::Node;

use super::helpers::{bounded_snippet, node_text};
use crate::parser::language::SupportedLanguage;
use crate::parser::types::*;

const IDENT_KINDS: &[&str] = &[
    "identifier",
    "field_identifier",
    "property_identifier",
    "type_identifier",
    "constant",
    "simple_identifier",
    "namespace_identifier",
    "nested_identifier",
    "custom_operator",
    "operator_name",
    "destructor_name",
    "operator",
];

const OPERATOR_TOKEN_KINDS: &[&str] = &[
    "!=", "!==", "%", "%=", "&", "*", "*=", "+", "++", "+=", "-", "--", "-=", "/", "/=", "<", "<<",
    "<=", "=", "==", "===", ">", ">=", ">>", "^", "|", "~",
];

#[derive(Default)]
struct ExtractState {
    symbols: Vec<ExtractedSymbol>,
    calls: Vec<ExtractedCall>,
}

include!("ast_parts/walk.rs");
include!("ast_parts/symbols.rs");
include!("ast_parts/calls.rs");
include!("ast_parts/names.rs");
include!("ast_parts/features.rs");
