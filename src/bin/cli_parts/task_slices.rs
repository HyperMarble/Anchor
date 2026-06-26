fn rank_task_slices(
    store: &AnchorStore,
    symbols: &[anchor::storage::SymbolEntry],
    call_index: &anchor::storage::CallIndex,
    tokens: &std::collections::BTreeSet<String>,
    candidates: &[anchor::storage::SymbolEntry],
    limit: usize,
) -> Vec<TaskSlice> {
    use std::collections::BTreeSet;

    let candidate_keys: BTreeSet<String> = candidates.iter().map(task_symbol_key).collect();
    let candidate_names: BTreeSet<String> = candidates
        .iter()
        .map(|symbol| symbol.name.to_ascii_lowercase())
        .collect();
    let candidate_paths: BTreeSet<&str> = candidates
        .iter()
        .map(|symbol| symbol.path.as_str())
        .collect();

    let mut scored = Vec::new();
    for symbol in symbols {
        if looks_like_test_path(&symbol.path) || !is_context_owner_symbol(symbol) {
            continue;
        }
        if is_large_owner_symbol(symbol) {
            continue;
        }
        let is_ranked_candidate = candidate_keys.contains(&task_symbol_key(symbol));
        let callers = call_index.callers_of(&symbol.name);
        let callees = call_index.callees_of(&symbol.name);
        let neighbor_hit = callers
            .iter()
            .chain(callees.iter())
            .any(|name| candidate_names.contains(&name.to_ascii_lowercase()));
        let is_candidate_path = candidate_paths.contains(symbol.path.as_str());
        let direct_signal = task_symbol_has_name_or_feature_signal(symbol, tokens);
        if !(is_ranked_candidate || neighbor_hit || is_candidate_path && direct_signal) {
            continue;
        }

        let Ok(projection) = store.create_projection(symbol) else {
            continue;
        };

        let mut reasons = Vec::new();
        let mut score = task_symbol_rank(symbol, tokens);

        let source_score = task_source_rank(&projection.text, tokens);
        if source_score > 0 {
            reasons.push("content".to_string());
            score += source_score;
        }

        let chunk_score = task_chunk_rank(&projection.text, symbol, tokens, &mut reasons);
        score += chunk_score;

        if is_ranked_candidate {
            reasons.push("ranked_symbol".to_string());
            score += 220;
        }

        if neighbor_hit {
            reasons.push("call_neighbor".to_string());
            score += 80;
        }

        if reasons.is_empty() && !is_ranked_candidate && !neighbor_hit {
            continue;
        }

        score += task_path_prior(&symbol.path);

        if score <= 0 {
            continue;
        }

        dedupe_strings(&mut reasons);
        let call_lines = store.call_lines_for_symbol(symbol);
        let sliced = slice_code(&projection.text, &call_lines, symbol.line_start);
        let code = if sliced.was_sliced {
            sliced.code
        } else {
            numbered_code(&projection.text, symbol.line_start)
        };

        scored.push(TaskSlice {
            path: symbol.path.clone(),
            source_hash: symbol.source_hash.clone(),
            owner: format!("{}::{}", symbol.path, symbol.name),
            symbol: symbol.name.clone(),
            kind: symbol.kind.clone(),
            line_start: symbol.line_start,
            line_end: symbol.line_end,
            score,
            meaning: task_slice_meaning(symbol),
            responsibility_tags: task_responsibility_tags(symbol, &reasons),
            reasons,
            code,
        });
    }

    scored.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.line_start.cmp(&b.line_start))
            .then_with(|| a.symbol.cmp(&b.symbol))
    });
    scored.truncate(limit);
    scored
}

fn is_context_owner_symbol(symbol: &anchor::storage::SymbolEntry) -> bool {
    matches!(
        symbol.kind.as_str(),
        "Class" | "Struct" | "Enum" | "Interface" | "Trait" | "Function" | "Method" | "Impl"
    )
}

fn task_chunk_rank(
    text: &str,
    symbol: &anchor::storage::SymbolEntry,
    query_tokens: &std::collections::BTreeSet<String>,
    reasons: &mut Vec<String>,
) -> i32 {
    if query_tokens.is_empty() {
        return 0;
    }

    let mut score = 0;
    let lower_text = text.to_ascii_lowercase();
    let lower_path = symbol.path.to_ascii_lowercase();
    let lower_name = symbol.name.to_ascii_lowercase();
    let file_stem_tokens = Path::new(&lower_path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(task_search_tokens)
        .unwrap_or_default();
    let chunk_tokens = task_search_tokens(&format!("{} {} {}", symbol.name, symbol.path, text));
    let mut matched_query_terms = 0usize;

    for token in query_tokens {
        let mut token_matched = false;
        if lower_name == *token {
            reasons.push("symbol_exact".to_string());
            score += 180;
            token_matched = true;
        } else if lower_name.contains(token) {
            reasons.push("symbol_match".to_string());
            score += 90;
            token_matched = true;
        }

        if file_stem_tokens
            .iter()
            .any(|stem_token| soft_token_match(token, stem_token))
        {
            reasons.push("file_stem_match".to_string());
            score += 220;
            token_matched = true;
        } else if lower_path.contains(token) {
            reasons.push("path_match".to_string());
            score += 65;
            token_matched = true;
        }

        if lower_text.contains(token) {
            reasons.push("literal_match".to_string());
            score += 28;
            token_matched = true;
        } else if chunk_tokens
            .iter()
            .any(|chunk_token| soft_token_match(token, chunk_token))
        {
            reasons.push("soft_token_match".to_string());
            score += 14;
            token_matched = true;
        }

        if symbol.features.iter().any(|feature| feature == token) {
            reasons.push("feature_match".to_string());
            score += 35;
            token_matched = true;
        }

        if token_matched {
            matched_query_terms += 1;
        }
    }

    if matched_query_terms >= 2 {
        let coverage_terms = matched_query_terms.min(8);
        reasons.push("query_coverage".to_string());
        score += (coverage_terms * coverage_terms * 8) as i32;
    }

    score
}
