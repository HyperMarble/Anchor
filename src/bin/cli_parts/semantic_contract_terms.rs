fn contract_terms(intent: &str) -> std::collections::BTreeSet<String> {
    let mut terms = std::collections::BTreeSet::new();
    let mut current = String::new();
    for ch in intent.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            current.push(ch.to_ascii_lowercase());
        } else {
            push_contract_term(&mut terms, &current);
            current.clear();
        }
    }
    push_contract_term(&mut terms, &current);
    terms
}

fn contract_search_terms(intent: &str) -> std::collections::BTreeSet<String> {
    let lower = intent.to_ascii_lowercase();
    let Some(start) = lower.find("search_terms") else {
        return std::collections::BTreeSet::new();
    };
    let rest = &intent[start + "search_terms".len()..];
    let rest_lower = rest.to_ascii_lowercase();
    let end = [
        "required_edges",
        "expected_behavior",
        "verification_requirements",
        "quality_constraints",
        "non_goals",
        "likely_files",
    ]
    .into_iter()
    .filter_map(|marker| rest_lower.find(marker))
    .min()
    .unwrap_or(rest.len());
    contract_owner_terms(&rest[..end])
}

fn contract_owner_terms(intent: &str) -> std::collections::BTreeSet<String> {
    let mut terms = std::collections::BTreeSet::new();
    let mut current = String::new();
    for ch in intent.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            current.push(ch.to_ascii_lowercase());
        } else {
            push_contract_owner_term(&mut terms, &current);
            current.clear();
        }
    }
    push_contract_owner_term(&mut terms, &current);
    terms
}

fn push_contract_owner_term(terms: &mut std::collections::BTreeSet<String>, term: &str) {
    let term = term.trim_matches('_');
    if term.len() < 3 {
        return;
    }
    terms.insert(term.to_string());
    for part in split_camel_token(term) {
        if part.len() >= 3 {
            terms.insert(part);
        }
    }
}

fn contract_term_order(
    terms: &std::collections::BTreeSet<String>,
) -> std::collections::BTreeMap<String, usize> {
    terms
        .iter()
        .enumerate()
        .map(|(idx, term)| (term.clone(), idx))
        .collect()
}

fn contract_symbol_order(
    symbol: &str,
    order: &std::collections::BTreeMap<String, usize>,
) -> usize {
    order
        .get(&symbol.to_ascii_lowercase())
        .copied()
        .unwrap_or(usize::MAX)
}

fn exact_contract_symbol(symbol: &str, terms: &std::collections::BTreeSet<String>) -> bool {
    terms.contains(&symbol.to_ascii_lowercase())
}

fn push_contract_term(terms: &mut std::collections::BTreeSet<String>, term: &str) {
    let term = term.trim_matches('_');
    if term.len() < 3 {
        return;
    }
    terms.insert(term.to_string());
    for part in split_camel_token(term) {
        if part.len() >= 3 {
            terms.insert(part);
        }
    }
    for part in term.split('_') {
        if part.len() >= 3 {
            terms.insert(part.to_string());
        }
    }
}
