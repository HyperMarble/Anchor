fn task_symbol_rank(
    symbol: &anchor::storage::SymbolEntry,
    tokens: &std::collections::BTreeSet<String>,
) -> i32 {
    let name = symbol.name.to_ascii_lowercase();
    let path = symbol.path.to_ascii_lowercase();
    let kind = symbol.kind.as_str();
    let signal_tokens = task_symbol_signal_tokens(symbol);
    let mut score = 0i32;
    let mut matched_terms = 0usize;

    if matches!(
        kind,
        "Class" | "Struct" | "Enum" | "Interface" | "Trait" | "Function" | "Method"
    ) {
        score += 20;
    }
    if matches!(kind, "Class" | "Struct" | "Interface" | "Trait") {
        score += 20;
    }

    for token in tokens {
        let mut matched = false;
        if name == *token {
            score += 160;
            matched = true;
        } else if name.contains(token) {
            score += 80;
            matched = true;
        }
        if path.contains(token) {
            score += 45;
            matched = true;
        }
        if signal_tokens.iter().any(|feature| feature == token) {
            score += 20;
            matched = true;
        }
        if matched {
            matched_terms += 1;
        }
    }

    if matched_terms >= 2 {
        let coverage_terms = matched_terms.min(8);
        score += (coverage_terms * coverage_terms * 12) as i32;
    }

    if matches!(
        name.as_str(),
        "add" | "append" | "call" | "create" | "delete" | "get" | "insert" | "remove" | "set"
    ) {
        score -= 120;
    }
    if symbol.path.contains("/tests/") || symbol.path.starts_with("tests/") {
        score -= 140;
    }
    if name.starts_with("test") {
        score -= 120;
    }
    if is_constructor_symbol_name(&name)
        && !tokens.contains("init")
        && !tokens.contains("constructor")
        && !tokens.contains("initialize")
    {
        score -= 300;
    }

    score
}

fn is_constructor_symbol_name(name: &str) -> bool {
    matches!(
        name,
        "__init__" | "init" | "new" | "constructor" | "initialize"
    )
}

fn source_backed_task_candidates(
    root: &Path,
    symbols: &[anchor::storage::SymbolEntry],
    tokens: &std::collections::BTreeSet<String>,
    limit: usize,
) -> Vec<anchor::storage::SymbolEntry> {
    use std::collections::BTreeMap;

    let mut by_path: BTreeMap<&str, Vec<&anchor::storage::SymbolEntry>> = BTreeMap::new();
    for symbol in symbols {
        by_path.entry(&symbol.path).or_default().push(symbol);
    }

    let signal_tokens = task_specific_tokens(tokens);
    let requires_signal = signal_tokens != *tokens;
    let allow_support_context = task_support_context_requested(tokens);
    let mut scored = Vec::new();
    for (path, path_symbols) in by_path {
        let path_has_signal = task_path_has_signal(path, &signal_tokens);
        let symbol_has_signal = path_symbols
            .iter()
            .any(|symbol| task_symbol_has_name_or_feature_signal(symbol, &signal_tokens));
        if requires_signal && !path_has_signal && !symbol_has_signal {
            continue;
        }
        if is_support_context_path(path) && !allow_support_context {
            continue;
        }

        let full_path = root.join(path);
        if !anchor::parser::language::is_source_path(&full_path) {
            continue;
        }
        if path.contains("/tests/") || path.starts_with("tests/") {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&full_path) else {
            continue;
        };
        let file_score = task_source_rank(&source, tokens);
        if file_score < 80 {
            continue;
        }

        for symbol in path_symbols {
            if !matches!(
                symbol.kind.as_str(),
                "Class" | "Struct" | "Enum" | "Interface" | "Trait" | "Function" | "Method"
            ) {
                continue;
            }
            let owner_bonus = if matches!(
                symbol.kind.as_str(),
                "Class" | "Struct" | "Enum" | "Interface" | "Trait"
            ) {
                40
            } else {
                0
            };
            scored.push((
                file_score.saturating_mul(2) + owner_bonus + task_symbol_rank(symbol, tokens),
                (*symbol).clone(),
            ));
        }
    }

    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.path.cmp(&b.1.path))
            .then_with(|| a.1.line_start.cmp(&b.1.line_start))
            .then_with(|| a.1.name.cmp(&b.1.name))
    });
    scored.truncate(limit);
    scored.into_iter().map(|(_, symbol)| symbol).collect()
}

fn task_path_has_signal(path: &str, tokens: &std::collections::BTreeSet<String>) -> bool {
    let lower_path = path.to_ascii_lowercase();
    let path_tokens = path_signal_tokens(path);
    tokens.iter().any(|token| {
        lower_path.contains(token)
            || path_tokens
                .iter()
                .any(|path_token| soft_token_match(token, path_token))
    })
}

fn task_symbol_has_name_or_feature_signal(
    symbol: &anchor::storage::SymbolEntry,
    tokens: &std::collections::BTreeSet<String>,
) -> bool {
    let lower_name = symbol.name.to_ascii_lowercase();
    let signal_tokens = task_symbol_signal_tokens(symbol);
    tokens.iter().any(|token| {
        lower_name.contains(token) || signal_tokens.iter().any(|feature| feature == token)
    })
}

fn task_symbol_signal_tokens(
    symbol: &anchor::storage::SymbolEntry,
) -> std::collections::BTreeSet<String> {
    task_search_tokens(&format!("{} {} {}", symbol.name, symbol.kind, symbol.path))
}

fn task_specific_tokens(
    tokens: &std::collections::BTreeSet<String>,
) -> std::collections::BTreeSet<String> {
    let filtered: std::collections::BTreeSet<String> = tokens
        .iter()
        .filter(|token| !is_generic_task_token(token))
        .cloned()
        .collect();
    if filtered.len() < 2 {
        tokens.clone()
    } else {
        filtered
    }
}

fn is_generic_task_token(token: &str) -> bool {
    matches!(
        token,
        "body"
            | "code"
            | "config"
            | "data"
            | "file"
            | "files"
            | "header"
            | "headers"
            | "model"
            | "request"
            | "response"
            | "responses"
            | "result"
            | "results"
            | "test"
            | "tests"
            | "true"
            | "value"
            | "values"
    )
}

fn dedupe_symbols(symbols: &mut Vec<anchor::storage::SymbolEntry>) {
    use std::collections::BTreeSet;

    let mut seen = BTreeSet::new();
    symbols.retain(|symbol| seen.insert(task_symbol_key(symbol)));
}

fn task_symbol_total_rank(
    symbol: &anchor::storage::SymbolEntry,
    tokens: &std::collections::BTreeSet<String>,
    source_scores: &std::collections::BTreeMap<String, i32>,
) -> i32 {
    let source_score = source_scores
        .get(&task_symbol_key(symbol))
        .copied()
        .unwrap_or_default();
    let owner_bonus = if matches!(
        symbol.kind.as_str(),
        "Class" | "Struct" | "Enum" | "Interface" | "Trait"
    ) && source_score >= 80
    {
        320
    } else {
        0
    };

    task_symbol_rank(symbol, tokens) + source_score.saturating_mul(2) + owner_bonus
}

fn task_symbol_key(symbol: &anchor::storage::SymbolEntry) -> String {
    format!("{}:{}:{}", symbol.path, symbol.line_start, symbol.name)
}

fn task_source_rank(source: &str, tokens: &std::collections::BTreeSet<String>) -> i32 {
    let source = source.to_ascii_lowercase();
    let mut score = 0;
    let mut matched_terms = 0usize;

    for token in tokens {
        if source.contains(token) {
            matched_terms += 1;
            score += 28;
        }
    }

    if matched_terms >= 2 {
        let coverage_terms = matched_terms.min(10);
        score += (coverage_terms * coverage_terms * 10) as i32;
    }

    score
}
