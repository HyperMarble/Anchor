struct ExpectedHash {
    value: Option<String>,
    source: &'static str,
}

fn expected_hash_from_recent_read(
    root: &Path,
    path: &Path,
    requested: &str,
    provided_hash: Option<&str>,
) -> ExpectedHash {
    if let Some(provided_hash) = provided_hash {
        return ExpectedHash {
            value: Some(provided_hash.to_string()),
            source: "provided",
        };
    }

    let Some(store) = AnchorStore::discover(root)
        .or_else(|_| AnchorStore::init(root))
        .ok()
    else {
        return ExpectedHash {
            value: None,
            source: "none",
        };
    };
    let repo_path = lock_path(root, path, requested);
    let session_id = std::env::var("ANCHOR_SESSION_ID").unwrap_or_else(|_| "local".into());
    let agent_id = lockd::agent_id().to_string();
    let value = events::last_read_hash(store.anchor_root(), &session_id, &agent_id, &repo_path);

    if value.is_some() {
        ExpectedHash {
            value,
            source: "context",
        }
    } else {
        ExpectedHash {
            value: None,
            source: "none",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChangeSummary {
    start_line: usize,
    old_end_line: usize,
    new_end_line: usize,
    old_changed_lines: usize,
    new_changed_lines: usize,
}

#[derive(Clone, Copy)]
struct WriteStats {
    lines: usize,
    bytes: usize,
    replacements: Option<usize>,
}

#[derive(Clone, Copy)]
struct WriteReceipt<'a> {
    before_hash: Option<&'a str>,
    after_hash: Option<&'a str>,
    change: Option<&'a ChangeSummary>,
    stats: WriteStats,
}

fn line_change_summary(before: Option<&str>, after: &str) -> ChangeSummary {
    let before_lines: Vec<&str> = before.unwrap_or("").lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();

    let mut prefix = 0;
    while prefix < before_lines.len()
        && prefix < after_lines.len()
        && before_lines[prefix] == after_lines[prefix]
    {
        prefix += 1;
    }

    let mut suffix = 0;
    while suffix < before_lines.len().saturating_sub(prefix)
        && suffix < after_lines.len().saturating_sub(prefix)
        && before_lines[before_lines.len() - 1 - suffix]
            == after_lines[after_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let old_end_line = before_lines.len().saturating_sub(suffix);
    let new_end_line = after_lines.len().saturating_sub(suffix);
    let old_changed_lines = old_end_line.saturating_sub(prefix);
    let new_changed_lines = new_end_line.saturating_sub(prefix);

    ChangeSummary {
        start_line: prefix + 1,
        old_end_line,
        new_end_line,
        old_changed_lines,
        new_changed_lines,
    }
}

/// Strict-mode read requirement: an existing source file may only be changed
/// by a session that has actually read it (a recorded `context.read`).
fn enforce_read_requirement(
    root: &Path,
    path: &Path,
    requested: &str,
    expected_hash: &ExpectedHash,
    event_kind: &str,
    event_symbol: Option<&str>,
) -> Result<()> {
    if !strict_mode() || expected_hash.value.is_some() || !path.exists() || !is_source_path(path) {
        return Ok(());
    }
    let repo_path = lock_path(root, path, requested);
    if let Ok(store) = AnchorStore::discover(root).or_else(|_| AnchorStore::init(root)) {
        events::record(
            store.anchor_root(),
            event_kind,
            Some(repo_path.clone()),
            event_symbol.map(str::to_string),
            "blocked",
            Some("strict mode: no recorded read for this session".to_string()),
        );
    }
    println!("<result>");
    println!("<path>{}</path>", repo_path);
    println!("<status>read_required</status>");
    println!(
        "<message>strict mode: read this file through anchor context before editing it</message>"
    );
    println!("</result>");
    anyhow::bail!("strict mode: no recorded read for {}", repo_path)
}

/// Provenance is load-bearing: the attempt is recorded *before* the file is
/// touched, so an unwritable event log refuses the mutation instead of
/// producing an unrecorded change.
fn record_write_attempt(
    root: &Path,
    path: &Path,
    requested: &str,
    operation: &str,
    symbol: Option<&str>,
) -> Result<()> {
    let store = AnchorStore::discover(root).or_else(|_| AnchorStore::init(root))?;
    events::record_required(
        store.anchor_root(),
        "write.attempt",
        Some(lock_path(root, path, requested)),
        symbol.map(str::to_string),
        "ok",
        Some(format!("operation={operation}")),
    )
}

fn verify_expected_hash(
    root: &Path,
    path: &Path,
    requested: &str,
    before_hash: Option<&str>,
    expected_hash: Option<&str>,
    event_kind: &str,
    event_symbol: Option<&str>,
) -> Result<()> {
    let Some(expected_hash) = expected_hash else {
        return Ok(());
    };
    let actual_hash = before_hash.unwrap_or("missing");
    if actual_hash == expected_hash {
        return Ok(());
    }

    if let Ok(store) = AnchorStore::discover(root).or_else(|_| AnchorStore::init(root)) {
        events::record(
            store.anchor_root(),
            event_kind,
            Some(lock_path(root, path, requested)),
            event_symbol.map(str::to_string),
            "blocked",
            Some(format!(
                "stale_file expected={} actual={}",
                expected_hash, actual_hash
            )),
        );
    }

    println!("<result>");
    println!("<path>{}</path>", lock_path(root, path, requested));
    println!("<status>stale_file</status>");
    println!("<expected_hash>{}</expected_hash>", expected_hash);
    println!("<actual_hash>{}</actual_hash>", actual_hash);
    println!("</result>");

    anyhow::bail!(
        "stale file: expected hash {}, actual {}",
        expected_hash,
        actual_hash
    )
}

fn write_event_meta(
    expected_hash: &ExpectedHash,
    content_hash: Option<String>,
    receipt: WriteReceipt<'_>,
) -> std::collections::BTreeMap<String, String> {
    let mut meta = std::collections::BTreeMap::new();
    meta.insert(
        "before_hash".to_string(),
        receipt.before_hash.unwrap_or("missing").to_string(),
    );
    meta.insert(
        "after_hash".to_string(),
        receipt.after_hash.unwrap_or("missing").to_string(),
    );
    meta.insert(
        "expected_hash_source".to_string(),
        expected_hash.source.to_string(),
    );
    if let Some(expected_hash) = &expected_hash.value {
        meta.insert("expected_hash".to_string(), expected_hash.clone());
    }
    if let Some(content_hash) = content_hash {
        meta.insert("content_hash".to_string(), content_hash);
    }
    if let Some(change) = receipt.change {
        meta.insert(
            "changed_start_line".to_string(),
            change.start_line.to_string(),
        );
        meta.insert("old_end_line".to_string(), change.old_end_line.to_string());
        meta.insert("new_end_line".to_string(), change.new_end_line.to_string());
        meta.insert(
            "old_changed_lines".to_string(),
            change.old_changed_lines.to_string(),
        );
        meta.insert(
            "new_changed_lines".to_string(),
            change.new_changed_lines.to_string(),
        );
    }
    meta.insert("lines".to_string(), receipt.stats.lines.to_string());
    meta.insert("bytes".to_string(), receipt.stats.bytes.to_string());
    if let Some(replacements) = receipt.stats.replacements {
        meta.insert("replacements".to_string(), replacements.to_string());
    }
    meta
}

fn print_compact_write_receipt(status: &str, path: &str, receipt: WriteReceipt<'_>) {
    println!("<result>");
    println!("<path>{}</path>", path);
    println!("<status>{}</status>", status);
    if let Some(before_hash) = receipt.before_hash {
        println!("<before_hash>{}</before_hash>", before_hash);
    }
    if let Some(after_hash) = receipt.after_hash {
        println!("<after_hash>{}</after_hash>", after_hash);
    }
    if let Some(change) = receipt.change {
        println!(
            "<changed_range start=\"{}\" old_end=\"{}\" new_end=\"{}\" old_lines=\"{}\" new_lines=\"{}\"/>",
            change.start_line,
            change.old_end_line,
            change.new_end_line,
            change.old_changed_lines,
            change.new_changed_lines
        );
    }
    println!("<lines>{}</lines>", receipt.stats.lines);
    println!("<bytes>{}</bytes>", receipt.stats.bytes);
    if let Some(replacements) = receipt.stats.replacements {
        println!("<replacements>{}</replacements>", replacements);
    }
    println!("</result>");
}
