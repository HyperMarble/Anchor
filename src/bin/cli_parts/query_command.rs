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
    let signal_tokens = task_specific_tokens(&tokens);
    let allow_support_context = task_support_context_requested(&tokens);
    let mut candidates = state
        .store
        .search_symbols_hybrid(&query_text, limit.saturating_mul(8).clamp(24, 96))?;
    retain_query_owner_candidates(&mut candidates, &signal_tokens, allow_support_context);
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
    retain_existing_symbol_paths(root, &mut candidates);
    dedupe_symbols(&mut candidates);

    let slices = rank_task_slices(
        &state.store,
        &state.symbol_index.symbols,
        &state.call_index,
        &tokens,
        &candidates,
        limit.saturating_mul(3).clamp(8, 24),
        false,
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
    let report = query_report(
        &intent,
        state.scoped_files,
        &chunks,
        &candidates,
        &tests,
        &state,
        &tokens,
    );
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
    let store = ensure_query_indexed_store(root)?;
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
    tokens: &std::collections::BTreeSet<String>,
) -> QueryReport {
    QueryReport {
        schema: "anchor.query.v1",
        intent: intent.to_string(),
        scoped_files,
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
        files: query_files(chunks, candidates, tokens),
        next: vec![
            "anchor view <chunk-handle> or anchor read <chunk-handle>".to_string(),
            "anchor read <chunk-handle> --around <text> for an enclosing block".to_string(),
            "anchor read file:<path> --around <text> to resolve text to an owner chunk".to_string(),
            "anchor edit/write with the returned source hash".to_string(),
        ],
    }
}

fn retain_existing_symbol_paths(root: &Path, symbols: &mut Vec<SymbolEntry>) {
    symbols.retain(|symbol| root.join(&symbol.path).is_file());
}

fn retain_query_owner_candidates(
    symbols: &mut Vec<SymbolEntry>,
    signal_tokens: &std::collections::BTreeSet<String>,
    allow_support_context: bool,
) {
    symbols.retain(|symbol| {
        if looks_like_test_path(&symbol.path) {
            return false;
        }
        if is_support_context_path(&symbol.path) && !allow_support_context {
            return false;
        }
        task_path_has_signal(&symbol.path, signal_tokens)
            || task_symbol_has_name_or_feature_signal(symbol, signal_tokens)
    });
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

fn query_files(
    chunks: &[TaskSlice],
    candidates: &[SymbolEntry],
    tokens: &std::collections::BTreeSet<String>,
) -> Vec<QueryFile> {
    let mut seen = std::collections::BTreeSet::new();
    let mut files = Vec::new();
    let allow_support_context = task_support_context_requested(tokens);
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
        if looks_like_test_path(&symbol.path) {
            continue;
        }
        if is_support_context_path(&symbol.path) && !allow_support_context {
            continue;
        }
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
