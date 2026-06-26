#[derive(Debug)]
struct TaskIndexState {
    store: AnchorStore,
    symbol_index: SymbolIndex,
    call_index: CallIndex,
    path_index: PathIndex,
    history_index: HistoryIndex,
    scoped_files: usize,
}

#[derive(Debug, Default)]
struct TaskFileCandidates {
    source_paths: Vec<String>,
    test_paths: Vec<String>,
}

fn prepare_task_index_state(
    root: &Path,
    task_tokens: &std::collections::BTreeSet<String>,
    limit: usize,
) -> Result<TaskIndexState> {
    let store = open_store(root)?;
    let mut history_index = store.load_history_index();
    if history_index.schema.is_empty() && !store.history_index_path().exists() {
        history_index = build_history_index(root);
        store.save_history_index(&history_index)?;
    }

    let candidates = task_file_candidates(root, task_tokens, &history_index, limit)?;
    let scoped_files = refresh_task_scoped_indexes(root, &store, &candidates)?;

    Ok(TaskIndexState {
        symbol_index: store.load_symbol_index()?,
        call_index: store.load_call_index(),
        path_index: store.load_path_index()?,
        history_index,
        store,
        scoped_files,
    })
}

fn task_file_candidates(
    root: &Path,
    task_tokens: &std::collections::BTreeSet<String>,
    history_index: &HistoryIndex,
    limit: usize,
) -> Result<TaskFileCandidates> {
    use std::collections::BTreeMap;
    use std::io::Read;

    const TASK_SOURCE_SCAN_BYTES: usize = 48 * 1024;
    const TASK_SOURCE_SCAN_TOTAL_BYTES: usize = 16 * 1024 * 1024;

    let source_limit = limit.clamp(8, 24);
    let test_limit = limit.saturating_mul(2).clamp(8, 24);
    let history_scores: BTreeMap<&str, usize> = history_index
        .paths
        .iter()
        .map(|entry| (entry.path.as_str(), entry.score.max(entry.commits)))
        .collect();
    let mut remaining_scan_bytes = TASK_SOURCE_SCAN_TOTAL_BYTES;
    let mut sources: BTreeMap<String, usize> = BTreeMap::new();
    let mut tests: BTreeMap<String, usize> = BTreeMap::new();
    let mut scan_queue: Vec<(String, PathBuf, usize)> = Vec::new();

    for entry in Walk::new(root).filter_map(|entry| entry.ok()) {
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let path = entry.path();
        if !is_indexable_text_path(path) {
            continue;
        }
        let Some(relative) = repo_relative_string(root, path) else {
            continue;
        };
        if is_task_ignored_path(&relative) {
            continue;
        }

        let mut score = task_file_path_score(&relative, task_tokens);
        score += history_scores
            .get(relative.as_str())
            .copied()
            .unwrap_or_default()
            .min(500);

        if looks_like_test_path(&relative) {
            if score > 0 {
                tests.insert(relative, score);
            }
            continue;
        }
        if !anchor::parser::language::is_source_path(path) {
            continue;
        }

        scan_queue.push((relative, path.to_path_buf(), score));
    }

    // Path-scored files spend the content-scan budget first so unrelated
    // files in a large repo cannot starve out the likely ones, but files with
    // no path signal still get scanned with leftover budget: the task terms
    // may only appear in the code itself.
    scan_queue.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    for (relative, path, mut score) in scan_queue {
        if remaining_scan_bytes > 0 {
            if let Ok(metadata) = std::fs::metadata(&path) {
                let bytes_to_scan = (metadata.len() as usize)
                    .min(TASK_SOURCE_SCAN_BYTES)
                    .min(remaining_scan_bytes);
                if bytes_to_scan > 0 {
                    if let Ok(file) = std::fs::File::open(&path) {
                        let mut bytes = Vec::new();
                        let mut limited = file.take(bytes_to_scan as u64);
                        let _ = limited.read_to_end(&mut bytes);
                        let prefix = String::from_utf8_lossy(&bytes);
                        score += task_source_rank(&prefix, task_tokens).max(0) as usize * 2;
                        remaining_scan_bytes = remaining_scan_bytes.saturating_sub(bytes_to_scan);
                    }
                }
            }
        }

        if score > 0 {
            sources.insert(relative, score);
        }
    }

    let source_paths: Vec<String> = top_scored_owned_paths(&sources, source_limit)
        .into_iter()
        .map(|(path, _)| path)
        .collect();

    for source_path in &source_paths {
        for (test_path, score) in tests.iter_mut() {
            *score += source_test_affinity_score(source_path, test_path);
        }
        if let Some(neighbors) = history_index.adjacency.get(source_path) {
            for neighbor in neighbors {
                if neighbor.is_test {
                    let score = tests.entry(neighbor.related_path.clone()).or_default();
                    *score += neighbor.score.max(neighbor.commits).min(500);
                }
            }
        }
    }

    let test_paths: Vec<String> = top_scored_owned_paths(&tests, test_limit)
        .into_iter()
        .map(|(path, _)| path)
        .collect();

    Ok(TaskFileCandidates {
        source_paths,
        test_paths,
    })
}

fn refresh_task_scoped_indexes(
    root: &Path,
    store: &AnchorStore,
    candidates: &TaskFileCandidates,
) -> Result<usize> {
    const TASK_PARSE_FILE_BYTES_MAX: u64 = 512 * 1024;

    let mut call_index = store.load_call_index();
    let mut scoped_files = 0usize;

    for relative in &candidates.source_paths {
        let source_path = root.join(relative);
        if std::fs::metadata(&source_path)
            .map(|metadata| metadata.len() > TASK_PARSE_FILE_BYTES_MAX)
            .unwrap_or(false)
        {
            continue;
        }
        let old_symbols = store
            .load_symbol_index()?
            .symbols
            .into_iter()
            .filter(|symbol| symbol.path == *relative)
            .collect::<Vec<_>>();
        for symbol in old_symbols {
            call_index.calls.remove(&symbol.name);
        }

        let (_, symbols, _) = match store.upsert_symbols_for_path(&source_path) {
            Ok(result) => result,
            Err(_) => continue,
        };
        if symbols.is_empty() {
            continue;
        }
        if let Ok(source) = std::fs::read_to_string(&source_path) {
            if let Ok(extraction) = anchor::parser::extract_file(&source_path, &source) {
                for (caller, callee) in extracted_calls(&extraction) {
                    let callees = call_index.calls.entry(caller).or_default();
                    if !callees.contains(&callee) {
                        callees.push(callee);
                    }
                }
            }
        }
        scoped_files += 1;
    }

    for relative in &candidates.test_paths {
        let test_path = root.join(relative);
        if store.upsert_path(&test_path).is_ok() {
            scoped_files += 1;
        }
    }

    for callees in call_index.calls.values_mut() {
        callees.sort();
        callees.dedup();
    }
    store.save_call_index(&call_index)?;

    Ok(scoped_files)
}
