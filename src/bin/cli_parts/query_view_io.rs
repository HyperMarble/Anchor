fn view_path(
    root: &Path,
    store: &AnchorStore,
    handle: &str,
    kind: &str,
    path: &str,
    around: Option<&str>,
    full: bool,
) -> Result<ViewReport> {
    let source_path = checked_repo_file(root, path)?;
    let source = std::fs::read_to_string(&source_path)?;
    let source_hash = content_hash(source.as_bytes());
    let code = if let Some(term) = around {
        view_around_text(&source, 1, term, full)?
    } else {
        view_numbered_text(&source, 1, full)
    };
    let slice_hash = content_hash(code.as_bytes());
    record_view_event(store, kind, path, None, &source_hash, &slice_hash, "ok");
    Ok(ViewReport {
        schema: "anchor.view.v1",
        handle: handle.to_string(),
        kind: kind.to_string(),
        path: path.to_string(),
        symbol: None,
        line_start: 1,
        line_end: line_count(&source),
        source_hash,
        slice_hash,
        refreshed: false,
        code,
    })
}

fn view_chunk(
    root: &Path,
    store: &AnchorStore,
    handle: &str,
    chunk: &QueryHandle,
    around: Option<&str>,
    full: bool,
) -> Result<ViewReport> {
    let QueryHandle::Chunk {
        path,
        symbol,
        line_start,
        line_end,
    } = chunk
    else {
        bail!("view_chunk requires a chunk handle");
    };
    let (symbol_entry, refreshed) =
        resolve_view_symbol(root, store, path, symbol, *line_start, *line_end)?;
    let projection = store.create_projection(&symbol_entry)?;
    let code = if let Some(term) = around {
        view_around_text(&projection.text, symbol_entry.line_start, term, full)?
    } else {
        view_numbered_text(&projection.text, symbol_entry.line_start, full)
    };
    record_context_read(
        store,
        &symbol_entry,
        if refreshed { "refreshed" } else { "ok" },
        Some(handle.to_string()),
    );
    Ok(ViewReport {
        schema: "anchor.view.v1",
        handle: handle.to_string(),
        kind: "chunk".to_string(),
        path: symbol_entry.path,
        symbol: Some(symbol_entry.name),
        line_start: symbol_entry.line_start,
        line_end: symbol_entry.line_end,
        source_hash: projection.source_hash,
        slice_hash: projection.slice_hash,
        refreshed,
        code,
    })
}

fn resolve_view_symbol(
    root: &Path,
    store: &AnchorStore,
    path: &str,
    symbol: &str,
    line_start: usize,
    line_end: usize,
) -> Result<(SymbolEntry, bool)> {
    let source_path = checked_repo_file(root, path)?;
    let source = std::fs::read(&source_path)?;
    let current_hash = content_hash(&source);
    let mut refreshed = false;
    let mut index = store.load_symbol_index()?;
    let stale = index
        .symbols
        .iter()
        .filter(|entry| entry.path == path)
        .any(|entry| entry.source_hash != current_hash);
    if stale || !index.symbols.iter().any(|entry| entry.path == path) {
        let _ = store.upsert_symbols_for_path(&source_path)?;
        index = store.load_symbol_index()?;
        refreshed = true;
    }
    if let Some(entry) = index.symbols.iter().find(|entry| {
        entry.path == path
            && entry.name == symbol
            && entry.line_start == line_start
            && entry.line_end == line_end
    }) {
        return Ok((entry.clone(), refreshed));
    }
    let same_symbol: Vec<_> = index
        .symbols
        .iter()
        .filter(|entry| entry.path == path && entry.name == symbol)
        .cloned()
        .collect();
    if same_symbol.len() == 1 {
        return Ok((same_symbol[0].clone(), true));
    }
    bail!("stale or ambiguous chunk handle: {path}#{symbol}@{line_start}-{line_end}")
}

fn checked_repo_file(root: &Path, path: &str) -> Result<PathBuf> {
    let relative = Path::new(path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        bail!("handle path must be repo-relative: {path}");
    }
    let full = root.join(relative);
    if !full.is_file() {
        bail!("handle path is not a file: {path}");
    }
    Ok(full)
}

fn view_numbered_text(text: &str, start_line: usize, full: bool) -> String {
    let mut out = String::new();
    for (idx, line) in text.lines().enumerate() {
        if !full && idx >= DEFAULT_CONTEXT_LINE_BUDGET {
            out.push_str(&format!(
                "    ... [context truncated at {DEFAULT_CONTEXT_LINE_BUDGET} lines]\n"
            ));
            break;
        }
        out.push_str(&format!(" {:>3}: {line}\n", start_line + idx));
    }
    out
}

fn view_around_text(text: &str, start_line: usize, term: &str, full: bool) -> Result<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut keep = std::collections::BTreeSet::new();
    for (idx, line) in lines.iter().enumerate() {
        if !line.contains(term) {
            continue;
        }
        let first = idx.saturating_sub(3);
        let last = (idx + 3).min(lines.len().saturating_sub(1));
        for item in first..=last {
            keep.insert(item);
        }
    }
    if keep.is_empty() {
        bail!("around text not found in handle view: {term}");
    }
    let mut out = String::new();
    let mut previous = None;
    for (shown, idx) in keep.into_iter().enumerate() {
        if !full && shown >= DEFAULT_CONTEXT_LINE_BUDGET {
            out.push_str(&format!(
                "    ... [context truncated at {DEFAULT_CONTEXT_LINE_BUDGET} lines]\n"
            ));
            break;
        }
        if previous.map(|prev| idx > prev + 1).unwrap_or(false) {
            out.push_str("    ...\n");
        }
        out.push_str(&format!(" {:>3}: {}\n", start_line + idx, lines[idx]));
        previous = Some(idx);
    }
    Ok(out)
}

fn record_view_event(
    store: &AnchorStore,
    kind: &str,
    path: &str,
    symbol: Option<&str>,
    source_hash: &str,
    slice_hash: &str,
    status: &str,
) {
    let mut meta = std::collections::BTreeMap::new();
    meta.insert("kind".to_string(), kind.to_string());
    meta.insert("source_hash".to_string(), source_hash.to_string());
    meta.insert("slice_hash".to_string(), slice_hash.to_string());
    events::record_with_meta(
        store.anchor_root(),
        "view.read",
        Some(path.to_string()),
        symbol.map(ToString::to_string),
        status,
        None,
        meta,
    );
}
