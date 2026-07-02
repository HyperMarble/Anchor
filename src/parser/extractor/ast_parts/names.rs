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

