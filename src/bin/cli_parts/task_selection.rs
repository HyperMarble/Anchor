fn select_diverse_task_slices(slices: &[TaskSlice], limit: usize) -> Vec<TaskSlice> {
    let mut selected = Vec::new();
    let mut per_file: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    let mut remaining: Vec<&TaskSlice> = slices.iter().collect();

    while selected.len() < limit && !remaining.is_empty() {
        let best = remaining
            .iter()
            .enumerate()
            .map(|(idx, slice)| {
                let selected_for_file = per_file.get(slice.path.as_str()).copied().unwrap_or(0);
                let divisor = match selected_for_file {
                    0 => 1,
                    1 => 2,
                    _ => 4,
                };
                (idx, slice.score / divisor)
            })
            .max_by(|left, right| {
                left.1
                    .cmp(&right.1)
                    .then_with(|| remaining[right.0].path.cmp(&remaining[left.0].path))
                    .then_with(|| {
                        remaining[right.0]
                            .line_start
                            .cmp(&remaining[left.0].line_start)
                    })
            });

        let Some((idx, effective_score)) = best else {
            break;
        };
        if effective_score <= 0 {
            break;
        }
        let slice = remaining.swap_remove(idx);
        *per_file.entry(slice.path.as_str()).or_default() += 1;
        selected.push(slice.clone());
    }

    selected
}

fn rank_task_tests(
    files: &[anchor::storage::PathEntry],
    task_tokens: &std::collections::BTreeSet<String>,
    candidates: &[anchor::storage::SymbolEntry],
    slices: &[TaskSlice],
    source_seed_paths: &std::collections::BTreeSet<String>,
    historical_tests: &std::collections::BTreeMap<String, usize>,
    limit: usize,
) -> Vec<TaskTest> {
    let owner_chunks = select_diverse_task_slices(slices, 12);
    let mut active_source_weights: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for (idx, slice) in owner_chunks.iter().enumerate() {
        let rank_weight = 12usize.saturating_sub(idx.min(11));
        let score_weight = (slice.score.max(0) as usize / 500).clamp(1, 8);
        let entry = active_source_weights.entry(slice.path.clone()).or_default();
        *entry += rank_weight + score_weight;
    }
    if active_source_weights.is_empty() {
        for source_path in source_seed_paths.iter().take(8) {
            active_source_weights.insert(source_path.clone(), 2);
        }
    }

    let mut active_path_tokens = std::collections::BTreeSet::new();
    for source_path in active_source_weights.keys() {
        active_path_tokens.extend(path_signal_tokens(source_path));
    }

    let mut active_symbol_tokens = std::collections::BTreeSet::new();
    for slice in &owner_chunks {
        active_symbol_tokens.extend(task_search_tokens(&slice.symbol));
    }
    for symbol in candidates.iter().take(8) {
        active_symbol_tokens.extend(task_search_tokens(&symbol.name));
    }

    let mut scored = std::collections::BTreeMap::new();
    for file in files {
        if !looks_like_test_path(&file.path) || is_test_helper_path(&file.path) {
            continue;
        }
        let path_lower = file.path.to_ascii_lowercase();
        let mut score = historical_tests
            .get(&file.path)
            .copied()
            .unwrap_or_default()
            .saturating_mul(4);

        for (source_path, weight) in &active_source_weights {
            score += source_test_affinity_score(source_path, &file.path) * *weight;
        }
        let test_path_tokens = path_signal_tokens(&file.path);
        for token in &active_path_tokens {
            if path_lower.contains(token.as_str())
                || test_path_tokens
                    .iter()
                    .any(|test_token| soft_token_match(token, test_token))
            {
                score += 180;
            }
        }
        for token in &active_symbol_tokens {
            if token.len() >= 4 && path_lower.contains(token.as_str()) {
                score += 60;
            }
        }
        for token in task_tokens {
            if path_lower.contains(token.as_str()) {
                score += 20;
            }
        }
        if score > 0 {
            scored.insert(file.path.clone(), score);
        }
    }

    top_scored_paths(&scored, limit)
        .into_iter()
        .map(|(path, score)| TaskTest {
            path: path.clone(),
            score,
            reasons: vec!["source_test_affinity".to_string()],
        })
        .collect()
}

fn is_test_helper_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let file_name = Path::new(&lower)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    file_name == "conftest.py"
        || file_name == "__init__.py"
        || file_name.contains("helper")
        || file_name.contains("fixture")
        || lower.contains("/examples/")
        || lower.starts_with("examples/")
}

