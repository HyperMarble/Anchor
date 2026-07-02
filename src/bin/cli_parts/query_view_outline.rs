struct AroundCandidate {
    score: i32,
    lines: usize,
    entry: SymbolEntry,
    source_hash: String,
    slice_hash: String,
    code: String,
}

fn view_path_outline(
    store: &AnchorStore,
    path: &str,
    source: &str,
    source_hash: &str,
) -> Result<String> {
    let mut symbols = store
        .load_symbol_index()?
        .symbols
        .into_iter()
        .filter(|symbol| symbol.path == path)
        .collect::<Vec<_>>();
    symbols.sort_by(|left, right| {
        left.line_start
            .cmp(&right.line_start)
            .then_with(|| left.line_end.cmp(&right.line_end))
            .then_with(|| left.name.cmp(&right.name))
    });

    let mut out = String::new();
    out.push_str(&format!("outline: {path}\nsource_hash: {source_hash}\n"));
    out.push_str("\nprelude:\n");
    out.push_str(&source_prelude_preview(source));
    out.push('\n');
    if symbols.is_empty() {
        out.push_str("  (no indexed symbols)\n");
    }
    for (idx, symbol) in symbols.iter().enumerate() {
        if idx >= DEFAULT_CONTEXT_LINE_BUDGET {
            out.push_str(&format!(
                "  ... [outline truncated at {DEFAULT_CONTEXT_LINE_BUDGET} symbols; refine query or view a chunk]\n"
            ));
            break;
        }
        let role = if is_context_owner_symbol(symbol) {
            "owner"
        } else {
            "symbol"
        };
        out.push_str(&format!(
            "  {} {} {} lines:{}-{} hash:{} role:{}\n",
            chunk_handle(
                &symbol.path,
                &symbol.name,
                symbol.line_start,
                symbol.line_end
            ),
            symbol.kind,
            symbol.name,
            symbol.line_start,
            symbol.line_end,
            symbol.slice_hash,
            role
        ));
    }
    out.push_str("\nnext:\n");
    out.push_str("  anchor view file:<path>             # outline plus imports/prelude\n");
    out.push_str("  anchor view <chunk-handle>         # read one owner body\n");
    out.push_str("  anchor read <chunk-handle> --around \"text\"  # enclosing owner block\n");
    out.push_str(&format!(
        "  anchor read file:{path} --around \"text\"  # resolve text to an owner chunk\n"
    ));
    Ok(out)
}

fn source_prelude_preview(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let end = prelude_end_line(&lines).min(DEFAULT_CONTEXT_LINE_BUDGET);
    if end == 0 {
        return "  (empty file)\n".to_string();
    }
    let mut out = String::new();
    for (idx, line) in lines.iter().take(end).enumerate() {
        out.push_str(&format!(" {:>3}: {line}\n", idx + 1));
    }
    if end < lines.len() {
        out.push_str("    ... [prelude only; use chunk handles or --around for more]\n");
    }
    out
}

fn prelude_end_line(lines: &[&str]) -> usize {
    let mut last = 0;
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if idx == 0 && trimmed.starts_with("#!") {
            last = idx + 1;
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            if last == idx {
                last = idx + 1;
            }
            continue;
        }
        if looks_like_prelude_line(trimmed) {
            last = idx + 1;
            continue;
        }
        if last > 0 {
            break;
        }
        last = idx + 1;
        if idx >= 12 {
            break;
        }
    }
    last.max(1)
}

fn looks_like_prelude_line(trimmed: &str) -> bool {
    trimmed.starts_with("import ")
        || trimmed.starts_with("from ")
        || trimmed.starts_with("use ")
        || trimmed.starts_with("mod ")
        || trimmed.starts_with("pub mod ")
        || trimmed.starts_with("package ")
        || trimmed.starts_with("using ")
        || trimmed.starts_with("const ")
        || trimmed.starts_with("export ")
        || trimmed.starts_with("\"use ")
}

fn view_related_chunk_around(
    _root: &Path,
    store: &AnchorStore,
    original: &QueryHandle,
    term: &str,
    full: bool,
) -> Result<Option<ViewReport>> {
    let QueryHandle::Chunk {
        path,
        symbol,
        line_start,
        line_end,
    } = original
    else {
        return Ok(None);
    };

    let mut best: Option<AroundCandidate> = None;
    let mut entries = store
        .load_symbol_index()?
        .symbols
        .into_iter()
        .filter(|entry| entry.path == *path && is_context_owner_symbol(entry))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.line_start
            .cmp(&right.line_start)
            .then_with(|| left.line_end.cmp(&right.line_end))
            .then_with(|| left.name.cmp(&right.name))
    });

    for entry in entries {
        if entry.name == *symbol && entry.line_start == *line_start && entry.line_end == *line_end {
            continue;
        }
        let Ok(projection) = store.create_projection(&entry) else {
            continue;
        };
        let Ok(code) = view_around_text(&projection.text, entry.line_start, term, full) else {
            continue;
        };
        let score = around_candidate_score(&entry, &projection.text, term);
        let lines = entry.line_end.saturating_sub(entry.line_start) + 1;
        let candidate = AroundCandidate {
            score,
            lines,
            entry,
            source_hash: projection.source_hash,
            slice_hash: projection.slice_hash,
            code,
        };
        if is_better_around_candidate(&candidate, best.as_ref()) {
            best = Some(candidate);
        }
    }

    let Some(candidate) = best else {
        return Ok(None);
    };
    let handle = chunk_handle(
        &candidate.entry.path,
        &candidate.entry.name,
        candidate.entry.line_start,
        candidate.entry.line_end,
    );
    record_context_read(store, &candidate.entry, "resolved_around", Some(handle.clone()));
    Ok(Some(ViewReport {
        schema: "anchor.view.v1",
        handle,
        kind: "chunk".to_string(),
        path: candidate.entry.path,
        symbol: Some(candidate.entry.name),
        line_start: candidate.entry.line_start,
        line_end: candidate.entry.line_end,
        source_hash: candidate.source_hash,
        slice_hash: candidate.slice_hash,
        refreshed: false,
        code: candidate.code,
    }))
}

fn is_better_around_candidate(
    candidate: &AroundCandidate,
    current: Option<&AroundCandidate>,
) -> bool {
    let Some(current) = current else {
        return true;
    };
    candidate
        .score
        .cmp(&current.score)
        .then_with(|| current.lines.cmp(&candidate.lines))
        .then_with(|| current.entry.line_start.cmp(&candidate.entry.line_start))
        .is_gt()
}

fn around_candidate_score(symbol: &SymbolEntry, text: &str, term: &str) -> i32 {
    let lower_term = term.to_ascii_lowercase();
    let lower_name = symbol.name.to_ascii_lowercase();
    let header = text.lines().next().unwrap_or("").to_ascii_lowercase();
    let mut score = 0;
    if header.contains(&lower_term) {
        score += 300;
    }
    if lower_term.contains(&lower_name) {
        score += 180;
    }
    if lower_name.contains(&lower_term) {
        score += 120;
    }
    if matches!(symbol.kind.as_str(), "Function" | "Method") {
        score += 70;
    } else if is_class_like_symbol(symbol) {
        score += 50;
    }
    score - symbol.line_end.saturating_sub(symbol.line_start).min(200) as i32
}
