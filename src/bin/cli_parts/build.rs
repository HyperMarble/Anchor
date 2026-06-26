
// ── AnchorStore commands ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct BuildStats {
    indexed: usize,
    skipped: usize,
    sym_count: usize,
    call_count: usize,
    history_commits: usize,
    history_edges: usize,
}

fn cmd_build(root: &Path) -> Result<()> {
    let stats = build_indexes(root)?;
    print_build_stats(stats);
    Ok(())
}

fn build_indexes(root: &Path) -> Result<BuildStats> {
    use anchor::storage::content_hash;
    use anchor::storage::{CallIndex, PathEntry, PathIndex, SymbolEntry, SymbolIndex};
    use std::collections::HashMap;
    use std::fs;

    let store = AnchorStore::init(root)?;
    let files: Vec<PathBuf> = Walk::new(root)
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter(|e| is_indexable_text_path(e.path()))
        .map(|e| e.into_path())
        .collect();

    // Parse all files in parallel — read-only, no shared writes
    let results: Vec<_> = files
        .par_iter()
        .filter_map(|path| {
            let source = match fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("read fail: {}: {e}", path.display());
                    return None;
                }
            };
            let hash = content_hash(source.as_bytes());
            let relative = path
                .strip_prefix(root)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");

            // Try extracting symbols; skip unsupported files silently
            let extraction = match anchor::parser::extract_file(path, &source) {
                Ok(e) => e,
                Err(e) => {
                    if path.extension().map(|x| x == "rs").unwrap_or(false) {
                        eprintln!("extract fail: {}: {e}", path.display());
                    }
                    return None;
                }
            };
            if extraction.symbols.is_empty() {
                return None;
            }

            let path_entry = PathEntry {
                path: relative.clone(),
                source_hash: hash.clone(),
                bytes: source.len() as u64,
            };

            let symbols: Vec<SymbolEntry> = extraction
                .symbols
                .iter()
                .map(|s| SymbolEntry {
                    path: relative.clone(),
                    source_hash: hash.clone(),
                    name: s.name.clone(),
                    kind: format!("{:?}", s.kind),
                    line_start: s.line_start,
                    line_end: s.line_end,
                    slice_hash: content_hash(s.code_snippet.as_bytes()),
                    features: s.features.clone(),
                })
                .collect();

            // Build qualified name map: fn_name → Parent::fn_name (only for unambiguous names)
            let mut name_count: HashMap<String, usize> = HashMap::new();
            for s in &extraction.symbols {
                *name_count.entry(s.name.clone()).or_default() += 1;
            }
            let qualified: HashMap<String, String> = extraction
                .symbols
                .iter()
                .filter(|s| name_count[&s.name] == 1)
                .filter_map(|s| {
                    s.parent
                        .as_ref()
                        .map(|p| (s.name.clone(), format!("{}::{}", p, s.name)))
                })
                .collect();

            // Collect calls: qualify caller with parent when unambiguous
            let calls: Vec<(String, String)> = extraction
                .calls
                .iter()
                .map(|c| {
                    let caller = qualified
                        .get(&c.caller)
                        .cloned()
                        .unwrap_or_else(|| c.caller.clone());
                    (caller, c.callee.clone())
                })
                .collect();

            Some((path_entry, symbols, calls))
        })
        .collect();

    // Write indexes once — sequential, no races
    let mut path_index = PathIndex::default();
    let mut symbol_index = SymbolIndex::default();
    let mut call_map: HashMap<String, std::collections::HashSet<String>> = HashMap::new();

    for (path_entry, syms, calls) in &results {
        path_index.files.push(path_entry.clone());
        symbol_index.symbols.extend_from_slice(syms);
        for (caller, callee) in calls {
            call_map
                .entry(caller.clone())
                .or_default()
                .insert(callee.clone());
        }
    }

    path_index.files.sort_by(|a, b| a.path.cmp(&b.path));
    symbol_index.symbols.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| a.line_start.cmp(&b.line_start))
    });

    let call_index = CallIndex {
        calls: call_map
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().collect()))
            .collect(),
    };

    store.save_path_index(&path_index)?;
    store.save_symbol_index(&symbol_index)?;
    store.save_call_index(&call_index)?;
    let history_index = build_history_index(root);
    store.save_history_index(&history_index)?;

    let indexed = results.len();
    let skipped = files.len() - indexed;
    let sym_count = symbol_index.symbols.len();
    let call_count = call_index.calls.values().map(|v| v.len()).sum::<usize>();
    let history_commits = history_index.commits_scanned;
    let history_edges = history_index.cochanges.len();

    Ok(BuildStats {
        indexed,
        skipped,
        sym_count,
        call_count,
        history_commits,
        history_edges,
    })
}

