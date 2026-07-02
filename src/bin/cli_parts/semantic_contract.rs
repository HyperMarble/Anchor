fn materialize_contract_semantic_workspace(
    root: &Path,
    intent: &str,
    owner_limit: usize,
) -> Result<Option<PathBuf>> {
    let store = ensure_query_indexed_store(root)?;
    let packet = match contract_packet(&store, intent, owner_limit)? {
        Some(packet) => packet,
        None => return Ok(None),
    };
    let path = materialize_semantic_workspace(&store, &packet, owner_limit)?;
    events::record(
        store.anchor_root(),
        "semantic.contract",
        None,
        None,
        "ok",
        Some(format!(
            "intent={} owners={} tests={}",
            intent,
            packet.owner_chunks.len(),
            packet.likely_tests.len()
        )),
    );
    Ok(Some(path))
}

fn contract_packet(
    store: &AnchorStore,
    intent: &str,
    owner_limit: usize,
) -> Result<Option<TaskPacket>> {
    let path_index = store.load_path_index()?;
    let symbol_index = store.load_symbol_index()?;
    let all_terms = contract_terms(intent);
    let explicit_terms = contract_search_terms(intent);
    let owner_terms = if explicit_terms.is_empty() {
        contract_terms(intent)
    } else {
        explicit_terms
    };
    let explicit_paths = contract_paths(intent, &path_index.files);
    let term_order = contract_term_order(&owner_terms);
    let owners = contract_owner_chunks(
        store,
        &symbol_index.symbols,
        &owner_terms,
        &term_order,
        &explicit_paths,
        owner_limit.clamp(3, 12),
    )?;

    if owners.is_empty() && explicit_paths.is_empty() {
        return Ok(None);
    }

    let mut likely_files = contract_likely_files(&owners, &explicit_paths, &path_index.files);
    likely_files.truncate(8);
    let likely_tests = contract_likely_tests(&path_index.files, &all_terms, &owners);
    let likely_test_refs: Vec<(&String, usize)> = likely_tests
        .iter()
        .map(|test| (&test.path, test.score))
        .collect();
    let verification_plan = build_task_verification_plan(&likely_test_refs);

    Ok(Some(TaskPacket {
        schema: "anchor.semantic_contract.v1".to_string(),
        intent: intent.to_string(),
        likely_files,
        owner_chunks: owners,
        related_files: Vec::new(),
        likely_tests,
        verification_plan,
    }))
}

fn contract_owner_chunks(
    store: &AnchorStore,
    symbols: &[SymbolEntry],
    terms: &std::collections::BTreeSet<String>,
    term_order: &std::collections::BTreeMap<String, usize>,
    explicit_paths: &std::collections::BTreeSet<String>,
    limit: usize,
) -> Result<Vec<TaskSlice>> {
    let mut out = Vec::new();
    for symbol in symbols {
        if !is_context_owner_symbol(symbol) {
            continue;
        }
        let name = symbol.name.to_ascii_lowercase();
        if !terms.contains(&name) {
            continue;
        }
        if !explicit_paths.is_empty() && !explicit_paths.contains(&symbol.path) {
            continue;
        }
        let projection = store.create_projection(symbol)?;
        out.push(TaskSlice {
            path: symbol.path.clone(),
            source_hash: projection.source_hash,
            owner: format!("{}::{}", symbol.path, symbol.name),
            symbol: symbol.name.clone(),
            kind: symbol.kind.clone(),
            line_start: symbol.line_start,
            line_end: symbol.line_end,
            score: 10_000,
            reasons: vec!["contract_symbol".to_string()],
            meaning: task_slice_meaning(symbol),
            responsibility_tags: vec!["contract".to_string(), infer_task_file_role(&symbol.path)],
            code: numbered_code(&projection.text, symbol.line_start),
        });
    }
    out.sort_by(|a, b| {
        contract_symbol_order(&a.symbol, term_order)
            .cmp(&contract_symbol_order(&b.symbol, term_order))
            .then_with(|| {
                exact_contract_symbol(&b.symbol, terms)
                    .cmp(&exact_contract_symbol(&a.symbol, terms))
            })
            .then_with(|| {
                a.path
                    .cmp(&b.path)
                    .then_with(|| a.line_start.cmp(&b.line_start))
                    .then_with(|| a.symbol.cmp(&b.symbol))
            })
    });
    dedupe_task_slices(&mut out);
    Ok(select_contract_owners(out, limit))
}

fn select_contract_owners(mut owners: Vec<TaskSlice>, limit: usize) -> Vec<TaskSlice> {
    let mut selected = Vec::new();
    let mut seen_symbols = std::collections::BTreeSet::new();
    let mut rest = Vec::new();
    for owner in owners.drain(..) {
        if selected.len() < limit && seen_symbols.insert(owner.symbol.clone()) {
            selected.push(owner);
        } else {
            rest.push(owner);
        }
    }
    for owner in rest {
        if selected.len() >= limit {
            break;
        }
        selected.push(owner);
    }
    selected
}

fn contract_likely_files(
    owners: &[TaskSlice],
    explicit_paths: &std::collections::BTreeSet<String>,
    indexed_files: &[anchor::storage::PathEntry],
) -> Vec<TaskPath> {
    let hashes: std::collections::BTreeMap<&str, &str> = indexed_files
        .iter()
        .map(|file| (file.path.as_str(), file.source_hash.as_str()))
        .collect();
    let mut paths = explicit_paths.clone();
    paths.extend(owners.iter().map(|owner| owner.path.clone()));
    paths
        .into_iter()
        .map(|path| {
            if let Some(hash) = hashes.get(path.as_str()) {
                TaskPath {
                    path,
                    source_hash: (*hash).to_string(),
                    score: 10_000,
                    role: "contract".to_string(),
                    reasons: vec!["contract_path_or_owner".to_string()],
                }
            } else {
                TaskPath {
                    path,
                    source_hash: "missing".to_string(),
                    score: 9_000,
                    role: "planned_new_file".to_string(),
                    reasons: vec!["explicit_contract_path_missing_from_index".to_string()],
                }
            }
        })
        .collect()
}

fn dedupe_task_slices(slices: &mut Vec<TaskSlice>) {
    let mut seen = std::collections::BTreeSet::new();
    slices.retain(|slice| {
        seen.insert(format!(
            "{}:{}:{}:{}",
            slice.path, slice.symbol, slice.line_start, slice.line_end
        ))
    });
}

fn contract_likely_tests(
    indexed_files: &[anchor::storage::PathEntry],
    terms: &std::collections::BTreeSet<String>,
    owners: &[TaskSlice],
) -> Vec<TaskTest> {
    let mut test_terms = terms.clone();
    for owner in owners {
        test_terms.extend(task_search_tokens(&owner.symbol));
        test_terms.extend(path_signal_tokens(&owner.path));
    }
    let mut scored = Vec::new();
    for file in indexed_files {
        if !looks_like_test_path(&file.path) || is_test_helper_path(&file.path) {
            continue;
        }
        let path_tokens = path_signal_tokens(&file.path);
        let matches = path_tokens.intersection(&test_terms).count();
        if matches == 0 {
            continue;
        }
        scored.push(TaskTest {
            path: file.path.clone(),
            score: matches * matches * 100,
            reasons: vec!["contract_test_token_match".to_string()],
        });
    }
    scored.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    scored.truncate(6);
    scored
}
