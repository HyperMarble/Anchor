
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

const PRODUCT_MEMORY_SCHEMA: &str = "anchor.product_memory.v1";

fn cmd_build(root: &Path) -> Result<()> {
    let stats = build_indexes(root)?;
    print_build_stats(stats);
    Ok(())
}

fn build_indexes(root: &Path) -> Result<BuildStats> {
    use anchor::storage::content_hash;
    use anchor::storage::{
        CallIndex, PathEntry, PathIndex, ProductMemory, ProductMemoryFile, SymbolEntry,
        SymbolIndex,
    };
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
    store.save_product_memory(&build_product_memory(root)?)?;

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

fn build_product_memory(root: &Path) -> Result<ProductMemory> {
    let mut instruction_files = Vec::new();

    for (path, kind, note) in [
        (
            "AGENTS.md",
            "agent_rules",
            "Repo-local agent instructions for coding sessions.",
        ),
        (
            "CLAUDE.md",
            "agent_rules",
            "Repo-local Claude guidance that prompt repair should preserve.",
        ),
        (
            "GEMINI.md",
            "agent_rules",
            "Repo-local Gemini guidance that prompt repair should preserve.",
        ),
        (
            ".github/copilot-instructions.md",
            "copilot_rules",
            "GitHub Copilot repository instructions.",
        ),
        (
            ".clinerules",
            "cline_rules",
            "Cline repository rules for agent behavior.",
        ),
        (
            ".cursorrules",
            "cursor_rules",
            "Cursor repository rules for agent behavior.",
        ),
        (
            ".windsurfrules",
            "windsurf_rules",
            "Windsurf repository rules for agent behavior.",
        ),
    ] {
        add_instruction_file(root, path, kind, note, &mut instruction_files)?;
    }

    collect_instruction_dir(
        root,
        ".cursor/rules",
        "cursor_rule",
        "Cursor rule file that prompt repair should preserve.",
        &mut instruction_files,
    )?;
    collect_instruction_dir(
        root,
        ".continue/rules",
        "continue_rule",
        "Continue rule file that prompt repair should preserve.",
        &mut instruction_files,
    )?;
    collect_instruction_dir(
        root,
        ".continue/prompts",
        "continue_prompt",
        "Continue prompt file that may shape repo-local agent behavior.",
        &mut instruction_files,
    )?;

    instruction_files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(ProductMemory {
        schema: PRODUCT_MEMORY_SCHEMA.to_string(),
        instruction_files,
    })
}

fn add_instruction_file(
    root: &Path,
    relative: &str,
    kind: &str,
    note: &str,
    out: &mut Vec<ProductMemoryFile>,
) -> Result<()> {
    use std::fs;

    let path = root.join(relative);
    if !path.is_file() {
        return Ok(());
    }

    let bytes = fs::read(&path)?;
    out.push(ProductMemoryFile {
        path: relative.replace('\\', "/"),
        kind: kind.to_string(),
        note: note.to_string(),
        source_hash: content_hash(&bytes),
    });
    Ok(())
}

fn collect_instruction_dir(
    root: &Path,
    relative_dir: &str,
    kind: &str,
    note: &str,
    out: &mut Vec<ProductMemoryFile>,
) -> Result<()> {
    use std::fs;

    let dir = root.join(relative_dir);
    if !dir.is_dir() {
        return Ok(());
    }

    let mut stack = vec![dir];
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                stack.push(path);
                continue;
            }
            if !file_type.is_file() || !looks_like_instruction_text(&path) {
                continue;
            }

            let relative = path
                .strip_prefix(root)
                .map(|value| value.to_string_lossy().replace('\\', "/"))?;
            let bytes = fs::read(&path)?;
            out.push(ProductMemoryFile {
                path: relative,
                kind: kind.to_string(),
                note: note.to_string(),
                source_hash: content_hash(&bytes),
            });
        }
    }

    Ok(())
}

fn looks_like_instruction_text(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("md" | "mdc" | "txt" | "json" | "yaml" | "yml")
    )
}
