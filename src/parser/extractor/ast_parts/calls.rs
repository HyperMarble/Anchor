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

