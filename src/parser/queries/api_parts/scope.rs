/// Extract a function/class name from a node.
fn extract_scope_name(node: &Node, source: &[u8]) -> Option<String> {
    // JS: variable_declarator only sets scope if value is a function
    if node.kind() == "variable_declarator" {
        let is_fn = node
            .child_by_field_name("value")
            .map(|v| {
                matches!(
                    v.kind(),
                    "arrow_function" | "function_expression" | "function"
                )
            })
            .unwrap_or(false);
        if !is_fn {
            return None;
        }
    }

    // Try "name" field (works for most languages)
    if let Some(name) = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
    {
        return Some(name.to_string());
    }

    // C++ fallback: declarator → declarator (nested)
    if let Some(decl) = node.child_by_field_name("declarator") {
        let inner = decl.child_by_field_name("declarator").unwrap_or(decl);
        if let Ok(text) = inner.utf8_text(source) {
            return Some(text.split('(').next().unwrap_or(text).to_string());
        }
    }

    None
}

/// Walk up parents and check siblings to find enclosing function scope.
fn resolve_scope(node: &Node, source: &[u8], fn_nodes: &[&str]) -> Option<String> {
    // Strategy 1: Walk up to find enclosing function (Java annotations, C# attributes, Rust attributes)
    let mut parent = node.parent();
    while let Some(p) = parent {
        if fn_nodes.contains(&p.kind()) {
            if let Some(name) = extract_scope_name(&p, source) {
                return Some(name);
            }
        }
        parent = p.parent();
    }

    // Strategy 2: Check siblings (Python: decorator → sibling function_definition)
    if let Some(p) = node.parent() {
        for i in 0..p.child_count() {
            if let Some(sibling) = p.child(i) {
                if fn_nodes.contains(&sibling.kind()) {
                    if let Some(name) = extract_scope_name(&sibling, source) {
                        return Some(name);
                    }
                }
            }
        }
    }

    None
}

/// Apply class-level base path to a URL.
fn apply_base_path(url: &str, base_path: &str) -> String {
    if base_path.is_empty() {
        return url.to_string();
    }
    if url.is_empty() {
        return base_path.to_string();
    }
    if url.starts_with(base_path) {
        return url.to_string();
    }
    let base = base_path.trim_end_matches('/');
    let suffix = if url.starts_with('/') {
        url.to_string()
    } else {
        format!("/{}", url)
    };
    format!("{}{}", base, suffix)
}

/// Extract the first quoted string from text.
fn extract_first_string(text: &str) -> Option<String> {
    // Double quotes
    if let Some(start) = text.find('"') {
        if let Some(end) = text[start + 1..].find('"') {
            return Some(text[start + 1..start + 1 + end].to_string());
        }
    }
    // Single quotes (only if it looks like a URL path)
    if let Some(start) = text.find('\'') {
        if let Some(end) = text[start + 1..].find('\'') {
            let s = &text[start + 1..start + 1 + end];
            if s.starts_with('/') || s.contains("api") || s.starts_with("http") {
                return Some(s.to_string());
            }
        }
    }
    // Backticks (JS template literals)
    if let Some(start) = text.find('`') {
        if let Some(end) = text[start + 1..].find('`') {
            return Some(text[start + 1..start + 1 + end].to_string());
        }
    }
    None
}

/// Auto-detect HTTP method from node text when pattern doesn't specify one.
fn detect_method_from_text(text: &str) -> Option<&'static str> {
    // Check for specific method indicators in the text
    if text.contains(".get(")
        || text.contains(".Get(")
        || text.contains(".GET(")
        || text.contains("\"GET\"")
    {
        return Some("GET");
    }
    if text.contains(".post(")
        || text.contains(".Post(")
        || text.contains(".POST(")
        || text.contains("\"POST\"")
    {
        return Some("POST");
    }
    if text.contains(".put(")
        || text.contains(".Put(")
        || text.contains(".PUT(")
        || text.contains("\"PUT\"")
    {
        return Some("PUT");
    }
    if text.contains(".delete(")
        || text.contains(".Delete(")
        || text.contains(".DELETE(")
        || text.contains("\"DELETE\"")
    {
        return Some("DELETE");
    }
    if text.contains(".patch(")
        || text.contains(".Patch(")
        || text.contains(".PATCH(")
        || text.contains("\"PATCH\"")
    {
        return Some("PATCH");
    }
    Some("GET") // Default
}
