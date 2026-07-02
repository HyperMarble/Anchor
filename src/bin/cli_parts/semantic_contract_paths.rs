fn contract_paths(
    intent: &str,
    indexed_files: &[anchor::storage::PathEntry],
) -> std::collections::BTreeSet<String> {
    let lower = intent.to_ascii_lowercase();
    let mut paths: std::collections::BTreeSet<String> = indexed_files
        .iter()
        .filter(|file| lower.contains(&file.path.to_ascii_lowercase()))
        .map(|file| file.path.clone())
        .collect();
    for token in path_tokens_from_text(intent, false) {
        add_contract_path_token(&mut paths, &token, indexed_files);
    }
    if let Some(section) = likely_files_section(intent) {
        for token in path_tokens_from_text(section, true) {
            add_contract_path_token(&mut paths, &token, indexed_files);
        }
    }
    paths
}

fn add_contract_path_token(
    paths: &mut std::collections::BTreeSet<String>,
    token: &str,
    indexed_files: &[anchor::storage::PathEntry],
) {
    if indexed_files.iter().any(|file| file.path == token) {
        paths.insert(token.to_string());
        return;
    }
    if token.contains('/') {
        let suffix = format!("/{token}");
        let mut matched = false;
        for file in indexed_files {
            if file.path.ends_with(&suffix) {
                paths.insert(file.path.clone());
                matched = true;
            }
        }
        if matched {
            return;
        }
    }
    paths.insert(token.to_string());
}

fn likely_files_section(intent: &str) -> Option<&str> {
    let lower = intent.to_ascii_lowercase();
    let start = lower
        .find("likely_files")
        .or_else(|| lower.find("likely files"))?;
    let rest = &intent[start..];
    let rest_lower = rest.to_ascii_lowercase();
    let end = [
        "verification",
        "search_terms",
        "search terms",
        "required_edges",
        "required edges",
        "quality_constraints",
        "quality constraints",
        "non_goals",
        "non goals",
    ]
    .into_iter()
    .filter_map(|marker| rest_lower.find(marker).filter(|idx| *idx > 0))
    .min()
    .unwrap_or(rest.len());
    Some(&rest[..end])
}

fn path_tokens_from_text(text: &str, allow_root_file: bool) -> std::collections::BTreeSet<String> {
    let mut paths = std::collections::BTreeSet::new();
    for raw in text.split(|ch: char| ch.is_whitespace() || ch == ',' || ch == ';') {
        let token = raw.trim_matches(|ch: char| {
            matches!(
                ch,
                '`' | '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ':' | '.'
            )
        });
        if looks_like_contract_path(token, allow_root_file) {
            paths.insert(token.replace('\\', "/"));
        }
    }
    paths
}

fn looks_like_contract_path(token: &str, allow_root_file: bool) -> bool {
    if token.contains("://") || token.starts_with('-') || token.is_empty() {
        return false;
    }
    if !allow_root_file && !token.contains('/') {
        return false;
    }
    is_source_path_token(token)
}

fn is_source_path_token(token: &str) -> bool {
    matches!(
        Path::new(token).extension().and_then(|value| value.to_str()),
        Some(
            "c" | "cc"
                | "cpp"
                | "cs"
                | "go"
                | "h"
                | "hpp"
                | "java"
                | "js"
                | "jsx"
                | "kt"
                | "mjs"
                | "py"
                | "rs"
                | "scala"
                | "swift"
                | "ts"
                | "tsx"
        )
    )
}
