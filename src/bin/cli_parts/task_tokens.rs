fn tokenize_intent(intent: &str) -> impl Iterator<Item = String> + '_ {
    intent
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| part.to_ascii_lowercase())
}

fn task_intent_tokens(intent: &str) -> std::collections::BTreeSet<String> {
    const STOPWORDS: &[&str] = &[
        "add",
        "adds",
        "change",
        "changes",
        "create",
        "creates",
        "delete",
        "deletes",
        "fix",
        "fixes",
        "handle",
        "handles",
        "implement",
        "implements",
        "make",
        "makes",
        "patch",
        "patches",
        "remove",
        "removes",
        "support",
        "supports",
        "update",
        "updates",
        "work",
        "works",
    ];

    let mut tokens = std::collections::BTreeSet::new();
    for token in tokenize_intent(intent).filter(|token| token.len() >= 3) {
        if STOPWORDS.contains(&token.as_str()) {
            continue;
        }
        tokens.insert(token.clone());
        for part in split_camel_token(&token) {
            if part.len() >= 3 && !STOPWORDS.contains(&part.as_str()) {
                tokens.insert(part);
            }
        }
        if let Some(stripped) = token.strip_suffix("ing") {
            if stripped.len() >= 3 {
                tokens.insert(stripped.to_string());
            }
        }
        if let Some(stripped) = token.strip_suffix("ed") {
            if stripped.len() >= 3 {
                tokens.insert(stripped.to_string());
            }
        }
        if let Some(stripped) = token.strip_suffix('s') {
            if stripped.len() >= 3 {
                tokens.insert(stripped.to_string());
            }
        }
        if token == "lifecycle" {
            tokens.extend(
                [
                    "close", "enter", "entry", "exit", "init", "open", "run", "setup", "start",
                    "stop", "teardown",
                ]
                .into_iter()
                .map(String::from),
            );
        }
    }
    tokens
}

