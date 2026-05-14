//
//  ast.rs
//  Anchor
//
//  Created by hak (tharun)
//

use tree_sitter::Node;

use super::helpers::{bounded_snippet, node_text};
use crate::graph::types::*;
use crate::parser::language::SupportedLanguage;

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

pub fn extract_symbols_and_calls(
    root: &Node,
    source: &[u8],
    lang: SupportedLanguage,
    file_path: &str,
) -> (Vec<ExtractedSymbol>, Vec<ExtractedCall>) {
    let mut state = ExtractState::default();
    let mut containers = Vec::new();
    let mut scopes = Vec::new();

    walk(
        root,
        source,
        lang,
        file_path,
        &mut containers,
        &mut scopes,
        &mut state,
    );

    (state.symbols, state.calls)
}

fn walk(
    node: &Node,
    source: &[u8],
    lang: SupportedLanguage,
    file_path: &str,
    containers: &mut Vec<String>,
    scopes: &mut Vec<String>,
    state: &mut ExtractState,
) {
    let container_name = container_name(node, source, lang);
    if let Some(name) = &container_name {
        containers.push(name.clone());
    }

    let symbol = symbol_from_node(node, source, lang);
    let mut pushed_scope = false;
    if let Some((name, kind)) = symbol {
        let parent = parent_for_symbol(&name, kind, containers);
        let features = generate_features(&name, kind, parent.as_deref(), file_path);

        state.symbols.push(ExtractedSymbol {
            name: name.clone(),
            kind,
            line_start: node.start_position().row + 1,
            line_end: node.end_position().row + 1,
            code_snippet: bounded_snippet(node, source),
            parent,
            features,
        });

        if is_scope(kind) {
            scopes.push(name);
            pushed_scope = true;
        }
    } else if let Some(name) = scope_name(node, source, lang) {
        scopes.push(name);
        pushed_scope = true;
    }

    if let Some(callee) = call_from_node(node, source, lang) {
        if let Some(caller) = scopes.last() {
            state.calls.push(ExtractedCall {
                callee,
                caller: caller.clone(),
                line: node.start_position().row + 1,
                line_end: node.end_position().row + 1,
            });
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk(&child, source, lang, file_path, containers, scopes, state);
        }
    }

    if pushed_scope {
        scopes.pop();
    }
    if container_name.is_some() {
        containers.pop();
    }
}

fn symbol_from_node(
    node: &Node,
    source: &[u8],
    lang: SupportedLanguage,
) -> Option<(String, NodeKind)> {
    let kind = node.kind();
    match lang {
        SupportedLanguage::Rust => match kind {
            "function_item" => name_field(node, source).map(|n| (n, NodeKind::Function)),
            "function_signature_item" => name_field(node, source).map(|n| (n, NodeKind::Method)),
            "struct_item" => name_field(node, source).map(|n| (n, NodeKind::Struct)),
            "enum_item" => name_field(node, source).map(|n| (n, NodeKind::Enum)),
            "trait_item" => name_field(node, source).map(|n| (n, NodeKind::Trait)),
            "mod_item" => name_field(node, source).map(|n| (n, NodeKind::Module)),
            "type_item" => name_field(node, source).map(|n| (n, NodeKind::Type)),
            "associated_type" => name_field(node, source).map(|n| (n, NodeKind::Type)),
            "macro_definition" => name_field(node, source).map(|n| (n, NodeKind::Function)),
            "const_item" | "static_item" => {
                name_field(node, source).map(|n| (n, NodeKind::Constant))
            }
            _ => None,
        },
        SupportedLanguage::Python => match kind {
            "class_definition" => name_field(node, source).map(|n| (n, NodeKind::Class)),
            "function_definition" => name_field(node, source).map(|n| (n, NodeKind::Function)),
            _ => None,
        },
        SupportedLanguage::JavaScript | SupportedLanguage::TypeScript | SupportedLanguage::Tsx => {
            js_symbol_from_node(node, source, lang)
        }
        SupportedLanguage::Go => match kind {
            "function_declaration" => name_field(node, source).map(|n| (n, NodeKind::Function)),
            "method_declaration" => name_field(node, source).map(|n| (n, NodeKind::Method)),
            "method_elem" => name_field(node, source).map(|n| (n, NodeKind::Method)),
            "type_spec" => go_type_spec(node, source),
            _ => None,
        },
        SupportedLanguage::Java => match kind {
            "class_declaration" => name_field(node, source).map(|n| (n, NodeKind::Class)),
            "record_declaration" => name_field(node, source).map(|n| (n, NodeKind::Class)),
            "annotation_type_declaration" => {
                name_field(node, source).map(|n| (n, NodeKind::Interface))
            }
            "interface_declaration" => name_field(node, source).map(|n| (n, NodeKind::Interface)),
            "enum_declaration" => name_field(node, source).map(|n| (n, NodeKind::Enum)),
            "method_declaration" => name_field(node, source).map(|n| (n, NodeKind::Method)),
            "annotation_type_element_declaration" => {
                name_field(node, source).map(|n| (n, NodeKind::Method))
            }
            "constructor_declaration" => name_field(node, source).map(|n| (n, NodeKind::Method)),
            "compact_constructor_declaration" => {
                Some(("constructor".to_string(), NodeKind::Method))
            }
            _ => None,
        },
        SupportedLanguage::CSharp => match kind {
            "class_declaration" => name_field(node, source).map(|n| (n, NodeKind::Class)),
            "record_declaration" => name_field(node, source).map(|n| (n, NodeKind::Class)),
            "interface_declaration" => name_field(node, source).map(|n| (n, NodeKind::Interface)),
            "enum_declaration" => name_field(node, source).map(|n| (n, NodeKind::Enum)),
            "struct_declaration" => name_field(node, source).map(|n| (n, NodeKind::Struct)),
            "delegate_declaration" => name_field(node, source).map(|n| (n, NodeKind::Function)),
            "property_declaration" => name_field(node, source).map(|n| (n, NodeKind::Variable)),
            "method_declaration" => name_field(node, source).map(|n| (n, NodeKind::Method)),
            "local_function_statement" => name_field(node, source).map(|n| (n, NodeKind::Function)),
            "constructor_declaration" => name_field(node, source).map(|n| (n, NodeKind::Method)),
            "destructor_declaration" => Some(("destructor".to_string(), NodeKind::Method)),
            "namespace_declaration" => name_field(node, source).map(|n| (n, NodeKind::Module)),
            _ => None,
        },
        SupportedLanguage::Ruby => match kind {
            "class" | "module" => first_identifier(node, source).map(|n| {
                let node_kind = if kind == "module" {
                    NodeKind::Module
                } else {
                    NodeKind::Class
                };
                (n, node_kind)
            }),
            "method" | "singleton_method" => {
                first_identifier(node, source).map(|n| (n, NodeKind::Method))
            }
            _ => None,
        },
        SupportedLanguage::Cpp => match kind {
            "class_specifier" => name_field(node, source)
                .or_else(|| first_identifier(node, source))
                .map(|n| (n, NodeKind::Class)),
            "struct_specifier" => name_field(node, source)
                .or_else(|| first_identifier(node, source))
                .map(|n| (n, NodeKind::Struct)),
            "enum_specifier" => name_field(node, source)
                .or_else(|| first_identifier(node, source))
                .map(|n| (n, NodeKind::Enum)),
            "function_definition" | "declaration" | "field_declaration" => {
                cpp_function_name(node, source).map(|n| (n, NodeKind::Function))
            }
            "namespace_definition" => name_field(node, source).map(|n| (n, NodeKind::Module)),
            _ => None,
        },
        SupportedLanguage::Swift => match kind {
            "class_declaration" => name_field(node, source).map(|n| (n, NodeKind::Class)),
            "struct_declaration" => name_field(node, source).map(|n| (n, NodeKind::Struct)),
            "enum_declaration" => name_field(node, source).map(|n| (n, NodeKind::Enum)),
            "protocol_declaration" => name_field(node, source).map(|n| (n, NodeKind::Interface)),
            "function_declaration" => name_field(node, source).map(|n| (n, NodeKind::Function)),
            "init_declaration" => Some(("init".to_string(), NodeKind::Method)),
            "deinit_declaration" => Some(("deinit".to_string(), NodeKind::Method)),
            "subscript_declaration" => Some(("subscript".to_string(), NodeKind::Method)),
            "operator_declaration" => {
                first_identifier(node, source).map(|n| (n, NodeKind::Function))
            }
            _ => None,
        },
    }
}

fn js_symbol_from_node(
    node: &Node,
    source: &[u8],
    lang: SupportedLanguage,
) -> Option<(String, NodeKind)> {
    match node.kind() {
        "abstract_class_declaration" => name_field(node, source).map(|n| (n, NodeKind::Class)),
        "class_declaration" => name_field(node, source).map(|n| (n, NodeKind::Class)),
        "function_declaration" => name_field(node, source).map(|n| (n, NodeKind::Function)),
        "generator_function_declaration" => {
            name_field(node, source).map(|n| (n, NodeKind::Function))
        }
        "function" | "function_expression" => {
            name_field(node, source).map(|n| (n, NodeKind::Function))
        }
        "method_definition" => name_field(node, source).map(|n| (n, NodeKind::Method)),
        "method_signature" | "abstract_method_signature" => {
            name_field(node, source).map(|n| (n, NodeKind::Method))
        }
        "function_signature" => name_field(node, source).map(|n| (n, NodeKind::Function)),
        "interface_declaration" if lang != SupportedLanguage::JavaScript => {
            name_field(node, source).map(|n| (n, NodeKind::Interface))
        }
        "type_alias_declaration" if lang != SupportedLanguage::JavaScript => {
            name_field(node, source).map(|n| (n, NodeKind::Type))
        }
        "enum_declaration" if lang != SupportedLanguage::JavaScript => {
            name_field(node, source).map(|n| (n, NodeKind::Enum))
        }
        "internal_module" | "module" if lang != SupportedLanguage::JavaScript => {
            name_field(node, source).map(|n| (n, NodeKind::Module))
        }
        "field_definition" | "public_field_definition" => {
            let value = node.child_by_field_name("value")?;
            if !is_js_function_value(value.kind()) {
                return None;
            }
            name_field(node, source)
                .or_else(|| first_identifier(node, source))
                .map(|n| (n, NodeKind::Method))
        }
        "pair" => {
            let value = node.child_by_field_name("value")?;
            if !is_js_function_value(value.kind()) {
                return None;
            }
            name_field(&value, source)
                .or_else(|| {
                    node.child_by_field_name("key")
                        .and_then(|key| terminal_name(&key, source))
                })
                .map(|n| (n, NodeKind::Function))
        }
        "property_signature" if lang != SupportedLanguage::JavaScript => {
            name_field(node, source).map(|n| (n, NodeKind::Variable))
        }
        "variable_declarator" => {
            let value = node.child_by_field_name("value")?;
            if !is_js_function_value(value.kind())
                && !matches!(value.kind(), "class" | "class_expression")
            {
                return None;
            }
            name_field(node, source)
                .or_else(|| first_identifier(node, source))
                .map(|n| (n, NodeKind::Function))
        }
        _ => None,
    }
}

fn is_js_function_value(kind: &str) -> bool {
    matches!(kind, "arrow_function" | "function" | "function_expression")
}

fn go_type_spec(node: &Node, source: &[u8]) -> Option<(String, NodeKind)> {
    let name = name_field(node, source)?;
    let value = node.child_by_field_name("type")?;
    let kind = match value.kind() {
        "struct_type" => NodeKind::Struct,
        "interface_type" => NodeKind::Interface,
        _ => NodeKind::Type,
    };
    Some((name, kind))
}

fn cpp_function_name(node: &Node, source: &[u8]) -> Option<String> {
    let declarator = node.child_by_field_name("declarator")?;
    match declarator.kind() {
        "function_declarator" => declarator_name(&declarator, source),
        _ => {
            let nested = find_child_kind(declarator, "function_declarator")?;
            declarator_name(&nested, source)
        }
    }
}

fn declarator_name(node: &Node, source: &[u8]) -> Option<String> {
    if let Some(declarator) = node.child_by_field_name("declarator") {
        return last_identifier(&declarator, source);
    }
    last_identifier(node, source)
}

fn call_from_node(node: &Node, source: &[u8], lang: SupportedLanguage) -> Option<String> {
    let kind = node.kind();
    match lang {
        SupportedLanguage::Rust => match kind {
            "call_expression" => field_terminal_name(node, "function", source),
            "method_call_expression" => name_field(node, source),
            "macro_invocation" => field_terminal_name(node, "macro", source),
            _ => None,
        },
        SupportedLanguage::Python => {
            if kind == "call" {
                field_terminal_name(node, "function", source)
            } else {
                None
            }
        }
        SupportedLanguage::JavaScript | SupportedLanguage::TypeScript | SupportedLanguage::Tsx => {
            match kind {
                "call_expression" => field_terminal_name(node, "function", source),
                "new_expression" => field_terminal_name(node, "function", source)
                    .or_else(|| field_terminal_name(node, "constructor", source))
                    .or_else(|| field_terminal_name(node, "type", source))
                    .or_else(|| first_identifier(node, source)),
                _ => None,
            }
        }
        SupportedLanguage::Go => {
            if kind == "call_expression" {
                field_terminal_name(node, "function", source)
            } else {
                None
            }
        }
        SupportedLanguage::Java => match kind {
            "method_invocation" => name_field(node, source),
            "object_creation_expression" => field_terminal_name(node, "type", source),
            _ => None,
        },
        SupportedLanguage::CSharp => match kind {
            "invocation_expression" => field_terminal_name(node, "function", source),
            "object_creation_expression" => field_terminal_name(node, "type", source),
            _ => None,
        },
        SupportedLanguage::Ruby => match kind {
            "call" | "method_call" | "command" | "command_call" => {
                name_field(node, source).or_else(|| last_identifier(node, source))
            }
            _ => None,
        },
        SupportedLanguage::Cpp => {
            if kind == "call_expression" {
                field_terminal_name(node, "function", source)
            } else {
                None
            }
        }
        SupportedLanguage::Swift => {
            if kind == "call_expression" {
                field_terminal_name(node, "function", source)
                    .or_else(|| first_identifier(node, source))
            } else {
                None
            }
        }
    }
}

fn container_name(node: &Node, source: &[u8], lang: SupportedLanguage) -> Option<String> {
    if let Some((name, kind)) = symbol_from_node(node, source, lang) {
        if is_container(kind) {
            return Some(name);
        }
    }

    match (lang, node.kind()) {
        (SupportedLanguage::Rust, "impl_item") => field_terminal_name(node, "type", source),
        _ => None,
    }
}

fn scope_name(node: &Node, source: &[u8], lang: SupportedLanguage) -> Option<String> {
    match (lang, node.kind()) {
        (SupportedLanguage::Rust, "impl_item") => field_terminal_name(node, "type", source),
        _ => None,
    }
}

fn parent_for_symbol(name: &str, kind: NodeKind, containers: &[String]) -> Option<String> {
    if is_container(kind) {
        return None;
    }
    containers
        .iter()
        .rev()
        .find(|parent| parent.as_str() != name)
        .cloned()
}

fn is_scope(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Function
            | NodeKind::Method
            | NodeKind::Class
            | NodeKind::Struct
            | NodeKind::Interface
            | NodeKind::Trait
            | NodeKind::Module
    )
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

fn name_field(node: &Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|child| terminal_name(&child, source))
}

fn field_terminal_name(node: &Node, field: &str, source: &[u8]) -> Option<String> {
    node.child_by_field_name(field)
        .and_then(|child| terminal_name(&child, source))
}

fn terminal_name(node: &Node, source: &[u8]) -> Option<String> {
    if IDENT_KINDS.contains(&node.kind()) {
        let text = clean_name(&node_text(node, source));
        return (!text.is_empty()).then_some(text);
    }
    if OPERATOR_TOKEN_KINDS.contains(&node.kind()) {
        return Some(node.kind().to_string());
    }

    for field in ["name", "field", "property", "member", "type", "declarator"] {
        if let Some(child) = node.child_by_field_name(field) {
            if let Some(name) = terminal_name(&child, source) {
                return Some(name);
            }
        }
    }

    last_identifier(node, source)
}

fn first_identifier(node: &Node, source: &[u8]) -> Option<String> {
    if IDENT_KINDS.contains(&node.kind()) {
        let text = clean_name(&node_text(node, source));
        return (!text.is_empty()).then_some(text);
    }
    if OPERATOR_TOKEN_KINDS.contains(&node.kind()) {
        return Some(node.kind().to_string());
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if let Some(name) = first_identifier(&child, source) {
                return Some(name);
            }
        }
    }

    None
}

fn last_identifier(node: &Node, source: &[u8]) -> Option<String> {
    let mut found = None;
    if IDENT_KINDS.contains(&node.kind()) {
        let text = clean_name(&node_text(node, source));
        if !text.is_empty() {
            found = Some(text);
        }
    }
    if OPERATOR_TOKEN_KINDS.contains(&node.kind()) {
        found = Some(node.kind().to_string());
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if let Some(name) = last_identifier(&child, source) {
                found = Some(name);
            }
        }
    }

    found
}

fn find_child_kind<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == kind {
                return Some(child);
            }
            if let Some(found) = find_child_kind(child, kind) {
                return Some(found);
            }
        }
    }
    None
}

fn clean_name(raw: &str) -> String {
    raw.trim()
        .trim_matches(|c: char| {
            matches!(
                c,
                '\'' | '"' | '`' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | ':' | ';' | ','
            )
        })
        .to_string()
}

fn split_identifier(name: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for part in name.split('_') {
        if part.is_empty() {
            continue;
        }
        let mut current = String::new();
        for ch in part.chars() {
            if ch.is_uppercase() && !current.is_empty() {
                tokens.push(current.to_lowercase());
                current = String::new();
            }
            current.push(ch);
        }
        if !current.is_empty() {
            tokens.push(current.to_lowercase());
        }
    }
    tokens.retain(|t| t.len() > 2);
    tokens
}

fn generate_features(
    name: &str,
    kind: NodeKind,
    parent: Option<&str>,
    file_path: &str,
) -> Vec<String> {
    let mut features = split_identifier(name);
    features.push(format!("{:?}", kind).to_lowercase());

    if let Some(parent) = parent {
        features.extend(split_identifier(parent));
    }

    for segment in file_path.split(&['/', '\\'][..]) {
        let stem = segment
            .strip_suffix(".rs")
            .or_else(|| segment.strip_suffix(".py"))
            .or_else(|| segment.strip_suffix(".ts"))
            .or_else(|| segment.strip_suffix(".tsx"))
            .or_else(|| segment.strip_suffix(".js"))
            .or_else(|| segment.strip_suffix(".jsx"))
            .or_else(|| segment.strip_suffix(".go"))
            .or_else(|| segment.strip_suffix(".java"))
            .or_else(|| segment.strip_suffix(".cs"))
            .or_else(|| segment.strip_suffix(".cpp"))
            .or_else(|| segment.strip_suffix(".hpp"))
            .or_else(|| segment.strip_suffix(".h"))
            .or_else(|| segment.strip_suffix(".swift"))
            .or_else(|| segment.strip_suffix(".rb"))
            .unwrap_or(segment);
        if stem.len() > 2 && stem != "src" && stem != "lib" && stem != "mod" {
            features.extend(split_identifier(stem));
        }
    }

    features.sort();
    features.dedup();
    features
}
