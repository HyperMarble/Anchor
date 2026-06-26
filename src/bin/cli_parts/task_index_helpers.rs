fn extracted_calls(extraction: &anchor::parser::FileExtractions) -> Vec<(String, String)> {
    use std::collections::HashMap;

    let mut name_count: HashMap<String, usize> = HashMap::new();
    for symbol in &extraction.symbols {
        *name_count.entry(symbol.name.clone()).or_default() += 1;
    }
    let qualified: HashMap<String, String> = extraction
        .symbols
        .iter()
        .filter(|symbol| name_count[&symbol.name] == 1)
        .filter_map(|symbol| {
            symbol
                .parent
                .as_ref()
                .map(|parent| (symbol.name.clone(), format!("{}::{}", parent, symbol.name)))
        })
        .collect();

    extraction
        .calls
        .iter()
        .map(|call| {
            let caller = qualified
                .get(&call.caller)
                .cloned()
                .unwrap_or_else(|| call.caller.clone());
            (caller, call.callee.clone())
        })
        .collect()
}

fn repo_relative_string(root: &Path, path: &Path) -> Option<String> {
    Some(
        path.strip_prefix(root)
            .ok()?
            .to_string_lossy()
            .replace('\\', "/"),
    )
}

fn is_task_ignored_path(path: &str) -> bool {
    path.starts_with(".anchor/")
        || path.starts_with(".git/")
        || path.contains("/.anchor/")
        || path.contains("/.git/")
        || path.contains("/node_modules/")
        || path.contains("/target/")
        || path.contains("/dist/")
        || path.contains("/build/")
        || path.contains("/vendor/")
        || path.contains("/__pycache__/")
}

fn task_file_path_score(path: &str, tokens: &std::collections::BTreeSet<String>) -> usize {
    let lower_path = path.to_ascii_lowercase();
    let path_tokens = path_signal_tokens(path);
    let mut score = 0usize;
    let mut matched_terms = 0usize;

    for token in tokens {
        let mut matched = false;
        if lower_path.contains(token) {
            score += 80;
            matched = true;
        }
        if path_tokens
            .iter()
            .any(|path_token| soft_token_match(token, path_token))
        {
            score += 120;
            matched = true;
        }
        if matched {
            matched_terms += 1;
        }
    }

    if matched_terms >= 2 {
        score += matched_terms.min(8) * matched_terms.min(8) * 20;
    }
    if looks_like_test_path(path) {
        score += 30;
    }

    score
}

fn top_scored_owned_paths(
    scores: &std::collections::BTreeMap<String, usize>,
    limit: usize,
) -> Vec<(String, usize)> {
    let mut items: Vec<_> = scores
        .iter()
        .map(|(path, score)| (path.clone(), *score))
        .collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    items.truncate(limit);
    items
}

fn is_git_history_path_indexable(path: &str) -> bool {
    if path.starts_with(".anchor/")
        || path.starts_with(".git/")
        || path.contains("/__pycache__/")
        || path.ends_with(".pyc")
        || path.ends_with(".pyo")
    {
        return false;
    }
    let path_obj = Path::new(path);
    is_indexable_text_path(path_obj)
}

fn looks_like_test_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.contains("/tests/")
        || lower.starts_with("tests/")
        || lower.contains("/test/")
        || lower.starts_with("test/")
        || lower.contains("_test.")
        || lower.contains("test_")
        || lower.ends_with(".spec.ts")
        || lower.ends_with(".test.ts")
        || lower.ends_with(".spec.tsx")
        || lower.ends_with(".test.tsx")
        || lower.ends_with(".spec.js")
        || lower.ends_with(".test.js")
        || lower.ends_with(".spec.jsx")
        || lower.ends_with(".test.jsx")
}

