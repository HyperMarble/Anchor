fn view_path_around_owner(
    store: &AnchorStore,
    requested_handle: &str,
    kind: &str,
    path: &str,
    source_hash: &str,
    term: &str,
    full: bool,
) -> Result<Option<ViewReport>> {
    let mut candidates = store
        .load_symbol_index()?
        .symbols
        .into_iter()
        .filter(|entry| entry.path == path && is_context_owner_symbol(entry))
        .filter_map(|entry| {
            let projection = store.create_projection(&entry).ok()?;
            let priority = owner_around_priority(&entry, &projection.text, term)?;
            Some((priority, entry, projection))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| owner_span(&left.1).cmp(&owner_span(&right.1)))
            .then_with(|| left.1.line_start.cmp(&right.1.line_start))
    });

    let Some((_priority, entry, projection)) = candidates.into_iter().next() else {
        return Ok(None);
    };

    let handle = chunk_handle(&entry.path, &entry.name, entry.line_start, entry.line_end);
    let code = match view_around_text(&projection.text, entry.line_start, term, full) {
        Ok(block) => block,
        Err(_) => view_numbered_text(&projection.text, entry.line_start, full),
    };
    record_context_read(
        store,
        &entry,
        "resolved_file_around",
        Some(handle.clone()),
    );
    record_view_event(
        store,
        kind,
        path,
        Some(&entry.name),
        source_hash,
        &projection.slice_hash,
        "resolved_file_around",
    );
    Ok(Some(ViewReport {
        schema: "anchor.view.v1",
        handle: if requested_handle.starts_with("file:") || requested_handle.starts_with("test:")
        {
            handle
        } else {
            requested_handle.to_string()
        },
        kind: "chunk".to_string(),
        path: entry.path,
        symbol: Some(entry.name),
        line_start: entry.line_start,
        line_end: entry.line_end,
        source_hash: projection.source_hash,
        slice_hash: projection.slice_hash,
        refreshed: false,
        code,
    }))
}

fn owner_around_priority(symbol: &SymbolEntry, text: &str, term: &str) -> Option<u8> {
    let term_lower = term.to_ascii_lowercase();
    let name_lower = symbol.name.to_ascii_lowercase();
    let text_lower = text.to_ascii_lowercase();
    let header_lower = text.lines().next().unwrap_or("").to_ascii_lowercase();
    if header_lower.contains(&term_lower) {
        return Some(0);
    }
    if name_lower.contains(&term_lower) || term_lower.contains(&name_lower) {
        return Some(1);
    }
    if text_lower.contains(&term_lower) {
        return Some(2);
    }
    None
}

fn owner_span(symbol: &SymbolEntry) -> usize {
    symbol.line_end.saturating_sub(symbol.line_start)
}
