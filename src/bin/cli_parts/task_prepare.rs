struct PreparedTaskWorkspace {
    store: AnchorStore,
    symbol_index: SymbolIndex,
    call_index: CallIndex,
    history_index: HistoryIndex,
    scoped_files: usize,
    intent: String,
    candidates: Vec<SymbolEntry>,
    context_limit: usize,
    packet: TaskPacket,
    related_files: std::collections::BTreeSet<String>,
    historical_files: std::collections::BTreeMap<String, usize>,
    likely_tests_owned: Vec<TaskTest>,
    historical_tests: std::collections::BTreeMap<String, usize>,
}

fn prepare_task_workspace(
    root: &Path,
    intent_parts: &[String],
    limit: usize,
    context_limit: usize,
) -> Result<PreparedTaskWorkspace> {
    use anchor::parser::language::is_source_path;
    use std::collections::{BTreeMap, BTreeSet, HashSet};

    let intent = intent_parts.join(" ");
    if intent.trim().is_empty() {
        bail!("task requires an intent");
    }

    let task_tokens = task_intent_tokens(&intent);
    let task_state =
        prepare_task_index_state(root, &task_tokens, limit.max(context_limit).clamp(8, 16))?;
    let store = task_state.store;
    let symbol_index = task_state.symbol_index;
    let call_index = task_state.call_index;
    let path_index = task_state.path_index;
    let history_index = task_state.history_index;
    let query = if task_tokens.is_empty() {
        intent.clone()
    } else {
        task_tokens.iter().cloned().collect::<Vec<_>>().join(" ")
    };

    let pool_limit = limit.max(context_limit).saturating_mul(8).clamp(24, 96);
    let mut candidates = store.search_symbols_hybrid(&query, pool_limit)?;
    candidates.extend(source_backed_task_candidates(
        root,
        &symbol_index.symbols,
        &task_tokens,
        pool_limit,
    ));
    retain_query_owner_candidates(
        &mut candidates,
        &task_specific_tokens(&task_tokens),
        task_support_context_requested(&task_tokens),
    );
    dedupe_symbols(&mut candidates);
    sort_task_candidates(&store, &task_tokens, &mut candidates);
    candidates.truncate(limit.max(context_limit));

    let current_paths: HashSet<String> =
        path_index.files.iter().map(|entry| entry.path.clone()).collect();
    let mut related_files = BTreeSet::new();
    for sym in &candidates {
        related_files.insert(sym.path.clone());
        for neighbor in call_index
            .callers_of(&sym.name)
            .into_iter()
            .chain(call_index.callees_of(&sym.name).into_iter())
            .take(8)
        {
            for hit in store.search_symbols_hybrid(neighbor, 2).unwrap_or_default() {
                if is_source_path(Path::new(&hit.path)) {
                    related_files.insert(hit.path);
                }
            }
        }
    }

    let seed_paths: BTreeSet<String> = candidates
        .iter()
        .map(|sym| sym.path.clone())
        .chain(related_files.iter().cloned())
        .collect();
    let mut historical_files = BTreeMap::new();
    let mut historical_tests = BTreeMap::new();
    add_history_neighbors(
        &history_index,
        &seed_paths,
        &current_paths,
        &mut historical_files,
        &mut historical_tests,
    );
    related_files.extend(historical_files.keys().cloned());

    let source_seed_paths: BTreeSet<String> = seed_paths
        .iter()
        .filter(|path| !looks_like_test_path(path))
        .cloned()
        .collect();
    let task_slices = rank_task_slices(
        &store,
        &symbol_index.symbols,
        &call_index,
        &task_tokens,
        &candidates,
        limit.max(context_limit).saturating_mul(2).clamp(8, 24),
        true,
    );
    let likely_tests_owned = rank_task_tests(
        &path_index.files,
        &task_tokens,
        &candidates,
        &task_slices,
        &source_seed_paths,
        &historical_tests,
        12,
    );
    let likely_test_refs: Vec<(&String, usize)> = likely_tests_owned
        .iter()
        .map(|test| (&test.path, test.score))
        .collect();
    let verification_plan = build_task_verification_plan(&likely_test_refs);
    let file_hashes: std::collections::BTreeMap<&str, &str> = path_index
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.source_hash.as_str()))
        .collect();
    let packet = build_task_packet(
        &intent,
        &task_slices,
        &related_files,
        &historical_files,
        &likely_test_refs,
        &verification_plan,
        &file_hashes,
    );
    save_task_packet(&store, &packet)?;

    Ok(PreparedTaskWorkspace {
        store,
        symbol_index,
        call_index,
        history_index,
        scoped_files: task_state.scoped_files,
        intent,
        candidates,
        context_limit,
        packet,
        related_files,
        historical_files,
        likely_tests_owned,
        historical_tests,
    })
}

fn sort_task_candidates(
    store: &AnchorStore,
    tokens: &std::collections::BTreeSet<String>,
    candidates: &mut [SymbolEntry],
) {
    let scores: std::collections::BTreeMap<String, i32> = candidates
        .iter()
        .map(|symbol| {
            let score = store
                .create_projection(symbol)
                .map(|projection| task_source_rank(&projection.text, tokens))
                .unwrap_or_default();
            (task_symbol_key(symbol), score)
        })
        .collect();
    candidates.sort_by(|a, b| {
        task_symbol_total_rank(b, tokens, &scores)
            .cmp(&task_symbol_total_rank(a, tokens, &scores))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.line_start.cmp(&b.line_start))
            .then_with(|| a.name.cmp(&b.name))
    });
}

fn add_history_neighbors(
    history: &HistoryIndex,
    seeds: &std::collections::BTreeSet<String>,
    current_paths: &std::collections::HashSet<String>,
    files: &mut std::collections::BTreeMap<String, usize>,
    tests: &mut std::collections::BTreeMap<String, usize>,
) {
    for seed in seeds {
        let neighbors = history.adjacency.get(seed).cloned().unwrap_or_default();
        for neighbor in neighbors {
            if !current_paths.contains(&neighbor.related_path) {
                continue;
            }
            let score = neighbor.score.max(neighbor.commits);
            *files.entry(neighbor.related_path.clone()).or_default() += score;
            if neighbor.is_test {
                add_path_score(tests, neighbor.related_path, score);
            }
        }
    }
    if !history.adjacency.is_empty() {
        return;
    }
    for edge in &history.cochanges {
        if !seeds.contains(&edge.path) || !current_paths.contains(&edge.related_path) {
            continue;
        }
        let score = edge.score.max(edge.commits);
        *files.entry(edge.related_path.clone()).or_default() += score;
        if looks_like_test_path(&edge.related_path) {
            add_path_score(tests, edge.related_path.clone(), score);
        }
    }
}
