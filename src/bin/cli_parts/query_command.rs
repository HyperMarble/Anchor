#[derive(Debug, Serialize)]
struct ViewReport {
    schema: &'static str,
    handle: String,
    kind: String,
    path: String,
    symbol: Option<String>,
    line_start: usize,
    line_end: usize,
    source_hash: String,
    slice_hash: String,
    refreshed: bool,
    code: String,
}

fn cmd_query(root: &Path, query: &[String], limit: usize, json: bool) -> Result<()> {
    let intent = query.join(" ");
    if intent.trim().is_empty() {
        bail!("query requires search text or task intent");
    }
    let tokens = task_intent_tokens(&intent);
    let state = prepare_task_index_state(root, &tokens, limit.clamp(8, 24))?;
    let query_text = if tokens.is_empty() {
        intent.clone()
    } else {
        tokens.iter().cloned().collect::<Vec<_>>().join(" ")
    };
    let mut candidates = state
        .store
        .search_symbols_hybrid(&query_text, limit.saturating_mul(8).clamp(24, 96))?;
    candidates.extend(source_backed_task_candidates(
        root,
        &state.symbol_index.symbols,
        &tokens,
        limit.saturating_mul(4).clamp(12, 48),
    ));
    candidates.extend(content_backed_query_candidates(
        root,
        &state.symbol_index.symbols,
        &tokens,
        limit.saturating_mul(4).clamp(12, 48),
    ));
    dedupe_symbols(&mut candidates);

    let slices = rank_task_slices(
        &state.store,
        &state.symbol_index.symbols,
        &state.call_index,
        &tokens,
        &candidates,
        limit.saturating_mul(3).clamp(8, 24),
    );
    let chunks = select_diverse_task_slices(&slices, limit);
    let source_paths = query_source_paths(&chunks, &candidates);
    let tests = rank_task_tests(
        &state.path_index.files,
        &tokens,
        &candidates,
        &chunks,
        &source_paths,
        &std::collections::BTreeMap::new(),
        8,
    );
    let report = query_report(&intent, state.scoped_files, &chunks, &candidates, &tests, &state);
    events::record(
        state.store.anchor_root(),
        "query",
        None,
        None,
        "ok",
        Some(format!(
            "intent={} chunks={} files={} tests={}",
            intent,
            report.chunks.len(),
            report.files.len(),
            report.tests.len()
        )),
    );
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_query_report(&report);
    }
    Ok(())
}

fn cmd_view(root: &Path, handle: &str, around: Option<&str>, full: bool, json: bool) -> Result<()> {
    let store = ensure_indexed_store(root)?;
    let report = match parse_query_handle(handle)? {
        QueryHandle::File(path) => view_path(root, &store, handle, "file", &path, around, full)?,
        QueryHandle::Test(path) => view_path(root, &store, handle, "test", &path, around, full)?,
        chunk @ QueryHandle::Chunk { .. } => {
            view_chunk(root, &store, handle, &chunk, around, full)?
        }
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_view_report(&report);
    }
    Ok(())
}

fn query_report(
    intent: &str,
    scoped_files: usize,
    chunks: &[TaskSlice],
    candidates: &[SymbolEntry],
    tests: &[TaskTest],
    state: &TaskIndexState,
) -> QueryReport {
    QueryReport {
        schema: "anchor.query.v1",
        intent: intent.to_string(),
        scoped_files,
        files: query_files(chunks, candidates),
        chunks: chunks
            .iter()
            .map(|slice| QueryChunk {
                handle: chunk_handle(&slice.path, &slice.symbol, slice.line_start, slice.line_end),
                path: slice.path.clone(),
                symbol: slice.symbol.clone(),
                kind: slice.kind.clone(),
                line_start: slice.line_start,
                line_end: slice.line_end,
                source_hash: slice.source_hash.clone(),
                score: slice.score,
                reasons: slice.reasons.clone(),
                calls: state
                    .call_index
                    .callees_of(&slice.symbol)
                    .into_iter()
                    .take(8)
                    .map(ToString::to_string)
                    .collect(),
                called_by: state
                    .call_index
                    .callers_of(&slice.symbol)
                    .into_iter()
                    .take(8)
                    .map(ToString::to_string)
                    .collect(),
            })
            .collect(),
        tests: tests
            .iter()
            .map(|test| QueryTestFile {
                handle: test_handle(&test.path),
                path: test.path.clone(),
                score: test.score,
                reasons: test.reasons.clone(),
            })
            .collect(),
        next: vec![
            "anchor view <handle>".to_string(),
            "anchor edit/write with the returned source hash".to_string(),
        ],
    }
}

fn query_source_paths(
    chunks: &[TaskSlice],
    candidates: &[SymbolEntry],
) -> std::collections::BTreeSet<String> {
    chunks
        .iter()
        .map(|chunk| chunk.path.clone())
        .chain(candidates.iter().take(8).map(|symbol| symbol.path.clone()))
        .collect()
}

fn query_files(chunks: &[TaskSlice], candidates: &[SymbolEntry]) -> Vec<QueryFile> {
    let mut seen = std::collections::BTreeSet::new();
    let mut files = Vec::new();
    for chunk in chunks {
        if seen.insert(chunk.path.clone()) {
            files.push(QueryFile {
                handle: file_handle(&chunk.path),
                path: chunk.path.clone(),
                source_hash: chunk.source_hash.clone(),
                score: chunk.score,
                reason: "owner_chunk".to_string(),
            });
        }
    }
    for symbol in candidates.iter().take(12) {
        if seen.insert(symbol.path.clone()) {
            files.push(QueryFile {
                handle: file_handle(&symbol.path),
                path: symbol.path.clone(),
                source_hash: symbol.source_hash.clone(),
                score: task_symbol_rank(symbol, &std::collections::BTreeSet::new()),
                reason: "symbol_candidate".to_string(),
            });
        }
    }
    files.truncate(12);
    files
}

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
    let mut scored = Vec::new();
    for (path, path_symbols) in by_path {
        if looks_like_test_path(path) {
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
            scored.push((source_score * 3 + owner_bonus + task_symbol_rank(symbol, tokens), (*symbol).clone()));
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
