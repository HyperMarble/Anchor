fn cmd_task(
    root: &Path,
    intent_parts: &[String],
    limit: usize,
    context_limit: usize,
) -> Result<()> {
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
    let task_query = if task_tokens.is_empty() {
        intent.clone()
    } else {
        task_tokens.iter().cloned().collect::<Vec<_>>().join(" ")
    };
    let candidate_pool_limit = limit.max(context_limit).saturating_mul(8).clamp(24, 96);
    let mut candidates = store.search_symbols_hybrid(&task_query, candidate_pool_limit)?;
    candidates.extend(source_backed_task_candidates(
        root,
        &symbol_index.symbols,
        &task_tokens,
        candidate_pool_limit,
    ));
    dedupe_symbols(&mut candidates);
    let task_source_scores: BTreeMap<String, i32> = candidates
        .iter()
        .map(|symbol| {
            let source_score = store
                .create_projection(symbol)
                .map(|projection| task_source_rank(&projection.text, &task_tokens))
                .unwrap_or_default();
            (task_symbol_key(symbol), source_score)
        })
        .collect();
    candidates.sort_by(|a, b| {
        task_symbol_total_rank(b, &task_tokens, &task_source_scores)
            .cmp(&task_symbol_total_rank(
                a,
                &task_tokens,
                &task_source_scores,
            ))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.line_start.cmp(&b.line_start))
            .then_with(|| a.name.cmp(&b.name))
    });
    candidates.truncate(limit.max(context_limit));
    let current_paths: HashSet<String> = path_index
        .files
        .iter()
        .map(|entry| entry.path.clone())
        .collect();

    let mut related_files = BTreeSet::new();
    let mut historical_files: BTreeMap<String, usize> = BTreeMap::new();
    let mut historical_tests: BTreeMap<String, usize> = BTreeMap::new();

    for sym in &candidates {
        related_files.insert(sym.path.clone());
        let callers = call_index.callers_of(&sym.name);
        let callees = call_index.callees_of(&sym.name);
        for neighbor in callers.into_iter().chain(callees.into_iter()).take(8) {
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
    let source_seed_paths: BTreeSet<String> = seed_paths
        .iter()
        .filter(|path| !looks_like_test_path(path))
        .cloned()
        .collect();
    if history_index.adjacency.is_empty() {
        for edge in &history_index.cochanges {
            if !seed_paths.contains(&edge.path) || !current_paths.contains(&edge.related_path) {
                continue;
            }
            let edge_score = edge.score.max(edge.commits);
            let score = historical_files
                .entry(edge.related_path.clone())
                .or_default();
            *score += edge_score;
            if looks_like_test_path(&edge.related_path) {
                add_path_score(&mut historical_tests, edge.related_path.clone(), edge_score);
            }
        }
    } else {
        for seed_path in &seed_paths {
            let Some(neighbors) = history_index.adjacency.get(seed_path) else {
                continue;
            };
            for neighbor in neighbors {
                if !current_paths.contains(&neighbor.related_path) {
                    continue;
                }
                let score = historical_files
                    .entry(neighbor.related_path.clone())
                    .or_default();
                *score += neighbor.score.max(neighbor.commits);
                if neighbor.is_test {
                    add_path_score(
                        &mut historical_tests,
                        neighbor.related_path.clone(),
                        neighbor.score.max(neighbor.commits),
                    );
                }
            }
        }
    }

    for path in historical_files.keys() {
        related_files.insert(path.clone());
    }
    let task_slices = rank_task_slices(
        &store,
        &symbol_index.symbols,
        &call_index,
        &task_tokens,
        &candidates,
        limit.max(context_limit).saturating_mul(2).clamp(8, 24),
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
    let likely_tests: Vec<(&String, usize)> = likely_tests_owned
        .iter()
        .map(|test| (&test.path, test.score))
        .collect();
    let verification_plan = build_task_verification_plan(&likely_tests);
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
        &likely_tests,
        &verification_plan,
        &file_hashes,
    );
    save_task_packet(&store, &packet)?;

    events::record(
        store.anchor_root(),
        "task.intake",
        None,
        None,
        "ok",
        Some(format!(
            "intent={} scoped_files={} symbols={} context_symbols={} related_files={} tests={} historical_files={} historical_tests={}",
            intent,
            task_state.scoped_files,
            candidates.len(),
            context_limit.min(candidates.len()),
            related_files.len(),
            likely_tests_owned.len(),
            historical_files.len(),
            historical_tests.len()
        )),
    );

    print_task_intake_output(TaskIntakeOutput {
        store: &store,
        symbol_index: &symbol_index,
        call_index: &call_index,
        history_index: &history_index,
        scoped_files: task_state.scoped_files,
        intent: &intent,
        candidates: &candidates,
        context_limit,
        packet: &packet,
        related_files: &related_files,
        historical_files: &historical_files,
        likely_tests: &likely_tests,
        likely_test_count: likely_tests_owned.len(),
        historical_tests: &historical_tests,
    })?;

    Ok(())
}
