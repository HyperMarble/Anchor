fn content_backed_query_candidates(
    root: &Path,
    symbols: &[SymbolEntry],
    tokens: &std::collections::BTreeSet<String>,
    limit: usize,
) -> Vec<SymbolEntry> {
    let mut by_path: std::collections::BTreeMap<&str, Vec<&SymbolEntry>> =
        std::collections::BTreeMap::new();
    for symbol in symbols {
        by_path.entry(&symbol.path).or_default().push(symbol);
    }
    let signal_tokens = task_specific_tokens(tokens);
    let requires_signal = signal_tokens != *tokens;
    let allow_support_context = task_support_context_requested(tokens);
    let mut scored = Vec::new();
    for (path, path_symbols) in by_path {
        if looks_like_test_path(path) {
            continue;
        }
        if is_support_context_path(path) && !allow_support_context {
            continue;
        }
        let path_has_signal = task_path_has_signal(path, &signal_tokens);
        let symbol_has_signal = path_symbols
            .iter()
            .any(|symbol| task_symbol_has_name_or_feature_signal(symbol, &signal_tokens));
        if requires_signal && !path_has_signal && !symbol_has_signal {
            continue;
        }
        let full_path = root.join(path);
        if !anchor::parser::language::is_source_path(&full_path) {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&full_path) else {
            continue;
        };
        let source_score = task_source_rank(&source, tokens);
        if source_score <= 0 {
            continue;
        }
        for symbol in path_symbols {
            if !is_context_owner_symbol(symbol) || is_large_owner_symbol(symbol) {
                continue;
            }
            let owner_bonus = if is_class_like_symbol(symbol) { 80 } else { 20 };
            scored.push((
                source_score * 3 + owner_bonus + task_symbol_rank(symbol, tokens),
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
