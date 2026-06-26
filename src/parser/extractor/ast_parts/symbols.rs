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

