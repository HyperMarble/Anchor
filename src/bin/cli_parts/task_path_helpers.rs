fn is_large_owner_symbol(symbol: &anchor::storage::SymbolEntry) -> bool {
    let lines = symbol.line_end.saturating_sub(symbol.line_start) + 1;
    if is_class_like_symbol(symbol) {
        return lines > 80;
    }
    matches!(symbol.kind.as_str(), "Function" | "Method") && lines > 120
}

fn task_path_prior(path: &str) -> i32 {
    let normalised = path.replace('\\', "/").to_ascii_lowercase();
    let file_name = Path::new(&normalised)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let mut score = 0;

    if file_name == "__init__.py" || file_name == "mod.rs" {
        score -= 35;
    }
    if normalised.contains("/examples/") || normalised.starts_with("examples/") {
        score -= 80;
    }
    if normalised.contains("/docs/") || normalised.starts_with("docs/") {
        score -= 60;
    }
    if normalised.contains("/legacy/") || normalised.contains("/compat/") {
        score -= 45;
    }

    score
}

fn task_slice_meaning(symbol: &anchor::storage::SymbolEntry) -> String {
    let role = infer_task_file_role(&symbol.path);
    format!(
        "{} {} in {} ({role})",
        symbol.kind, symbol.name, symbol.path
    )
}

fn task_responsibility_tags(
    symbol: &anchor::storage::SymbolEntry,
    reasons: &[String],
) -> Vec<String> {
    let mut tags: Vec<String> = reasons.iter().take(6).cloned().collect();
    tags.push(infer_task_file_role(&symbol.path));
    if is_class_like_symbol(symbol) {
        tags.push("owner".to_string());
    } else if matches!(symbol.kind.as_str(), "Function" | "Method") {
        tags.push("behavior".to_string());
    }
    for token in path_signal_tokens(&symbol.path).into_iter().take(3) {
        tags.push(token);
    }
    dedupe_strings(&mut tags);
    tags
}

fn infer_task_file_role(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    if looks_like_test_path(&lower) {
        "test".to_string()
    } else if lower.contains("/docs/") || lower.starts_with("docs/") || lower.ends_with(".md") {
        "docs".to_string()
    } else if lower.contains("route") || lower.contains("handler") {
        "handler".to_string()
    } else if lower.contains("schema") || lower.contains("model") || lower.contains("data") {
        "data_model".to_string()
    } else if lower.contains("auth") || lower.contains("session") {
        "auth".to_string()
    } else if lower.contains("render") || lower.contains("view") || lower.contains("ui") {
        "ui".to_string()
    } else {
        "source".to_string()
    }
}

fn task_search_tokens(text: &str) -> std::collections::BTreeSet<String> {
    let mut tokens = std::collections::BTreeSet::new();
    for token in tokenize_intent(text).filter(|token| token.len() >= 3) {
        tokens.insert(token.clone());
        for part in split_camel_token(&token) {
            if part.len() >= 3 {
                tokens.insert(part);
            }
        }
    }
    tokens
}

fn split_camel_token(token: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in token.chars() {
        if ch.is_ascii_uppercase() && !current.is_empty() {
            out.push(current.to_ascii_lowercase());
            current.clear();
        }
        current.push(ch);
    }
    if !current.is_empty() {
        out.push(current.to_ascii_lowercase());
    }
    out
}

fn soft_token_match(query: &str, candidate: &str) -> bool {
    if query == candidate {
        return true;
    }
    if query.len() < 4 || candidate.len() < 4 {
        return false;
    }
    if query.starts_with(candidate) || candidate.starts_with(query) {
        return true;
    }
    common_prefix_len(query, candidate) >= 4
}

fn common_prefix_len(a: &str, b: &str) -> usize {
    a.chars()
        .zip(b.chars())
        .take_while(|(left, right)| left == right)
        .count()
}

fn numbered_code(code: &str, start_line: usize) -> String {
    let mut output = String::new();
    for (offset, line) in code.lines().take(DEFAULT_CONTEXT_LINE_BUDGET).enumerate() {
        output.push_str(&format!(" {:>3}: {}\n", start_line + offset, line));
    }
    if code.lines().count() > DEFAULT_CONTEXT_LINE_BUDGET {
        output.push_str(&format!(
            "    ... [context truncated at {DEFAULT_CONTEXT_LINE_BUDGET} lines]\n"
        ));
    }
    output
}

