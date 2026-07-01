
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
    let indexed_files = files
        .iter()
        .filter_map(|path| path.strip_prefix(root).ok())
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect::<Vec<_>>();

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
    let project_profile = build_project_profile(root, &indexed_files, symbol_index.symbols.len())?;
    store.save_project_profile(&project_profile)?;

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

fn build_project_profile(
    root: &Path,
    indexed_files: &[String],
    indexed_symbols: usize,
) -> Result<ProjectProfile> {
    use std::collections::BTreeMap;
    use std::fs;

    let mut language_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut dir_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut fingerprint = String::new();

    for file in indexed_files {
        if let Some(language) = profile_language_name(file) {
            *language_counts.entry(language).or_default() += 1;
        }
        if let Some((first, _)) = file.split_once('/') {
            *dir_counts.entry(first.to_string()).or_default() += 1;
        }
        let bytes = fs::read(root.join(file))?;
        fingerprint.push_str(file);
        fingerprint.push('\n');
        fingerprint.push_str(&content_hash(&bytes));
        fingerprint.push('\n');
    }

    let (frameworks_present, frameworks_absent) = detect_frameworks(root);

    Ok(ProjectProfile {
        schema: "anchor.project_profile.v1".to_string(),
        source_hash: content_hash(fingerprint.as_bytes()),
        languages: sorted_counts(language_counts),
        manifests: existing_paths(
            root,
            &[
                "Cargo.toml",
                "Cargo.lock",
                "lockd/go.mod",
                "lockd/go.sum",
                "package.json",
                "pyproject.toml",
                "requirements.txt",
                "pytest.ini",
                "go.mod",
            ],
        ),
        key_files: existing_paths(
            root,
            &[
                "README.md",
                "Cargo.toml",
                "package.json",
                "docs/prompt-repair.md",
                "src/bin/cli.rs",
                "src/storage/anchor.rs",
                "src/storage/bm25.rs",
                "src/cache.rs",
            ],
        ),
        top_dirs: sorted_counts(dir_counts),
        test_commands: detect_test_commands(root),
        indexed_files: indexed_files.to_vec(),
        frameworks_present,
        frameworks_absent,
        entrypoints: existing_paths(
            root,
            &[
                "src/bin/cli.rs",
                "main.go",
                "lockd/main.go",
                "manage.py",
                "package.json",
            ],
        ),
        indexed_symbols,
    })
}

fn detect_test_commands(root: &Path) -> Vec<String> {
    let mut commands = Vec::new();
    if root.join("Cargo.toml").exists() {
        commands.push("cargo test".to_string());
        commands.push("cargo build --release".to_string());
    }
    if root.join("lockd/go.mod").exists() {
        commands.push("cd lockd && go test ./...".to_string());
    } else if root.join("go.mod").exists() {
        commands.push("go test ./...".to_string());
    }
    if root.join("package.json").exists() {
        commands.push("npm test".to_string());
    }
    if root.join("pyproject.toml").exists() || root.join("pytest.ini").exists() {
        commands.push("pytest".to_string());
    }
    if root.join("docs/install.sh").exists() && root.join("docs/uninstall.sh").exists() {
        commands.push("bash -n docs/install.sh docs/uninstall.sh local_install.sh".to_string());
    }
    commands
}

fn detect_frameworks(root: &Path) -> (Vec<String>, Vec<String>) {
    let known = ["express", "jest", "react", "next"];
    let package_text = std::fs::read_to_string(root.join("package.json"))
        .unwrap_or_default()
        .to_lowercase();
    let next_config_present =
        root.join("next.config.js").exists() || root.join("next.config.ts").exists();
    let mut present = Vec::new();

    for framework in known {
        let found = match framework {
            "next" => next_config_present || package_text.contains("\"next\""),
            _ => package_text.contains(&format!("\"{framework}\"")),
        };
        if found {
            present.push(framework.to_string());
        }
    }

    let absent = known
        .iter()
        .filter(|framework| !present.iter().any(|item| item == *framework))
        .map(|framework| (*framework).to_string())
        .collect();

    (present, absent)
}

fn existing_paths(root: &Path, candidates: &[&str]) -> Vec<String> {
    candidates
        .iter()
        .filter(|path| root.join(path).exists())
        .map(|path| (*path).to_string())
        .collect()
}

fn profile_language_name(path: &str) -> Option<&'static str> {
    match Path::new(path).extension().and_then(|ext| ext.to_str()) {
        Some("rs") => Some("Rust"),
        Some("py") | Some("pyw") => Some("Python"),
        Some("js") | Some("mjs") | Some("cjs") => Some("JavaScript"),
        Some("ts") | Some("mts") | Some("cts") => Some("TypeScript"),
        Some("tsx") | Some("jsx") => Some("TSX"),
        Some("go") => Some("Go"),
        Some("java") => Some("Java"),
        Some("cs") => Some("C#"),
        Some("rb") => Some("Ruby"),
        Some("cpp") | Some("cc") | Some("cxx") | Some("hpp") | Some("h") => Some("C++"),
        Some("swift") => Some("Swift"),
        Some("md") | Some("mdx") => Some("Markdown"),
        Some("toml") => Some("TOML"),
        Some("json") => Some("JSON"),
        Some("yaml") | Some("yml") => Some("YAML"),
        Some("csv") => Some("CSV"),
        _ => None,
    }
}

fn sorted_counts<T>(counts: std::collections::BTreeMap<T, usize>) -> Vec<T>
where
    T: Ord,
{
    let mut items = counts.into_iter().collect::<Vec<_>>();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    items.into_iter().map(|(item, _)| item).take(8).collect()
}
