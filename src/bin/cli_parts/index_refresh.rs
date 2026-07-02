enum IndexRefresh {
    Clean,
    Incremental { refreshed: usize },
}

fn refresh_stale_index_paths(root: &Path, store: &AnchorStore) -> Result<IndexRefresh> {
    refresh_index_paths(root, store, true)
}

fn refresh_index_paths(root: &Path, store: &AnchorStore, discover_new: bool) -> Result<IndexRefresh> {
    let path_index = store.load_path_index()?;
    let indexed_paths: std::collections::BTreeSet<String> =
        path_index.files.iter().map(|entry| entry.path.clone()).collect();
    let mut refreshed = 0usize;
    for entry in path_index.files {
        let path = root.join(&entry.path);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => {
                remove_indexed_path(store, &entry.path)?;
                refreshed += 1;
                continue;
            }
        };
        if content_hash(&bytes) != entry.source_hash {
            refresh_indexed_source_path(root, store, &path)?;
            refreshed += 1;
        }
    }
    if !discover_new {
        return if refreshed == 0 {
            Ok(IndexRefresh::Clean)
        } else {
            Ok(IndexRefresh::Incremental { refreshed })
        };
    }
    for entry in Walk::new(root).filter_map(|entry| entry.ok()) {
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let path = entry.path();
        if !is_indexable_text_path(path) {
            continue;
        }
        let Some(relative) = repo_relative_string(root, path) else {
            continue;
        };
        if indexed_paths.contains(&relative) || is_task_ignored_path(&relative) {
            continue;
        }
        if refresh_indexed_source_path(root, store, path).is_ok() {
            refreshed += 1;
        }
    }
    if refreshed == 0 {
        Ok(IndexRefresh::Clean)
    } else {
        Ok(IndexRefresh::Incremental { refreshed })
    }
}

fn refresh_indexed_source_path(root: &Path, store: &AnchorStore, path: &Path) -> Result<()> {
    let relative = repo_relative_string(root, path)
        .ok_or_else(|| anyhow::anyhow!("path is outside repo: {}", path.display()))?;
    let old_symbols = store
        .load_symbol_index()?
        .symbols
        .into_iter()
        .filter(|symbol| symbol.path == relative)
        .collect::<Vec<_>>();
    let (_, symbols, _) = store.upsert_symbols_for_path(path)?;
    let mut call_index = store.load_call_index();
    for symbol in old_symbols {
        call_index.calls.remove(&symbol.name);
    }
    if !symbols.is_empty() {
        let source = std::fs::read_to_string(path)?;
        let extraction = anchor::parser::extract_file(path, &source)?;
        for (caller, callee) in extracted_calls(&extraction) {
            let callees = call_index.calls.entry(caller).or_default();
            if !callees.contains(&callee) {
                callees.push(callee);
            }
        }
    }
    for callees in call_index.calls.values_mut() {
        callees.sort();
        callees.dedup();
    }
    store.save_call_index(&call_index)?;
    Ok(())
}

fn remove_indexed_path(store: &AnchorStore, path: &str) -> Result<()> {
    let mut path_index = store.load_path_index()?;
    path_index.files.retain(|entry| entry.path != path);
    store.save_path_index(&path_index)?;

    let mut symbol_index = store.load_symbol_index()?;
    let removed_symbols = symbol_index
        .symbols
        .iter()
        .filter(|symbol| symbol.path == path)
        .map(|symbol| symbol.name.clone())
        .collect::<Vec<_>>();
    symbol_index.symbols.retain(|symbol| symbol.path != path);
    store.save_symbol_index(&symbol_index)?;

    let mut call_index = store.load_call_index();
    for symbol in removed_symbols {
        call_index.calls.remove(&symbol);
    }
    store.save_call_index(&call_index)?;
    Ok(())
}
