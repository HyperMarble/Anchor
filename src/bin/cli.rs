//
//  cli.rs
//  Anchor
//
//  Created by hak (tharun)
//

use anchor::cache::PersistentCache;
use anchor::cli::{self, protect as cli_protect, write as cli_write, Cli, Commands};
use anchor::events;
use anchor::lock::lockd;
use anchor::parser::language::is_indexable_text_path;
use anchor::query::slice::slice_code;
use anchor::storage::{
    content_hash, AnchorStore, CallIndex, HistoryIndex, PathIndex, SymbolEntry, SymbolIndex,
};
use anyhow::{bail, Result};
use clap::Parser;
use ignore::Walk;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing_subscriber::EnvFilter;

const TASK_WORKSPACE_SCHEMA: &str = "anchor.task_workspace.v1";
const TASK_WORKSPACE_CURRENT: &str = "current.json";

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let roots: Vec<_> = cli
        .root
        .into_iter()
        .map(|r| r.canonicalize().unwrap_or(r))
        .collect();
    let root = roots[0].clone();
    lockd::set_workspace(&root);

    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            cli::print_usage();
            return Ok(());
        }
    };

    match command {
        Commands::Build => cmd_build(&root),

        Commands::Task {
            intent,
            limit,
            context_limit,
        } => cmd_task(&root, &intent, limit, context_limit),

        Commands::Context {
            queries,
            limit,
            full,
            bundle,
        } => cmd_context(&root, &queries, limit, full, bundle),

        Commands::Search {
            queries,
            pattern,
            limit,
        } => cmd_search(&root, &queries, pattern.as_deref(), limit),

        Commands::Map { scope } => cmd_map(&root, scope.as_deref()),

        Commands::Write {
            path,
            content,
            expect_hash,
        } => cli_write::create(&root, &path, &content, expect_hash.as_deref()),

        Commands::Edit {
            path,
            action,
            pattern,
            symbol,
            content,
            expect_hash,
        } => {
            if let Some(symbol) = symbol {
                let content = content
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("symbol edit requires --content"))?;
                return cli_write::replace_symbol(
                    &root,
                    &path,
                    &symbol,
                    content,
                    expect_hash.as_deref(),
                );
            }

            let action = action.ok_or_else(|| anyhow::anyhow!("edit requires --action"))?;
            let pattern = pattern.ok_or_else(|| anyhow::anyhow!("edit requires --pattern"))?;
            match action.as_str() {
                "insert" => cli_write::insert(
                    &root,
                    &path,
                    &pattern,
                    content.as_deref().unwrap_or(""),
                    expect_hash.as_deref(),
                ),
                "replace" => cli_write::replace(
                    &root,
                    &path,
                    &pattern,
                    content.as_deref().unwrap_or(""),
                    expect_hash.as_deref(),
                ),
                "delete" => cli_write::replace(&root, &path, &pattern, "", expect_hash.as_deref()),
                other => bail!("unknown edit action: {}", other),
            }
        }

        Commands::Protect { action } => cli_protect::run(&root, &action),

        Commands::Status => cmd_status(&root),

        Commands::Trace { limit } => cmd_trace(&root, limit),

        Commands::Receipt => cmd_receipt(&root),

        Commands::Gate { min_score, require_receipts } => cmd_gate(&root, min_score, require_receipts),

        Commands::Check { command } => cmd_check(&root, &command),

        Commands::Run { command } => cmd_run(&root, &command),
    }
}

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

fn build_history_index(root: &Path) -> anchor::storage::HistoryIndex {
    use anchor::storage::{CoChangeEntry, HistoryIndex, HistoryNeighbor, PathHistoryEntry};
    use std::collections::{BTreeMap, BTreeSet};

    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("log")
        .arg("--name-only")
        .arg("--pretty=format:__ANCHOR_COMMIT__%H")
        .arg("-n")
        .arg("200")
        .output();

    let Ok(output) = output else {
        return HistoryIndex {
            schema: "anchor.history_index.v2".to_string(),
            ..HistoryIndex::default()
        };
    };
    if !output.status.success() {
        return HistoryIndex {
            schema: "anchor.history_index.v2".to_string(),
            ..HistoryIndex::default()
        };
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut commits: Vec<Vec<String>> = Vec::new();
    let mut current: BTreeSet<String> = BTreeSet::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("__ANCHOR_COMMIT__") {
            if !current.is_empty() {
                commits.push(current.into_iter().collect());
                current = BTreeSet::new();
            }
            continue;
        }
        let path = line.replace('\\', "/");
        if is_git_history_path_indexable(&path) {
            current.insert(path);
        }
    }
    if !current.is_empty() {
        commits.push(current.into_iter().collect());
    }

    let mut path_stats: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut pair_stats: BTreeMap<(String, String), (usize, usize)> = BTreeMap::new();

    for (commit_idx, files) in commits.iter().enumerate() {
        if files.len() > 40 {
            continue;
        }
        let recency_score = commits.len().saturating_sub(commit_idx).max(1);
        for path in files {
            let stats = path_stats.entry(path.clone()).or_default();
            stats.0 += 1;
            stats.1 += recency_score;
        }
        for i in 0..files.len() {
            for j in i + 1..files.len() {
                let a = files[i].clone();
                let b = files[j].clone();
                let ab = pair_stats.entry((a.clone(), b.clone())).or_default();
                ab.0 += 1;
                ab.1 += recency_score;
                let ba = pair_stats.entry((b, a)).or_default();
                ba.0 += 1;
                ba.1 += recency_score;
            }
        }
    }

    let mut paths: Vec<PathHistoryEntry> = path_stats
        .into_iter()
        .map(|(path, (commits, score))| PathHistoryEntry {
            is_test: looks_like_test_path(&path),
            path,
            commits,
            score,
        })
        .collect();
    paths.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| b.commits.cmp(&a.commits))
            .then_with(|| a.path.cmp(&b.path))
    });

    let mut cochanges: Vec<CoChangeEntry> = pair_stats
        .into_iter()
        .filter(|(_, (commits, _))| *commits >= 1)
        .map(|((path, related_path), (commits, score))| CoChangeEntry {
            path,
            related_path,
            commits,
            score,
        })
        .collect();
    cochanges.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| b.commits.cmp(&a.commits))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.related_path.cmp(&b.related_path))
    });
    let mut adjacency: BTreeMap<String, Vec<HistoryNeighbor>> = BTreeMap::new();
    for edge in &cochanges {
        adjacency
            .entry(edge.path.clone())
            .or_default()
            .push(HistoryNeighbor {
                related_path: edge.related_path.clone(),
                commits: edge.commits,
                score: edge.score,
                is_test: looks_like_test_path(&edge.related_path),
            });
    }
    for neighbors in adjacency.values_mut() {
        neighbors.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| b.commits.cmp(&a.commits))
                .then_with(|| a.related_path.cmp(&b.related_path))
        });
        neighbors.truncate(HISTORY_NEIGHBOR_LIMIT);
    }
    cochanges.truncate(2_000);

    HistoryIndex {
        schema: "anchor.history_index.v2".to_string(),
        commits_scanned: commits.len(),
        cochanges,
        adjacency,
        paths,
    }
}

const HISTORY_NEIGHBOR_LIMIT: usize = 24;

fn print_build_stats(stats: BuildStats) {
    let BuildStats {
        indexed,
        skipped,
        sym_count,
        call_count,
        history_commits,
        history_edges,
    } = stats;
    println!(
        "<build>\n<files>{indexed}</files>\n<symbols>{sym_count}</symbols>\n<calls>{call_count}</calls>\n<history_commits>{history_commits}</history_commits>\n<history_edges>{history_edges}</history_edges>\n<skipped_files>{skipped}</skipped_files>\n</build>"
    );
}

fn open_store(root: &Path) -> Result<AnchorStore> {
    AnchorStore::discover(root)
        .or_else(|_| AnchorStore::init(root))
        .map_err(|e| anyhow::anyhow!(e))
}

fn ensure_indexed_store(root: &Path) -> Result<AnchorStore> {
    let store = open_store(root)?;
    let needs_build = !store.path_index_path().exists()
        || !store.symbol_index_path().exists()
        || !store.call_index_path().exists()
        || !store.history_index_path().exists()
        || store
            .load_symbol_index()
            .map(|index| index.symbols.is_empty())
            .unwrap_or(true);
    let needs_build = if needs_build {
        true
    } else {
        index_has_stale_paths(root, &store)?
    };

    if needs_build {
        let stats = build_indexes(root)?;
        events::record(
            store.anchor_root(),
            "index.build",
            None,
            None,
            "ok",
            Some(format!(
                "auto indexed={} symbols={} calls={}",
                stats.indexed, stats.sym_count, stats.call_count
            )),
        );
        eprintln!(
            "[anchor] auto-built index: files={} symbols={} calls={}",
            stats.indexed, stats.sym_count, stats.call_count
        );
    }

    open_store(root)
}

fn index_has_stale_paths(root: &Path, store: &AnchorStore) -> Result<bool> {
    let path_index = store.load_path_index()?;
    for entry in path_index.files {
        let path = root.join(&entry.path);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(true),
        };
        if content_hash(&bytes) != entry.source_hash {
            return Ok(true);
        }
    }
    Ok(false)
}

#[derive(Debug)]
struct TaskIndexState {
    store: AnchorStore,
    symbol_index: SymbolIndex,
    call_index: CallIndex,
    path_index: PathIndex,
    history_index: HistoryIndex,
    scoped_files: usize,
}

#[derive(Debug, Default)]
struct TaskFileCandidates {
    source_paths: Vec<String>,
    test_paths: Vec<String>,
}

fn prepare_task_index_state(
    root: &Path,
    task_tokens: &std::collections::BTreeSet<String>,
    limit: usize,
) -> Result<TaskIndexState> {
    let store = open_store(root)?;
    let mut history_index = store.load_history_index();
    if history_index.schema.is_empty() && !store.history_index_path().exists() {
        history_index = build_history_index(root);
        store.save_history_index(&history_index)?;
    }

    let candidates = task_file_candidates(root, task_tokens, &history_index, limit)?;
    let scoped_files = refresh_task_scoped_indexes(root, &store, &candidates)?;

    Ok(TaskIndexState {
        symbol_index: store.load_symbol_index()?,
        call_index: store.load_call_index(),
        path_index: store.load_path_index()?,
        history_index,
        store,
        scoped_files,
    })
}

fn task_file_candidates(
    root: &Path,
    task_tokens: &std::collections::BTreeSet<String>,
    history_index: &HistoryIndex,
    limit: usize,
) -> Result<TaskFileCandidates> {
    use std::collections::BTreeMap;
    use std::io::Read;

    const TASK_SOURCE_SCAN_BYTES: usize = 48 * 1024;
    const TASK_SOURCE_SCAN_TOTAL_BYTES: usize = 16 * 1024 * 1024;

    let source_limit = limit.clamp(8, 24);
    let test_limit = limit.saturating_mul(2).clamp(8, 24);
    let history_scores: BTreeMap<&str, usize> = history_index
        .paths
        .iter()
        .map(|entry| (entry.path.as_str(), entry.score.max(entry.commits)))
        .collect();
    let mut remaining_scan_bytes = TASK_SOURCE_SCAN_TOTAL_BYTES;
    let mut sources: BTreeMap<String, usize> = BTreeMap::new();
    let mut tests: BTreeMap<String, usize> = BTreeMap::new();
    let mut scan_queue: Vec<(String, PathBuf, usize)> = Vec::new();

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
        if is_task_ignored_path(&relative) {
            continue;
        }

        let mut score = task_file_path_score(&relative, task_tokens);
        score += history_scores
            .get(relative.as_str())
            .copied()
            .unwrap_or_default()
            .min(500);

        if looks_like_test_path(&relative) {
            if score > 0 {
                tests.insert(relative, score);
            }
            continue;
        }
        if !anchor::parser::language::is_source_path(path) {
            continue;
        }

        scan_queue.push((relative, path.to_path_buf(), score));
    }

    // Path-scored files spend the content-scan budget first so unrelated
    // files in a large repo cannot starve out the likely ones, but files with
    // no path signal still get scanned with leftover budget: the task terms
    // may only appear in the code itself.
    scan_queue.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    for (relative, path, mut score) in scan_queue {
        if remaining_scan_bytes > 0 {
            if let Ok(metadata) = std::fs::metadata(&path) {
                let bytes_to_scan = (metadata.len() as usize)
                    .min(TASK_SOURCE_SCAN_BYTES)
                    .min(remaining_scan_bytes);
                if bytes_to_scan > 0 {
                    if let Ok(file) = std::fs::File::open(&path) {
                        let mut bytes = Vec::new();
                        let mut limited = file.take(bytes_to_scan as u64);
                        let _ = limited.read_to_end(&mut bytes);
                        let prefix = String::from_utf8_lossy(&bytes);
                        score += task_source_rank(&prefix, task_tokens).max(0) as usize * 2;
                        remaining_scan_bytes = remaining_scan_bytes.saturating_sub(bytes_to_scan);
                    }
                }
            }
        }

        if score > 0 {
            sources.insert(relative, score);
        }
    }

    let source_paths: Vec<String> = top_scored_owned_paths(&sources, source_limit)
        .into_iter()
        .map(|(path, _)| path)
        .collect();

    for source_path in &source_paths {
        for (test_path, score) in tests.iter_mut() {
            *score += source_test_affinity_score(source_path, test_path);
        }
        if let Some(neighbors) = history_index.adjacency.get(source_path) {
            for neighbor in neighbors {
                if neighbor.is_test {
                    let score = tests.entry(neighbor.related_path.clone()).or_default();
                    *score += neighbor.score.max(neighbor.commits).min(500);
                }
            }
        }
    }

    let test_paths: Vec<String> = top_scored_owned_paths(&tests, test_limit)
        .into_iter()
        .map(|(path, _)| path)
        .collect();

    Ok(TaskFileCandidates {
        source_paths,
        test_paths,
    })
}

fn refresh_task_scoped_indexes(
    root: &Path,
    store: &AnchorStore,
    candidates: &TaskFileCandidates,
) -> Result<usize> {
    const TASK_PARSE_FILE_BYTES_MAX: u64 = 512 * 1024;

    let mut call_index = store.load_call_index();
    let mut scoped_files = 0usize;

    for relative in &candidates.source_paths {
        let source_path = root.join(relative);
        if std::fs::metadata(&source_path)
            .map(|metadata| metadata.len() > TASK_PARSE_FILE_BYTES_MAX)
            .unwrap_or(false)
        {
            continue;
        }
        let old_symbols = store
            .load_symbol_index()?
            .symbols
            .into_iter()
            .filter(|symbol| symbol.path == *relative)
            .collect::<Vec<_>>();
        for symbol in old_symbols {
            call_index.calls.remove(&symbol.name);
        }

        let (_, symbols, _) = match store.upsert_symbols_for_path(&source_path) {
            Ok(result) => result,
            Err(_) => continue,
        };
        if symbols.is_empty() {
            continue;
        }
        if let Ok(source) = std::fs::read_to_string(&source_path) {
            if let Ok(extraction) = anchor::parser::extract_file(&source_path, &source) {
                for (caller, callee) in extracted_calls(&extraction) {
                    let callees = call_index.calls.entry(caller).or_default();
                    if !callees.contains(&callee) {
                        callees.push(callee);
                    }
                }
            }
        }
        scoped_files += 1;
    }

    for relative in &candidates.test_paths {
        let test_path = root.join(relative);
        if store.upsert_path(&test_path).is_ok() {
            scoped_files += 1;
        }
    }

    for callees in call_index.calls.values_mut() {
        callees.sort();
        callees.dedup();
    }
    store.save_call_index(&call_index)?;

    Ok(scoped_files)
}

fn extracted_calls(extraction: &anchor::parser::FileExtractions) -> Vec<(String, String)> {
    use std::collections::HashMap;

    let mut name_count: HashMap<String, usize> = HashMap::new();
    for symbol in &extraction.symbols {
        *name_count.entry(symbol.name.clone()).or_default() += 1;
    }
    let qualified: HashMap<String, String> = extraction
        .symbols
        .iter()
        .filter(|symbol| name_count[&symbol.name] == 1)
        .filter_map(|symbol| {
            symbol
                .parent
                .as_ref()
                .map(|parent| (symbol.name.clone(), format!("{}::{}", parent, symbol.name)))
        })
        .collect();

    extraction
        .calls
        .iter()
        .map(|call| {
            let caller = qualified
                .get(&call.caller)
                .cloned()
                .unwrap_or_else(|| call.caller.clone());
            (caller, call.callee.clone())
        })
        .collect()
}

fn repo_relative_string(root: &Path, path: &Path) -> Option<String> {
    Some(
        path.strip_prefix(root)
            .ok()?
            .to_string_lossy()
            .replace('\\', "/"),
    )
}

fn is_task_ignored_path(path: &str) -> bool {
    path.starts_with(".anchor/")
        || path.starts_with(".git/")
        || path.contains("/.anchor/")
        || path.contains("/.git/")
        || path.contains("/node_modules/")
        || path.contains("/target/")
        || path.contains("/dist/")
        || path.contains("/build/")
        || path.contains("/vendor/")
        || path.contains("/__pycache__/")
}

fn task_file_path_score(path: &str, tokens: &std::collections::BTreeSet<String>) -> usize {
    let lower_path = path.to_ascii_lowercase();
    let path_tokens = path_signal_tokens(path);
    let mut score = 0usize;
    let mut matched_terms = 0usize;

    for token in tokens {
        let mut matched = false;
        if lower_path.contains(token) {
            score += 80;
            matched = true;
        }
        if path_tokens
            .iter()
            .any(|path_token| soft_token_match(token, path_token))
        {
            score += 120;
            matched = true;
        }
        if matched {
            matched_terms += 1;
        }
    }

    if matched_terms >= 2 {
        score += matched_terms.min(8) * matched_terms.min(8) * 20;
    }
    if looks_like_test_path(path) {
        score += 30;
    }

    score
}

fn top_scored_owned_paths(
    scores: &std::collections::BTreeMap<String, usize>,
    limit: usize,
) -> Vec<(String, usize)> {
    let mut items: Vec<_> = scores
        .iter()
        .map(|(path, score)| (path.clone(), *score))
        .collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    items.truncate(limit);
    items
}

fn is_git_history_path_indexable(path: &str) -> bool {
    if path.starts_with(".anchor/")
        || path.starts_with(".git/")
        || path.contains("/__pycache__/")
        || path.ends_with(".pyc")
        || path.ends_with(".pyo")
    {
        return false;
    }
    let path_obj = Path::new(path);
    is_indexable_text_path(path_obj)
}

fn looks_like_test_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.contains("/tests/")
        || lower.starts_with("tests/")
        || lower.contains("/test/")
        || lower.starts_with("test/")
        || lower.contains("_test.")
        || lower.contains("test_")
        || lower.ends_with(".spec.ts")
        || lower.ends_with(".test.ts")
        || lower.ends_with(".spec.tsx")
        || lower.ends_with(".test.tsx")
        || lower.ends_with(".spec.js")
        || lower.ends_with(".test.js")
        || lower.ends_with(".spec.jsx")
        || lower.ends_with(".test.jsx")
}

fn record_context_read(
    store: &AnchorStore,
    sym: &SymbolEntry,
    status: &str,
    message: Option<String>,
) {
    let mut meta = std::collections::BTreeMap::new();
    meta.insert("source_hash".to_string(), sym.source_hash.clone());
    meta.insert("slice_hash".to_string(), sym.slice_hash.clone());
    events::record_with_meta(
        store.anchor_root(),
        "context.read",
        Some(sym.path.clone()),
        Some(sym.name.clone()),
        status,
        message,
        meta,
    );
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskWorkspace {
    schema: String,
    intent: String,
    active_paths: Vec<TaskPath>,
    exact_slices: Vec<TaskSlice>,
    related_files: Vec<TaskRelatedFile>,
    likely_tests: Vec<TaskTest>,
    verification_plan: TaskVerificationPlan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskPath {
    path: String,
    source_hash: String,
    score: i32,
    role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskSlice {
    path: String,
    source_hash: String,
    symbol: String,
    kind: String,
    line_start: usize,
    line_end: usize,
    score: i32,
    reasons: Vec<String>,
    code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskTest {
    path: String,
    score: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskRelatedFile {
    path: String,
    score: usize,
    reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskVerificationPlan {
    steps: Vec<String>,
    preferred_check: Option<String>,
    check_hints: Vec<TaskCheckHint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskCheckHint {
    kind: String,
    command: String,
}

fn task_workspace_path(store: &AnchorStore) -> PathBuf {
    store
        .anchor_root()
        .join("tasks")
        .join(TASK_WORKSPACE_CURRENT)
}

fn save_task_workspace(store: &AnchorStore, workspace: &TaskWorkspace) -> Result<()> {
    let path = task_workspace_path(store);
    std::fs::create_dir_all(path.parent().ok_or_else(|| {
        anyhow::anyhow!("task workspace path has no parent: {}", path.display())
    })?)?;
    std::fs::write(path, serde_json::to_vec_pretty(workspace)?)?;
    Ok(())
}

fn cmd_search(root: &Path, queries: &[String], _pattern: Option<&str>, limit: usize) -> Result<()> {
    let store = ensure_indexed_store(root)?;
    let query = queries.join(" ");
    let results = store.search_symbols_hybrid(&query, limit)?;

    println!("<results query=\"{}\" count=\"{}\">", query, results.len());
    for sym in &results {
        println!(
            "  <symbol name=\"{}\" kind=\"{}\" file=\"{}\" line=\"{}\"/>",
            sym.name, sym.kind, sym.path, sym.line_start
        );
    }
    println!("</results>");
    Ok(())
}

fn cmd_context(
    root: &Path,
    queries: &[String],
    limit: usize,
    full: bool,
    bundle: bool,
) -> Result<()> {
    use std::collections::HashSet;

    let store = ensure_indexed_store(root)?;
    let call_index = store.load_call_index();
    let mut persistent_cache = PersistentCache::load(store.anchor_root());

    // track what we've printed to avoid duplicate bundle entries
    let mut shown: HashSet<String> = HashSet::new();
    let mut bundled_callees: Vec<String> = Vec::new();

    for (i, query) in queries.iter().enumerate() {
        if i > 0 {
            println!("===");
        }
        let candidates = store.search_symbols_hybrid(query, limit)?;
        println!(
            "<results query=\"{}\" count=\"{}\">",
            query,
            candidates.len()
        );

        for sym in &candidates {
            shown.insert(sym.name.clone());

            if persistent_cache.is_hit(&sym.name, &sym.path, &sym.slice_hash) {
                record_context_read(&store, sym, "cached", None);
                println!("<symbol cached=\"true\">");
                println!("<name>{}</name>", sym.name);
                println!("<kind>{}</kind>", sym.kind);
                println!("<file>{}</file>", sym.path);
                println!("<line>{}</line>", sym.line_start);
                println!("<file_hash>{}</file_hash>", sym.source_hash);
                println!("<cache>CACHED</cache>");
                println!("</symbol>");
                continue;
            }
            persistent_cache.update(&sym.name, &sym.path, &sym.slice_hash);

            let proj = match store.create_projection(sym) {
                Ok(p) => p,
                Err(e) => {
                    events::record(
                        store.anchor_root(),
                        "context.read",
                        Some(sym.path.clone()),
                        Some(sym.name.clone()),
                        "error",
                        Some(e.to_string()),
                    );
                    continue;
                }
            };
            let call_lines = store.call_lines_for_symbol(sym);
            let sliced = if full {
                anchor::query::slice::SliceResult {
                    code: proj.text.clone(),
                    total_lines: proj.text.lines().count(),
                    shown_lines: proj.text.lines().count(),
                    call_count: call_lines.len(),
                    was_sliced: false,
                }
            } else {
                slice_code(&proj.text, &call_lines, sym.line_start)
            };
            let callers = call_index.callers_of(&sym.name);
            let callees = call_index.callees_of(&sym.name);

            println!("<symbol>");
            println!("<name>{}</name>", sym.name);
            println!("<kind>{}</kind>", sym.kind);
            println!("<file>{}</file>", sym.path);
            println!("<line>{}</line>", sym.line_start);
            println!("<file_hash>{}</file_hash>", sym.source_hash);
            if !callers.is_empty() {
                println!(
                    "<called_by>{}</called_by>",
                    callers
                        .iter()
                        .take(8)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            if !callees.is_empty() {
                println!(
                    "<calls>{}</calls>",
                    callees
                        .iter()
                        .take(8)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            println!("<code>");
            if sliced.was_sliced {
                println!(
                    "[{}/{} lines, {} calls]",
                    sliced.shown_lines, sliced.total_lines, sliced.call_count
                );
                // sliced.code already includes "{:>4}: {}\n" line numbers
                print_bounded_numbered_code(&sliced.code, full);
            } else {
                print_bounded_plain_code(&sliced.code, sym.line_start, full);
            }
            println!("</code>");
            println!("</symbol>");

            record_context_read(&store, sym, "ok", None);

            if bundle {
                for callee in callees.iter().take(8) {
                    bundled_callees.push(callee.to_string());
                }
            }
        }
        println!("</results>");
    }

    if bundle && !bundled_callees.is_empty() {
        let mut already_bundled: HashSet<String> = HashSet::new();
        let mut bundle_lines = 0usize;

        println!("--- BUNDLED ---");
        for callee in &bundled_callees {
            if !already_bundled.insert(callee.clone()) || shown.contains(callee) {
                continue;
            }
            // only bundle project-defined functions (have their own callees in our index)
            if call_index.callees_of(callee).is_empty() {
                continue;
            }
            const SKIP: &[&str] = &[
                "new",
                "from",
                "into",
                "clone",
                "default",
                "collect",
                "iter",
                "map",
                "filter",
                "unwrap",
                "expect",
                "to_string",
                "as_str",
                "as_ref",
                "drop",
            ];
            if SKIP.contains(&callee.as_str()) {
                continue;
            }
            let neighbors = match store.search_symbols_hybrid(callee, 2) {
                Ok(v) => v,
                Err(_) => continue,
            };
            for sym in &neighbors {
                if sym.name != *callee {
                    continue;
                }
                shown.insert(sym.name.clone());
                let proj = match store.create_projection(sym) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let call_lines = store.call_lines_for_symbol(sym);
                let sliced = slice_code(&proj.text, &call_lines, sym.line_start);
                println!("{} {} {}:{}", sym.name, sym.kind, sym.path, sym.line_start);
                println!("  file_hash: {}", sym.source_hash);
                if sliced.was_sliced {
                    println!("  [{}/{} lines]", sliced.shown_lines, sliced.total_lines);
                    print_bounded_numbered_code(&sliced.code, false);
                } else {
                    print_bounded_plain_code(&sliced.code, sym.line_start, false);
                }
                record_context_read(&store, sym, "ok", Some("bundle".to_string()));
                bundle_lines += sliced.shown_lines;
                println!();
            }
        }
        eprintln!("[bundle: {} lines]", bundle_lines);
    }

    persistent_cache.save(store.anchor_root());

    Ok(())
}

fn cmd_task(
    root: &Path,
    intent_parts: &[String],
    limit: usize,
    context_limit: usize,
) -> Result<()> {
    use anchor::parser::language::is_source_path;
    use std::collections::{BTreeMap, BTreeSet, HashSet};

    let intent = intent_parts.join(" ");
    if intent.trim().is_empty() {
        bail!("task requires an intent");
    }

    let task_tokens = task_intent_tokens(&intent);
    let task_state =
        prepare_task_index_state(root, &task_tokens, limit.max(context_limit).clamp(8, 16))?;
    let store = task_state.store;
    let symbol_index = task_state.symbol_index;
    let call_index = task_state.call_index;
    let path_index = task_state.path_index;
    let history_index = task_state.history_index;
    let task_query = if task_tokens.is_empty() {
        intent.clone()
    } else {
        task_tokens.iter().cloned().collect::<Vec<_>>().join(" ")
    };
    let candidate_pool_limit = limit.max(context_limit).saturating_mul(8).clamp(24, 96);
    let mut candidates = store.search_symbols_hybrid(&task_query, candidate_pool_limit)?;
    candidates.extend(source_backed_task_candidates(
        root,
        &symbol_index.symbols,
        &task_tokens,
        candidate_pool_limit,
    ));
    dedupe_symbols(&mut candidates);
    let task_source_scores: BTreeMap<String, i32> = candidates
        .iter()
        .map(|symbol| {
            let source_score = store
                .create_projection(symbol)
                .map(|projection| task_source_rank(&projection.text, &task_tokens))
                .unwrap_or_default();
            (task_symbol_key(symbol), source_score)
        })
        .collect();
    candidates.sort_by(|a, b| {
        task_symbol_total_rank(b, &task_tokens, &task_source_scores)
            .cmp(&task_symbol_total_rank(
                a,
                &task_tokens,
                &task_source_scores,
            ))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.line_start.cmp(&b.line_start))
            .then_with(|| a.name.cmp(&b.name))
    });
    candidates.truncate(limit.max(context_limit));
    let current_paths: HashSet<String> = path_index
        .files
        .iter()
        .map(|entry| entry.path.clone())
        .collect();

    let mut shown_paths = BTreeSet::new();
    let mut related_files = BTreeSet::new();
    let mut historical_files: BTreeMap<String, usize> = BTreeMap::new();
    let mut historical_tests: BTreeMap<String, usize> = BTreeMap::new();

    for sym in &candidates {
        related_files.insert(sym.path.clone());
        let callers = call_index.callers_of(&sym.name);
        let callees = call_index.callees_of(&sym.name);
        for neighbor in callers.into_iter().chain(callees.into_iter()).take(8) {
            for hit in store.search_symbols_hybrid(neighbor, 2).unwrap_or_default() {
                if is_source_path(Path::new(&hit.path)) {
                    related_files.insert(hit.path);
                }
            }
        }
    }

    let seed_paths: BTreeSet<String> = candidates
        .iter()
        .map(|sym| sym.path.clone())
        .chain(related_files.iter().cloned())
        .collect();
    let source_seed_paths: BTreeSet<String> = seed_paths
        .iter()
        .filter(|path| !looks_like_test_path(path))
        .cloned()
        .collect();
    if history_index.adjacency.is_empty() {
        for edge in &history_index.cochanges {
            if !seed_paths.contains(&edge.path) || !current_paths.contains(&edge.related_path) {
                continue;
            }
            let edge_score = edge.score.max(edge.commits);
            let score = historical_files
                .entry(edge.related_path.clone())
                .or_default();
            *score += edge_score;
            if looks_like_test_path(&edge.related_path) {
                add_path_score(&mut historical_tests, edge.related_path.clone(), edge_score);
            }
        }
    } else {
        for seed_path in &seed_paths {
            let Some(neighbors) = history_index.adjacency.get(seed_path) else {
                continue;
            };
            for neighbor in neighbors {
                if !current_paths.contains(&neighbor.related_path) {
                    continue;
                }
                let score = historical_files
                    .entry(neighbor.related_path.clone())
                    .or_default();
                *score += neighbor.score.max(neighbor.commits);
                if neighbor.is_test {
                    add_path_score(
                        &mut historical_tests,
                        neighbor.related_path.clone(),
                        neighbor.score.max(neighbor.commits),
                    );
                }
            }
        }
    }

    for path in historical_files.keys() {
        related_files.insert(path.clone());
    }
    let task_slices = rank_task_slices(
        &store,
        &symbol_index.symbols,
        &call_index,
        &task_tokens,
        &candidates,
        limit.max(context_limit).saturating_mul(2).clamp(8, 24),
    );
    let likely_tests_owned = rank_task_tests(
        &path_index.files,
        &task_tokens,
        &candidates,
        &task_slices,
        &source_seed_paths,
        &historical_tests,
        12,
    );
    let likely_tests: Vec<(&String, usize)> = likely_tests_owned
        .iter()
        .map(|test| (&test.path, test.score))
        .collect();
    let verification_plan = build_task_verification_plan(&likely_tests);
    let file_hashes: std::collections::BTreeMap<&str, &str> = path_index
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.source_hash.as_str()))
        .collect();
    let workspace = build_task_workspace(
        &intent,
        &task_slices,
        &related_files,
        &historical_files,
        &likely_tests,
        &verification_plan,
        &file_hashes,
    );
    save_task_workspace(&store, &workspace)?;

    events::record(
        store.anchor_root(),
        "task.intake",
        None,
        None,
        "ok",
        Some(format!(
            "intent={} scoped_files={} symbols={} context_symbols={} related_files={} tests={} historical_files={} historical_tests={}",
            intent,
            task_state.scoped_files,
            candidates.len(),
            context_limit.min(candidates.len()),
            related_files.len(),
            likely_tests_owned.len(),
            historical_files.len(),
            historical_tests.len()
        )),
    );

    println!("<task_intake>");
    println!("<intent>{}</intent>", escape_xml_text(&intent));
    println!("<strategy>");
    println!("  <step>Use this intake as the first context read.</step>");
    println!("  <step>Start inside task_workspace active_paths and exact_slices.</step>");
    println!(
        "  <step>Drill down with anchor context only when a needed symbol is missing from the workspace.</step>"
    );
    println!("  <step>Edit through anchor edit/write when possible; run verification through anchor check before handoff.</step>");
    println!("</strategy>");
    println!("<scoped_files>{}</scoped_files>", task_state.scoped_files);

    println!(
        "<ranked_symbols count=\"{}\" shown=\"{}\">",
        candidates.len(),
        context_limit.min(candidates.len())
    );
    for sym in &candidates {
        println!(
            "  <symbol name=\"{}\" kind=\"{}\" file=\"{}\" line=\"{}\"/>",
            escape_xml_text(&sym.name),
            escape_xml_text(&sym.kind),
            escape_xml_text(&sym.path),
            sym.line_start
        );
    }
    println!("</ranked_symbols>");

    print_task_workspace(&workspace, &task_workspace_path(&store));

    println!("<context>");
    let mut emitted = 0usize;
    for sym in candidates.iter().take(context_limit) {
        let Ok(proj) = store.create_projection(sym) else {
            continue;
        };
        emitted += 1;
        shown_paths.insert(sym.path.clone());
        let call_lines = store.call_lines_for_symbol(sym);
        let sliced = slice_code(&proj.text, &call_lines, sym.line_start);
        let callers = call_index.callers_of(&sym.name);
        let callees = call_index.callees_of(&sym.name);

        println!("<symbol>");
        println!("<name>{}</name>", escape_xml_text(&sym.name));
        println!("<kind>{}</kind>", escape_xml_text(&sym.kind));
        println!("<file>{}</file>", escape_xml_text(&sym.path));
        println!("<line>{}</line>", sym.line_start);
        println!("<file_hash>{}</file_hash>", sym.source_hash);
        if !callers.is_empty() {
            println!(
                "<called_by>{}</called_by>",
                escape_xml_text(
                    &callers
                        .iter()
                        .take(6)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            );
        }
        if !callees.is_empty() {
            println!(
                "<calls>{}</calls>",
                escape_xml_text(
                    &callees
                        .iter()
                        .take(6)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            );
        }
        if workspace_has_exact_slice(&workspace, sym) {
            println!(
                "<workspace_slice_ref file=\"{}\" symbol=\"{}\" line=\"{}\"/>",
                escape_xml_text(&sym.path),
                escape_xml_text(&sym.name),
                sym.line_start
            );
        } else {
            println!("<code>");
            if sliced.was_sliced {
                println!(
                    "[{}/{} lines, {} calls]",
                    sliced.shown_lines, sliced.total_lines, sliced.call_count
                );
                print_bounded_numbered_code(&sliced.code, false);
            } else {
                print_bounded_plain_code(&sliced.code, sym.line_start, false);
            }
            println!("</code>");
        }
        print_constructor_child_context(&store, &symbol_index.symbols, sym)?;
        println!("</symbol>");
        record_context_read(&store, sym, "ok", Some("task_intake".to_string()));
    }
    println!("</context>");

    println!("<related_files count=\"{}\">", related_files.len());
    for path in related_files.iter().take(20) {
        let marker = if shown_paths.contains(path) {
            " shown=\"true\""
        } else {
            ""
        };
        println!("  <file{}>{}</file>", marker, escape_xml_text(path));
    }
    println!("</related_files>");

    println!(
        "<historical_files commits_scanned=\"{}\" count=\"{}\">",
        history_index.commits_scanned,
        historical_files.len()
    );
    for (path, score) in top_scored_paths(&historical_files, 12) {
        println!(
            "  <file score=\"{}\">{}</file>",
            score,
            escape_xml_text(path)
        );
    }
    println!("</historical_files>");

    println!("<likely_tests count=\"{}\">", likely_tests_owned.len());
    for (path, score) in &likely_tests {
        println!(
            "  <file score=\"{}\">{}</file>",
            score,
            escape_xml_text(path)
        );
    }
    println!("</likely_tests>");
    println!("<historical_tests count=\"{}\">", historical_tests.len());
    for (path, score) in top_scored_paths(&historical_tests, 8) {
        println!(
            "  <file score=\"{}\">{}</file>",
            score,
            escape_xml_text(path)
        );
    }
    println!("</historical_tests>");
    println!("<context_symbols>{emitted}</context_symbols>");
    println!("</task_intake>");

    Ok(())
}

fn cmd_map(root: &Path, scope: Option<&str>) -> Result<()> {
    let store = ensure_indexed_store(root)?;
    let index = store.load_symbol_index()?;

    // Group by top-level directory
    use std::collections::BTreeMap;
    let mut modules: BTreeMap<String, Vec<&anchor::storage::SymbolEntry>> = BTreeMap::new();

    for sym in &index.symbols {
        let module = sym.path.split('/').next().unwrap_or("root").to_string();
        let entry = modules.entry(module).or_default();
        if scope.map(|s| sym.path.contains(s)).unwrap_or(true) {
            entry.push(sym);
        }
    }

    println!("<map>");
    for (module, syms) in &modules {
        if syms.is_empty() {
            continue;
        }
        let file_count = syms
            .iter()
            .map(|s| &s.path)
            .collect::<std::collections::HashSet<_>>()
            .len();
        println!(
            "  <module name=\"{module}\" files=\"{file_count}\" symbols=\"{}\">",
            syms.len()
        );
        // Top 5 symbols by name length (proxy for importance/complexity)
        let mut top: Vec<_> = syms.iter().take(5).collect();
        top.sort_by_key(|s| s.name.len());
        for sym in top.iter().rev() {
            println!(
                "    <symbol name=\"{}\" kind=\"{}\" file=\"{}\"/>",
                sym.name, sym.kind, sym.path
            );
        }
        println!("  </module>");
    }
    println!("</map>");
    Ok(())
}

fn execution_summary(root: &Path, events: &[events::ExecutionEvent]) -> events::EventSummary {
    events::EventSummary::from_events(events).with_unrecorded_repo_changes(git_changed_paths(root))
}

fn git_changed_paths(root: &Path) -> Vec<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("status")
        .arg("--porcelain")
        .arg("--untracked-files=all")
        .output();

    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let mut paths = std::collections::BTreeSet::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(path) = git_status_path(line).filter(|path| is_repo_audit_path(path)) {
            paths.insert(path);
        }
    }
    paths.into_iter().collect()
}

fn git_status_path(line: &str) -> Option<String> {
    if line.len() < 4 {
        return None;
    }
    let path = if line.starts_with("R ") || line.starts_with("RM") || line.starts_with("RD") {
        line.get(3..)?
            .rsplit_once(" -> ")
            .map(|(_, to)| to)
            .unwrap_or_else(|| line.get(3..).unwrap_or_default())
    } else {
        line.get(3..)?
    };
    let path = path.trim().trim_matches('"').replace('\\', "/");
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

fn is_repo_audit_path(path: &str) -> bool {
    let path = path.replace('\\', "/");
    if path.starts_with(".anchor/")
        || path.starts_with(".git/")
        || path.starts_with(".cache/")
        || path.starts_with(".mypy_cache/")
        || path.starts_with(".pytest_cache/")
        || path.starts_with(".ruff_cache/")
        || path.starts_with(".venv/")
        || path.contains("/__pycache__/")
        || path.ends_with(".pyc")
        || path.ends_with(".pyo")
    {
        return false;
    }
    true
}

fn cmd_status(root: &Path) -> Result<()> {
    let store = open_store(root)?;
    let events = events::load(store.anchor_root())?;
    let summary = execution_summary(root, &events);
    let quality = summary.quality_profile();

    println!("<status>");
    println!("<events>{}</events>", summary.event_count);
    println!("<context_reads>{}</context_reads>", summary.context_reads);
    println!("<cache_hits>{}</cache_hits>", summary.cache_hits);
    println!("<edits>{}</edits>", summary.edits_ok);
    println!("<writes>{}</writes>", summary.writes_ok);
    println!("<checks_ok>{}</checks_ok>", summary.checks_ok);
    println!("<checks_failed>{}</checks_failed>", summary.checks_failed);
    println!(
        "<unresolved_checks_failed>{}</unresolved_checks_failed>",
        summary.unresolved_checks_failed
    );
    println!(
        "<test_checks_ok>{}</test_checks_ok>",
        summary.test_checks_ok
    );
    println!(
        "<test_checks_failed>{}</test_checks_failed>",
        summary.test_checks_failed
    );
    println!(
        "<unresolved_test_checks_failed>{}</unresolved_test_checks_failed>",
        summary.unresolved_test_checks_failed
    );
    println!(
        "<check_commands>{}</check_commands>",
        summary.check_commands.len()
    );
    for command in &summary.check_commands {
        println!(
            "  <check_command>{}</check_command>",
            escape_xml_text(command)
        );
    }
    println!(
        "<unresolved_check_commands>{}</unresolved_check_commands>",
        summary.unresolved_check_commands.len()
    );
    for command in &summary.unresolved_check_commands {
        println!(
            "  <unresolved_check_command>{}</unresolved_check_command>",
            escape_xml_text(command)
        );
    }
    println!(
        "<check_target_paths>{}</check_target_paths>",
        summary.check_target_paths.len()
    );
    for path in &summary.check_target_paths {
        println!("  <check_target_path>{path}</check_target_path>");
    }
    println!("<lock_blocks>{}</lock_blocks>", summary.lock_blocks);
    println!(
        "<stale_write_blocks>{}</stale_write_blocks>",
        summary.stale_write_blocks
    );
    println!(
        "<unresolved_stale_write_blocks>{}</unresolved_stale_write_blocks>",
        summary.unresolved_stale_write_blocks
    );
    for path in &summary.unresolved_stale_write_paths {
        println!("  <unresolved_stale_write_path>{path}</unresolved_stale_write_path>");
    }
    println!(
        "<guarded_writes>{}</guarded_writes>",
        summary.guarded_writes
    );
    println!(
        "<edits_without_file_context>{}</edits_without_file_context>",
        summary.edits_without_file_context
    );
    println!(
        "<unresolved_edits_without_file_context>{}</unresolved_edits_without_file_context>",
        summary.unresolved_edits_without_file_context
    );
    for path in &summary.unresolved_edits_without_file_context_paths {
        println!("  <unresolved_edit_without_context>{path}</unresolved_edit_without_context>");
    }
    println!(
        "<changed_line_total>{}</changed_line_total>",
        summary.changed_line_total
    );
    println!(
        "<max_changed_lines>{}</max_changed_lines>",
        summary.max_changed_lines
    );
    println!(
        "<oversized_edits>{}</oversized_edits>",
        summary.oversized_edits
    );
    println!(
        "<changed_file_scope>{}</changed_file_scope>",
        summary.changed_file_scope
    );
    for path in &summary.changed_file_scope_paths {
        println!("  <changed_file>{path}</changed_file>");
    }
    println!(
        "<unrecorded_changed_files>{}</unrecorded_changed_files>",
        summary.unrecorded_changed_files
    );
    for path in &summary.unrecorded_changed_file_list {
        println!("  <unrecorded_changed_file>{path}</unrecorded_changed_file>");
    }
    println!("<errors>{}</errors>", summary.errors);
    println!(
        "<unresolved_errors>{}</unresolved_errors>",
        summary.unresolved_errors
    );
    println!("<quality_score>{}</quality_score>", quality.score);
    println!("<risk>{}</risk>", quality.risk);
    println!("<quality_flags>{}</quality_flags>", quality.flags.len());
    for flag in &quality.flags {
        println!("  <flag>{flag}</flag>");
    }
    println!(
        "<recommendations>{}</recommendations>",
        quality.recommendations.len()
    );
    for recommendation in &quality.recommendations {
        println!("  <recommendation>{recommendation}</recommendation>");
    }
    let handoff = handoff_state(&summary);
    println!(
        "<handoff_ready>{}</handoff_ready>",
        if handoff.ready { "true" } else { "false" }
    );
    println!(
        "<handoff_blockers>{}</handoff_blockers>",
        handoff.blockers.len()
    );
    for blocker in &handoff.blockers {
        println!(
            "  <handoff_blocker reason=\"{}\">{}</handoff_blocker>",
            escape_xml_text(blocker.reason),
            escape_xml_text(blocker.message)
        );
    }
    println!("<paths>{}</paths>", summary.paths.len());
    for path in &summary.paths {
        println!("  <path>{path}</path>");
    }
    println!("<symbols>{}</symbols>", summary.symbols.len());
    for symbol in &summary.symbols {
        println!("  <symbol>{symbol}</symbol>");
    }
    println!("<risky_paths>{}</risky_paths>", summary.risky_paths.len());
    for path in &summary.risky_paths {
        println!("  <path>{path}</path>");
    }
    println!("<signals>");
    println!(
        "  <signal name=\"context_used\" status=\"{}\"/>",
        if summary.context_reads > 0 {
            "ok"
        } else {
            "missing"
        }
    );
    println!(
        "  <signal name=\"edits_applied\" status=\"{}\"/>",
        if summary.edits_ok + summary.writes_ok > 0 {
            "ok"
        } else {
            "missing"
        }
    );
    println!(
        "  <signal name=\"lock_conflicts\" status=\"{}\" count=\"{}\"/>",
        if summary.lock_blocks == 0 {
            "ok"
        } else {
            "blocked"
        },
        summary.lock_blocks
    );
    println!(
        "  <signal name=\"checks\" status=\"{}\" passed=\"{}\" failed=\"{}\"/>",
        if summary.unresolved_checks_failed == 0 {
            "ok"
        } else {
            "failed"
        },
        summary.checks_ok,
        summary.unresolved_checks_failed
    );
    println!(
        "  <signal name=\"errors\" status=\"{}\" count=\"{}\"/>",
        if summary.unresolved_errors == 0 {
            "ok"
        } else {
            "failed"
        },
        summary.unresolved_errors
    );
    println!("</signals>");
    println!("</status>");

    Ok(())
}

fn cmd_receipt(root: &Path) -> Result<()> {
    let store = open_store(root)?;
    let events = events::load(store.anchor_root())?;
    let summary = execution_summary(root, &events);
    let quality = summary.quality_profile();
    let receipt = serde_json::json!({
        "schema": "anchor.receipt.v1",
        "repo_root": store.repo_root().to_string_lossy(),
        "event_log": events::log_path(store.anchor_root()).to_string_lossy(),
        "summary": summary,
        "quality": quality,
    });

    println!("{}", serde_json::to_string_pretty(&receipt)?);
    Ok(())
}

struct HandoffBlocker {
    reason: &'static str,
    message: &'static str,
}

struct HandoffState {
    ready: bool,
    blockers: Vec<HandoffBlocker>,
}

fn handoff_state(summary: &events::EventSummary) -> HandoffState {
    let changed = summary.edits_ok + summary.writes_ok + summary.unrecorded_changed_files > 0;
    let checked = summary.checks_ok + summary.checks_failed > 0;
    let mut blockers = Vec::new();

    if changed && summary.context_reads == 0 {
        blockers.push(HandoffBlocker {
            reason: "missing_context",
            message: "changed files without recorded Anchor context",
        });
    }
    if summary.unresolved_edits_without_file_context > 0 {
        blockers.push(HandoffBlocker {
            reason: "edited_file_without_prior_context",
            message: "some edited files have no later Anchor context read",
        });
    }
    if changed && !checked {
        blockers.push(HandoffBlocker {
            reason: "missing_check",
            message: "changed files without any recorded Anchor check",
        });
    }
    if changed && checked && summary.test_checks_ok + summary.test_checks_failed == 0 {
        blockers.push(HandoffBlocker {
            reason: "missing_test_check",
            message: "changed files without a test-like Anchor check",
        });
    }
    if summary.unresolved_checks_failed > 0 {
        blockers.push(HandoffBlocker {
            reason: "unresolved_failed_check",
            message: "at least one check command still has a failing latest result",
        });
    }
    if summary.unresolved_errors > 0 {
        blockers.push(HandoffBlocker {
            reason: "execution_error",
            message: "at least one Anchor-recorded operation error is unresolved",
        });
    }
    if summary.unresolved_stale_write_blocks > 0 {
        blockers.push(HandoffBlocker {
            reason: "stale_write_blocked",
            message: "at least one stale write block is unresolved",
        });
    }
    if summary.unrecorded_changed_files > 0 {
        blockers.push(HandoffBlocker {
            reason: "unrecorded_changed_files",
            message: "repo has changed files not recorded through Anchor writes",
        });
    }

    HandoffState {
        ready: blockers.is_empty(),
        blockers,
    }
}

fn cmd_gate(root: &Path, min_score: u8, require_receipts: bool) -> Result<()> {
    let store = open_store(root)?;
    let events = events::load(store.anchor_root())?;
    let summary = execution_summary(root, &events);
    let quality = summary.quality_profile();
    let handoff = handoff_state(&summary);

    println!("<gate>");
    println!("<score>{}</score>", quality.score);
    println!("<min_score>{min_score}</min_score>");
    println!("<risk>{}</risk>", quality.risk);
    println!("<flags>{}</flags>", quality.flags.len());
    for flag in &quality.flags {
        println!("  <flag>{flag}</flag>");
    }
    println!(
        "<recommendations>{}</recommendations>",
        quality.recommendations.len()
    );
    for recommendation in &quality.recommendations {
        println!("  <recommendation>{recommendation}</recommendation>");
    }
    println!(
        "<handoff_ready>{}</handoff_ready>",
        if handoff.ready { "true" } else { "false" }
    );
    println!(
        "<handoff_blockers>{}</handoff_blockers>",
        handoff.blockers.len()
    );
    for blocker in &handoff.blockers {
        println!(
            "  <handoff_blocker reason=\"{}\">{}</handoff_blocker>",
            escape_xml_text(blocker.reason),
            escape_xml_text(blocker.message)
        );
    }

    // No receipt, no merge: every changed file must have a recorded Anchor
    // write event, or the change is untracked work the receipt cannot vouch for.
    if require_receipts && summary.unrecorded_changed_files > 0 {
        println!("<unreceipted_files>{}</unreceipted_files>", summary.unrecorded_changed_files);
        for path in &summary.unrecorded_changed_file_list {
            println!("  <unreceipted_file>{}</unreceipted_file>", escape_xml_text(path));
        }
        println!("<status>failed</status>");
        println!("</gate>");
        bail!(
            "receipt gate failed: {} changed file(s) have no recorded write event",
            summary.unrecorded_changed_files
        );
    }

    if handoff.ready && quality.score >= min_score {
        println!("<status>ok</status>");
        println!("</gate>");
        Ok(())
    } else {
        println!("<status>failed</status>");
        println!("</gate>");
        if !handoff.ready {
            bail!("handoff gate failed: unresolved blockers remain");
        }
        bail!(
            "quality gate failed: score {} below {}",
            quality.score,
            min_score
        )
    }
}

fn cmd_check(root: &Path, command: &[String]) -> Result<()> {
    if command.is_empty() {
        bail!("check requires a command");
    }

    let store = open_store(root)?;
    let mut cmd = std::process::Command::new(&command[0]);
    cmd.args(&command[1..]).current_dir(root);
    if cli_protect::is_active(root) {
        cmd.env("PYTHONDONTWRITEBYTECODE", "1");
    }
    let output = cmd.output()?;
    let code = output.status.code().unwrap_or(-1);
    let status = if output.status.success() {
        "ok"
    } else {
        "error"
    };
    let command_text = command.join(" ");
    let check_kind = classify_check_command(command);
    let target_paths = check_target_paths(root, command);
    let mut meta = std::collections::BTreeMap::new();
    meta.insert("command".to_string(), command_text.clone());
    meta.insert("check_kind".to_string(), check_kind.to_string());
    if !target_paths.is_empty() {
        meta.insert("target_paths".to_string(), target_paths.join("\n"));
    }

    events::record_with_meta(
        store.anchor_root(),
        "check.run",
        None,
        None,
        status,
        Some(format!("exit={code} cmd={command_text}")),
        meta,
    );
    let events_after = events::load(store.anchor_root())?;
    let summary = execution_summary(root, &events_after);
    let handoff = handoff_state(&summary);

    println!("<check>");
    println!("<command>{command_text}</command>");
    println!("<kind>{check_kind}</kind>");
    println!("<status>{status}</status>");
    println!("<exit_code>{code}</exit_code>");
    println!("<target_paths>{}</target_paths>", target_paths.len());
    for path in &target_paths {
        println!("  <target_path>{path}</target_path>");
    }
    println!("<stdout><![CDATA[");
    print!("{}", String::from_utf8_lossy(&output.stdout));
    println!("]]></stdout>");
    println!("<stderr><![CDATA[");
    print!("{}", String::from_utf8_lossy(&output.stderr));
    println!("]]></stderr>");
    if !handoff.ready {
        println!("<quality_feedback>");
        for blocker in &handoff.blockers {
            println!("  <warning>{}</warning>", escape_xml_text(blocker.message));
        }
        println!("</quality_feedback>");
        for blocker in &handoff.blockers {
            println!(
                "<handoff_gate status=\"blocked\" reason=\"{}\"/>",
                escape_xml_text(blocker.reason)
            );
        }
    } else {
        println!("<handoff_gate status=\"ok\"/>");
    }
    println!("</check>");

    if !output.status.success() {
        bail!("check failed with exit code {code}")
    }
    if !handoff.ready {
        bail!("handoff check failed: unresolved blockers remain")
    }
    Ok(())
}

fn classify_check_command(command: &[String]) -> &'static str {
    let tokens: Vec<String> = command
        .iter()
        .map(|token| token.to_ascii_lowercase())
        .collect();
    if tokens.is_empty() {
        return "unknown";
    }
    let runner_names = [
        "pytest", "unittest", "cargo", "go", "npm", "pnpm", "yarn", "bun", "mvn", "gradle", "tox",
        "nox", "vitest", "jest", "mocha", "rspec", "mix",
    ];
    for (idx, token) in tokens.iter().enumerate() {
        let name = Path::new(token)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(token);
        if name == "pytest"
            || name == "tox"
            || name == "nox"
            || name == "vitest"
            || name == "jest"
            || name == "mocha"
            || name == "rspec"
        {
            return "test";
        }
        if name == "python" || name == "python3" {
            if idx + 2 < tokens.len() && tokens[idx + 1] == "-m" && tokens[idx + 2] == "pytest" {
                return "test";
            }
            if idx + 2 < tokens.len() && tokens[idx + 1] == "-m" && tokens[idx + 2] == "unittest" {
                return "test";
            }
        }
        if [
            "cargo", "go", "npm", "pnpm", "yarn", "bun", "mvn", "gradle", "mix",
        ]
        .contains(&name)
            && tokens
                .iter()
                .skip(idx + 1)
                .any(|arg| arg == "test" || arg == "tests" || arg == "./...")
        {
            return "test";
        }
        if runner_names.contains(&name) && tokens.iter().any(|arg| arg.contains("test")) {
            return "test";
        }
    }
    "non_test"
}

fn check_target_paths(root: &Path, command: &[String]) -> Vec<String> {
    let mut paths = std::collections::BTreeSet::new();
    for arg in command {
        let cleaned = arg
            .trim_matches(|ch: char| ch == '\'' || ch == '"' || ch == ',' || ch == ';')
            .trim();
        if cleaned.is_empty() || cleaned.starts_with('-') {
            continue;
        }
        if cleaned.contains("://") || cleaned.contains('=') {
            continue;
        }
        let candidate = root.join(cleaned);
        if candidate.exists() {
            let relative = candidate
                .strip_prefix(root)
                .unwrap_or(candidate.as_path())
                .to_string_lossy()
                .replace('\\', "/");
            if !relative.is_empty() && relative != "." {
                paths.insert(relative);
            }
        }
    }
    paths.into_iter().collect()
}

fn cmd_run(root: &Path, command: &[String]) -> Result<()> {
    if command.is_empty() {
        bail!("run requires a command");
    }

    let store = open_store(root)?;
    let before = git_changed_paths(root)
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    let command_text = command.join(" ");

    let mut cmd = std::process::Command::new(&command[0]);
    cmd.args(&command[1..]).current_dir(root);
    if cli_protect::is_active(root) {
        cmd.env("PYTHONDONTWRITEBYTECODE", "1");
    }
    let output = cmd.output()?;
    let code = output.status.code().unwrap_or(-1);
    let status = if output.status.success() {
        "ok"
    } else {
        "error"
    };

    events::record(
        store.anchor_root(),
        "terminal.run",
        None,
        None,
        status,
        Some(format!("exit={code} cmd={command_text}")),
    );

    let after = git_changed_paths(root)
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    let newly_changed: Vec<String> = after.difference(&before).cloned().collect();
    let events_after = events::load(store.anchor_root())?;
    let summary_after = events::EventSummary::from_events(&events_after);
    let recorded_writes = summary_after
        .recorded_write_paths
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let raw_changed: Vec<String> = newly_changed
        .into_iter()
        .filter(|path| !recorded_writes.contains(path))
        .collect();

    for path in &raw_changed {
        events::record(
            store.anchor_root(),
            "terminal.raw_write",
            Some(path.clone()),
            None,
            "error",
            Some(format!("cmd={command_text}")),
        );
    }

    println!("<run>");
    println!("<command>{command_text}</command>");
    println!("<status>{status}</status>");
    println!("<exit_code>{code}</exit_code>");
    println!(
        "<raw_changed_files>{}</raw_changed_files>",
        raw_changed.len()
    );
    for path in &raw_changed {
        println!("  <raw_changed_file>{path}</raw_changed_file>");
    }
    println!("<stdout><![CDATA[");
    print!("{}", String::from_utf8_lossy(&output.stdout));
    println!("]]></stdout>");
    println!("<stderr><![CDATA[");
    print!("{}", String::from_utf8_lossy(&output.stderr));
    println!("]]></stderr>");
    println!("</run>");

    if !output.status.success() {
        bail!("run command failed with exit code {code}");
    }
    if !raw_changed.is_empty() {
        bail!("run command changed files outside Anchor writes");
    }
    Ok(())
}

fn cmd_trace(root: &Path, limit: usize) -> Result<()> {
    let store = open_store(root)?;
    let events = events::load(store.anchor_root())?;
    let start = events.len().saturating_sub(limit);

    println!(
        "<trace count=\"{}\" shown=\"{}\">",
        events.len(),
        events.len().saturating_sub(start)
    );
    for event in events.iter().skip(start) {
        println!(
            "  <event id=\"{}\" type=\"{}\" status=\"{}\" agent=\"{}\" session=\"{}\" ts=\"{}\">",
            event.event_id,
            event.event_type,
            event.status,
            event.agent_id,
            event.session_id,
            event.timestamp_ms
        );
        if let Some(path) = &event.path {
            println!("    <path>{path}</path>");
        }
        if let Some(symbol) = &event.symbol {
            println!("    <symbol>{symbol}</symbol>");
        }
        if let Some(message) = &event.message {
            println!("    <message>{message}</message>");
        }
        println!("  </event>");
    }
    println!("</trace>");

    Ok(())
}

const DEFAULT_CONTEXT_LINE_BUDGET: usize = 120;

fn print_bounded_numbered_code(code: &str, full: bool) {
    for (idx, line) in code.lines().enumerate() {
        if !full && idx >= DEFAULT_CONTEXT_LINE_BUDGET {
            println!(
                "    ... [context truncated at {DEFAULT_CONTEXT_LINE_BUDGET} lines; rerun with --full for complete symbol]"
            );
            break;
        }
        println!("{line}");
    }
}

fn print_bounded_plain_code(code: &str, start_line: usize, full: bool) {
    for (i, line) in code.lines().enumerate() {
        if !full && i >= DEFAULT_CONTEXT_LINE_BUDGET {
            println!(
                "    ... [context truncated at {DEFAULT_CONTEXT_LINE_BUDGET} lines; rerun with --full for complete symbol]"
            );
            break;
        }
        println!(" {:>3}: {}", start_line + i, line);
    }
}

fn print_constructor_child_context(
    store: &AnchorStore,
    symbols: &[SymbolEntry],
    parent: &SymbolEntry,
) -> Result<()> {
    if !is_class_like_symbol(parent) {
        return Ok(());
    }

    let mut children: Vec<&SymbolEntry> = symbols
        .iter()
        .filter(|candidate| {
            candidate.path == parent.path
                && candidate.line_start > parent.line_start
                && candidate.line_end <= parent.line_end
                && is_constructor_like_symbol(candidate)
        })
        .collect();
    children.sort_by_key(|symbol| symbol.line_start);
    children.truncate(2);

    for child in children {
        let projection = store.create_projection(child)?;
        println!(
            "<child_context role=\"constructor\" name=\"{}\" line=\"{}\">",
            escape_xml_text(&child.name),
            child.line_start
        );
        print_bounded_plain_code(&projection.text, child.line_start, false);
        println!("</child_context>");
    }

    Ok(())
}

fn is_class_like_symbol(symbol: &SymbolEntry) -> bool {
    matches!(
        symbol.kind.to_ascii_lowercase().as_str(),
        "class" | "struct" | "interface" | "enum"
    )
}

fn is_constructor_like_symbol(symbol: &SymbolEntry) -> bool {
    let name = symbol.name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        "__init__" | "constructor" | "init" | "new" | "default"
    )
}

fn escape_xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

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

fn task_symbol_rank(
    symbol: &anchor::storage::SymbolEntry,
    tokens: &std::collections::BTreeSet<String>,
) -> i32 {
    let name = symbol.name.to_ascii_lowercase();
    let path = symbol.path.to_ascii_lowercase();
    let kind = symbol.kind.as_str();
    let mut score = 0i32;
    let mut matched_terms = 0usize;

    if matches!(
        kind,
        "Class" | "Struct" | "Enum" | "Interface" | "Trait" | "Function" | "Method"
    ) {
        score += 20;
    }
    if matches!(kind, "Class" | "Struct" | "Interface" | "Trait") {
        score += 20;
    }

    for token in tokens {
        let mut matched = false;
        if name == *token {
            score += 160;
            matched = true;
        } else if name.contains(token) {
            score += 80;
            matched = true;
        }
        if path.contains(token) {
            score += 45;
            matched = true;
        }
        if symbol.features.iter().any(|feature| feature == token) {
            score += 20;
            matched = true;
        }
        if matched {
            matched_terms += 1;
        }
    }

    if matched_terms >= 2 {
        let coverage_terms = matched_terms.min(8);
        score += (coverage_terms * coverage_terms * 12) as i32;
    }

    if matches!(
        name.as_str(),
        "add" | "append" | "call" | "create" | "delete" | "get" | "insert" | "remove" | "set"
    ) {
        score -= 120;
    }
    if symbol.path.contains("/tests/") || symbol.path.starts_with("tests/") {
        score -= 140;
    }
    if name.starts_with("test") {
        score -= 120;
    }

    score
}

fn source_backed_task_candidates(
    root: &Path,
    symbols: &[anchor::storage::SymbolEntry],
    tokens: &std::collections::BTreeSet<String>,
    limit: usize,
) -> Vec<anchor::storage::SymbolEntry> {
    use std::collections::BTreeMap;

    let mut by_path: BTreeMap<&str, Vec<&anchor::storage::SymbolEntry>> = BTreeMap::new();
    for symbol in symbols {
        by_path.entry(&symbol.path).or_default().push(symbol);
    }

    let mut scored = Vec::new();
    for (path, path_symbols) in by_path {
        let path_has_signal = task_path_has_signal(path, tokens);
        let symbol_has_signal = path_symbols
            .iter()
            .any(|symbol| task_symbol_has_name_or_feature_signal(symbol, tokens));
        if !path_has_signal && !symbol_has_signal {
            continue;
        }

        let full_path = root.join(path);
        if !anchor::parser::language::is_source_path(&full_path) {
            continue;
        }
        if path.contains("/tests/") || path.starts_with("tests/") {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&full_path) else {
            continue;
        };
        let file_score = task_source_rank(&source, tokens);
        if file_score < 80 {
            continue;
        }

        for symbol in path_symbols {
            if !matches!(
                symbol.kind.as_str(),
                "Class" | "Struct" | "Enum" | "Interface" | "Trait" | "Function" | "Method"
            ) {
                continue;
            }
            let owner_bonus = if matches!(
                symbol.kind.as_str(),
                "Class" | "Struct" | "Enum" | "Interface" | "Trait"
            ) {
                40
            } else {
                0
            };
            scored.push((
                file_score.saturating_mul(2) + owner_bonus + task_symbol_rank(symbol, tokens),
                (*symbol).clone(),
            ));
        }
    }

    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.path.cmp(&b.1.path))
            .then_with(|| a.1.line_start.cmp(&b.1.line_start))
            .then_with(|| a.1.name.cmp(&b.1.name))
    });
    scored.truncate(limit);
    scored.into_iter().map(|(_, symbol)| symbol).collect()
}

fn task_path_has_signal(path: &str, tokens: &std::collections::BTreeSet<String>) -> bool {
    let lower_path = path.to_ascii_lowercase();
    let path_tokens = path_signal_tokens(path);
    tokens.iter().any(|token| {
        lower_path.contains(token)
            || path_tokens
                .iter()
                .any(|path_token| soft_token_match(token, path_token))
    })
}

fn task_symbol_has_name_or_feature_signal(
    symbol: &anchor::storage::SymbolEntry,
    tokens: &std::collections::BTreeSet<String>,
) -> bool {
    let lower_name = symbol.name.to_ascii_lowercase();
    tokens.iter().any(|token| {
        lower_name.contains(token) || symbol.features.iter().any(|feature| feature == token)
    })
}

fn dedupe_symbols(symbols: &mut Vec<anchor::storage::SymbolEntry>) {
    use std::collections::BTreeSet;

    let mut seen = BTreeSet::new();
    symbols.retain(|symbol| seen.insert(task_symbol_key(symbol)));
}

fn task_symbol_total_rank(
    symbol: &anchor::storage::SymbolEntry,
    tokens: &std::collections::BTreeSet<String>,
    source_scores: &std::collections::BTreeMap<String, i32>,
) -> i32 {
    let source_score = source_scores
        .get(&task_symbol_key(symbol))
        .copied()
        .unwrap_or_default();
    let owner_bonus = if matches!(
        symbol.kind.as_str(),
        "Class" | "Struct" | "Enum" | "Interface" | "Trait"
    ) && source_score >= 80
    {
        320
    } else {
        0
    };

    task_symbol_rank(symbol, tokens) + source_score.saturating_mul(2) + owner_bonus
}

fn task_symbol_key(symbol: &anchor::storage::SymbolEntry) -> String {
    format!("{}:{}:{}", symbol.path, symbol.line_start, symbol.name)
}

fn task_source_rank(source: &str, tokens: &std::collections::BTreeSet<String>) -> i32 {
    let source = source.to_ascii_lowercase();
    let mut score = 0;
    let mut matched_terms = 0usize;

    for token in tokens {
        if source.contains(token) {
            matched_terms += 1;
            score += 28;
        }
    }

    if matched_terms >= 2 {
        let coverage_terms = matched_terms.min(10);
        score += (coverage_terms * coverage_terms * 10) as i32;
    }

    score
}

fn rank_task_slices(
    store: &AnchorStore,
    symbols: &[anchor::storage::SymbolEntry],
    call_index: &anchor::storage::CallIndex,
    tokens: &std::collections::BTreeSet<String>,
    candidates: &[anchor::storage::SymbolEntry],
    limit: usize,
) -> Vec<TaskSlice> {
    use std::collections::BTreeSet;

    let candidate_keys: BTreeSet<String> = candidates.iter().map(task_symbol_key).collect();
    let candidate_names: BTreeSet<String> = candidates
        .iter()
        .map(|symbol| symbol.name.to_ascii_lowercase())
        .collect();
    let candidate_paths: BTreeSet<&str> = candidates
        .iter()
        .map(|symbol| symbol.path.as_str())
        .collect();

    let mut scored = Vec::new();
    for symbol in symbols {
        if looks_like_test_path(&symbol.path) || !is_context_owner_symbol(symbol) {
            continue;
        }
        if is_large_owner_symbol(symbol) {
            continue;
        }
        let is_ranked_candidate = candidate_keys.contains(&task_symbol_key(symbol));
        let callers = call_index.callers_of(&symbol.name);
        let callees = call_index.callees_of(&symbol.name);
        let neighbor_hit = callers
            .iter()
            .chain(callees.iter())
            .any(|name| candidate_names.contains(&name.to_ascii_lowercase()));
        let is_candidate_path = candidate_paths.contains(symbol.path.as_str());
        let direct_signal = task_symbol_has_name_or_feature_signal(symbol, tokens);
        if !is_ranked_candidate && !neighbor_hit && !(is_candidate_path && direct_signal) {
            continue;
        }

        let Ok(projection) = store.create_projection(symbol) else {
            continue;
        };

        let mut reasons = Vec::new();
        let mut score = task_symbol_rank(symbol, tokens);

        let source_score = task_source_rank(&projection.text, tokens);
        if source_score > 0 {
            reasons.push("content".to_string());
            score += source_score;
        }

        let chunk_score = task_chunk_rank(&projection.text, symbol, tokens, &mut reasons);
        score += chunk_score;

        if is_ranked_candidate {
            reasons.push("ranked_symbol".to_string());
            score += 220;
        }

        if neighbor_hit {
            reasons.push("call_neighbor".to_string());
            score += 80;
        }

        if reasons.is_empty() && !is_ranked_candidate && !neighbor_hit {
            continue;
        }

        score += task_path_prior(&symbol.path);

        if score <= 0 {
            continue;
        }

        dedupe_strings(&mut reasons);
        let call_lines = store.call_lines_for_symbol(symbol);
        let sliced = slice_code(&projection.text, &call_lines, symbol.line_start);
        let code = if sliced.was_sliced {
            sliced.code
        } else {
            numbered_code(&projection.text, symbol.line_start)
        };

        scored.push(TaskSlice {
            path: symbol.path.clone(),
            source_hash: symbol.source_hash.clone(),
            symbol: symbol.name.clone(),
            kind: symbol.kind.clone(),
            line_start: symbol.line_start,
            line_end: symbol.line_end,
            score,
            reasons,
            code,
        });
    }

    scored.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.line_start.cmp(&b.line_start))
            .then_with(|| a.symbol.cmp(&b.symbol))
    });
    scored.truncate(limit);
    scored
}

fn is_context_owner_symbol(symbol: &anchor::storage::SymbolEntry) -> bool {
    matches!(
        symbol.kind.as_str(),
        "Class" | "Struct" | "Enum" | "Interface" | "Trait" | "Function" | "Method" | "Impl"
    )
}

fn task_chunk_rank(
    text: &str,
    symbol: &anchor::storage::SymbolEntry,
    query_tokens: &std::collections::BTreeSet<String>,
    reasons: &mut Vec<String>,
) -> i32 {
    if query_tokens.is_empty() {
        return 0;
    }

    let mut score = 0;
    let lower_text = text.to_ascii_lowercase();
    let lower_path = symbol.path.to_ascii_lowercase();
    let lower_name = symbol.name.to_ascii_lowercase();
    let file_stem_tokens = Path::new(&lower_path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(task_search_tokens)
        .unwrap_or_default();
    let chunk_tokens = task_search_tokens(&format!("{} {} {}", symbol.name, symbol.path, text));
    let mut matched_query_terms = 0usize;

    for token in query_tokens {
        let mut token_matched = false;
        if lower_name == *token {
            reasons.push("symbol_exact".to_string());
            score += 180;
            token_matched = true;
        } else if lower_name.contains(token) {
            reasons.push("symbol_match".to_string());
            score += 90;
            token_matched = true;
        }

        if file_stem_tokens
            .iter()
            .any(|stem_token| soft_token_match(token, stem_token))
        {
            reasons.push("file_stem_match".to_string());
            score += 220;
            token_matched = true;
        } else if lower_path.contains(token) {
            reasons.push("path_match".to_string());
            score += 65;
            token_matched = true;
        }

        if lower_text.contains(token) {
            reasons.push("literal_match".to_string());
            score += 28;
            token_matched = true;
        } else if chunk_tokens
            .iter()
            .any(|chunk_token| soft_token_match(token, chunk_token))
        {
            reasons.push("soft_token_match".to_string());
            score += 14;
            token_matched = true;
        }

        if symbol.features.iter().any(|feature| feature == token) {
            reasons.push("feature_match".to_string());
            score += 35;
            token_matched = true;
        }

        if token_matched {
            matched_query_terms += 1;
        }
    }

    if matched_query_terms >= 2 {
        let coverage_terms = matched_query_terms.min(8);
        reasons.push("query_coverage".to_string());
        score += (coverage_terms * coverage_terms * 8) as i32;
    }

    score
}

fn is_large_owner_symbol(symbol: &anchor::storage::SymbolEntry) -> bool {
    let lines = symbol.line_end.saturating_sub(symbol.line_start) + 1;
    if is_class_like_symbol(symbol) {
        return lines > 80;
    }
    matches!(symbol.kind.as_str(), "Function" | "Method") && lines > 120
}

fn task_path_prior(path: &str) -> i32 {
    let normalised = path.replace('\\', "/").to_ascii_lowercase();
    let file_name = Path::new(&normalised)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let mut score = 0;

    if file_name == "__init__.py" || file_name == "mod.rs" {
        score -= 35;
    }
    if normalised.contains("/examples/") || normalised.starts_with("examples/") {
        score -= 80;
    }
    if normalised.contains("/docs/") || normalised.starts_with("docs/") {
        score -= 60;
    }
    if normalised.contains("/legacy/") || normalised.contains("/compat/") {
        score -= 45;
    }

    score
}

fn task_search_tokens(text: &str) -> std::collections::BTreeSet<String> {
    let mut tokens = std::collections::BTreeSet::new();
    for token in tokenize_intent(text).filter(|token| token.len() >= 3) {
        tokens.insert(token.clone());
        for part in split_camel_token(&token) {
            if part.len() >= 3 {
                tokens.insert(part);
            }
        }
    }
    tokens
}

fn split_camel_token(token: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in token.chars() {
        if ch.is_ascii_uppercase() && !current.is_empty() {
            out.push(current.to_ascii_lowercase());
            current.clear();
        }
        current.push(ch);
    }
    if !current.is_empty() {
        out.push(current.to_ascii_lowercase());
    }
    out
}

fn soft_token_match(query: &str, candidate: &str) -> bool {
    if query == candidate {
        return true;
    }
    if query.len() < 4 || candidate.len() < 4 {
        return false;
    }
    if query.starts_with(candidate) || candidate.starts_with(query) {
        return true;
    }
    common_prefix_len(query, candidate) >= 4
}

fn common_prefix_len(a: &str, b: &str) -> usize {
    a.chars()
        .zip(b.chars())
        .take_while(|(left, right)| left == right)
        .count()
}

fn numbered_code(code: &str, start_line: usize) -> String {
    let mut output = String::new();
    for (offset, line) in code.lines().take(DEFAULT_CONTEXT_LINE_BUDGET).enumerate() {
        output.push_str(&format!(" {:>3}: {}\n", start_line + offset, line));
    }
    if code.lines().count() > DEFAULT_CONTEXT_LINE_BUDGET {
        output.push_str(&format!(
            "    ... [context truncated at {DEFAULT_CONTEXT_LINE_BUDGET} lines]\n"
        ));
    }
    output
}

fn dedupe_strings(values: &mut Vec<String>) {
    let mut seen = std::collections::BTreeSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

fn build_task_workspace(
    intent: &str,
    slices: &[TaskSlice],
    related_files: &std::collections::BTreeSet<String>,
    historical_files: &std::collections::BTreeMap<String, usize>,
    likely_tests: &[(&String, usize)],
    verification_plan: &TaskVerificationPlan,
    file_hashes: &std::collections::BTreeMap<&str, &str>,
) -> TaskWorkspace {
    let mut path_scores: std::collections::BTreeMap<String, (i32, String)> =
        std::collections::BTreeMap::new();
    let mut path_hashes: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    let mut slice_counts: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();

    let mut selected_slices = select_diverse_task_slices(slices, 12);

    for slice in &selected_slices {
        path_hashes.insert(slice.path.clone(), slice.source_hash.clone());
        let selected_count = slice_counts.entry(slice.path.clone()).or_default();
        let contribution = match *selected_count {
            0 => slice.score.max(0),
            1 => slice.score.max(0) / 2,
            _ => slice.score.max(0) / 4,
        };
        *selected_count += 1;
        let entry = path_scores
            .entry(slice.path.clone())
            .or_insert((0, "source".to_string()));
        entry.0 += contribution;
    }

    for path in related_files {
        if let Some(entry) = path_scores.get_mut(path) {
            entry.0 += 40;
            if entry.1 == "source" {
                entry.1 = "source+related".to_string();
            }
        }
    }

    for (path, score) in historical_files {
        if let Some(entry) = path_scores.get_mut(path) {
            entry.0 += (*score).min(500) as i32;
            entry.1 = format!("{}+history", entry.1);
        } else if *score >= 50 && !looks_like_test_path(path) {
            // Files that historically co-change with the seed files belong in
            // the working set even when no slice matched the intent: registry
            // and wiring files rarely contain the task's keywords but are
            // edited in almost every related commit.
            if let Some(hash) = file_hashes.get(path.as_str()) {
                path_hashes.insert(path.clone(), (*hash).to_string());
                path_scores.insert(
                    path.clone(),
                    ((*score).min(500) as i32, "history".to_string()),
                );
            }
        }
    }

    let mut active_paths: Vec<TaskPath> = path_scores
        .into_iter()
        .filter_map(|(path, (score, role))| {
            path_hashes.get(&path).map(|source_hash| TaskPath {
                path,
                source_hash: source_hash.clone(),
                score,
                role,
            })
        })
        .collect();
    active_paths.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    active_paths.truncate(8);
    let active_path_set: std::collections::BTreeSet<&str> =
        active_paths.iter().map(|path| path.path.as_str()).collect();

    let mut workspace_related_files: Vec<TaskRelatedFile> = related_files
        .iter()
        .filter(|path| !looks_like_test_path(path))
        .filter(|path| !active_path_set.contains(path.as_str()))
        .map(|path| {
            let history_score = historical_files.get(path).copied().unwrap_or_default();
            TaskRelatedFile {
                path: path.clone(),
                score: 40 + history_score.min(500),
                reason: if history_score > 0 {
                    "related+history".to_string()
                } else {
                    "related".to_string()
                },
            }
        })
        .collect();
    workspace_related_files.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    workspace_related_files.truncate(12);

    TaskWorkspace {
        schema: TASK_WORKSPACE_SCHEMA.to_string(),
        intent: intent.to_string(),
        active_paths,
        exact_slices: {
            selected_slices.truncate(12);
            selected_slices
        },
        related_files: workspace_related_files,
        likely_tests: likely_tests
            .iter()
            .take(6)
            .map(|(path, score)| TaskTest {
                path: (*path).clone(),
                score: *score,
            })
            .collect(),
        verification_plan: verification_plan.clone(),
    }
}

fn select_diverse_task_slices(slices: &[TaskSlice], limit: usize) -> Vec<TaskSlice> {
    let mut selected = Vec::new();
    let mut per_file: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    let mut remaining: Vec<&TaskSlice> = slices.iter().collect();

    while selected.len() < limit && !remaining.is_empty() {
        let best = remaining
            .iter()
            .enumerate()
            .map(|(idx, slice)| {
                let selected_for_file = per_file.get(slice.path.as_str()).copied().unwrap_or(0);
                let divisor = match selected_for_file {
                    0 => 1,
                    1 => 2,
                    _ => 4,
                };
                (idx, slice.score / divisor)
            })
            .max_by(|left, right| {
                left.1
                    .cmp(&right.1)
                    .then_with(|| remaining[right.0].path.cmp(&remaining[left.0].path))
                    .then_with(|| {
                        remaining[right.0]
                            .line_start
                            .cmp(&remaining[left.0].line_start)
                    })
            });

        let Some((idx, effective_score)) = best else {
            break;
        };
        if effective_score <= 0 {
            break;
        }
        let slice = remaining.swap_remove(idx);
        *per_file.entry(slice.path.as_str()).or_default() += 1;
        selected.push(slice.clone());
    }

    selected
}

fn rank_task_tests(
    files: &[anchor::storage::PathEntry],
    task_tokens: &std::collections::BTreeSet<String>,
    candidates: &[anchor::storage::SymbolEntry],
    slices: &[TaskSlice],
    source_seed_paths: &std::collections::BTreeSet<String>,
    historical_tests: &std::collections::BTreeMap<String, usize>,
    limit: usize,
) -> Vec<TaskTest> {
    let exact_slices = select_diverse_task_slices(slices, 12);
    let mut active_source_weights: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for (idx, slice) in exact_slices.iter().enumerate() {
        let rank_weight = 12usize.saturating_sub(idx.min(11));
        let score_weight = (slice.score.max(0) as usize / 500).clamp(1, 8);
        let entry = active_source_weights.entry(slice.path.clone()).or_default();
        *entry += rank_weight + score_weight;
    }
    if active_source_weights.is_empty() {
        for source_path in source_seed_paths.iter().take(8) {
            active_source_weights.insert(source_path.clone(), 2);
        }
    }

    let mut active_path_tokens = std::collections::BTreeSet::new();
    for source_path in active_source_weights.keys() {
        active_path_tokens.extend(path_signal_tokens(source_path));
    }

    let mut active_symbol_tokens = std::collections::BTreeSet::new();
    for slice in &exact_slices {
        active_symbol_tokens.extend(task_search_tokens(&slice.symbol));
    }
    for symbol in candidates.iter().take(8) {
        active_symbol_tokens.extend(task_search_tokens(&symbol.name));
    }

    let mut scored = std::collections::BTreeMap::new();
    for file in files {
        if !looks_like_test_path(&file.path) || is_test_helper_path(&file.path) {
            continue;
        }
        let path_lower = file.path.to_ascii_lowercase();
        let mut score = historical_tests
            .get(&file.path)
            .copied()
            .unwrap_or_default()
            .saturating_mul(4);

        for (source_path, weight) in &active_source_weights {
            score += source_test_affinity_score(source_path, &file.path) * *weight;
        }
        let test_path_tokens = path_signal_tokens(&file.path);
        for token in &active_path_tokens {
            if path_lower.contains(token.as_str())
                || test_path_tokens
                    .iter()
                    .any(|test_token| soft_token_match(token, test_token))
            {
                score += 180;
            }
        }
        for token in &active_symbol_tokens {
            if token.len() >= 4 && path_lower.contains(token.as_str()) {
                score += 60;
            }
        }
        for token in task_tokens {
            if path_lower.contains(token.as_str()) {
                score += 20;
            }
        }
        if score > 0 {
            scored.insert(file.path.clone(), score);
        }
    }

    top_scored_paths(&scored, limit)
        .into_iter()
        .map(|(path, score)| TaskTest {
            path: path.clone(),
            score,
        })
        .collect()
}

fn is_test_helper_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let file_name = Path::new(&lower)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    file_name == "conftest.py"
        || file_name == "__init__.py"
        || file_name.contains("helper")
        || file_name.contains("fixture")
        || lower.contains("/examples/")
        || lower.starts_with("examples/")
}

fn print_task_workspace(workspace: &TaskWorkspace, path: &Path) {
    println!(
        "<task_workspace schema=\"{}\" file=\"{}\">",
        escape_xml_text(&workspace.schema),
        escape_xml_text(&path.display().to_string())
    );
    println!("<active_paths count=\"{}\">", workspace.active_paths.len());
    for active in workspace.active_paths.iter().take(6) {
        println!(
            "  <file path=\"{}\" score=\"{}\" role=\"{}\" hash=\"{}\"/>",
            escape_xml_text(&active.path),
            active.score,
            escape_xml_text(&active.role),
            escape_xml_text(&active.source_hash)
        );
    }
    println!("</active_paths>");

    println!("<exact_slices count=\"{}\">", workspace.exact_slices.len());
    for slice in workspace.exact_slices.iter().take(5) {
        println!(
            "<slice file=\"{}\" symbol=\"{}\" kind=\"{}\" lines=\"{}-{}\" score=\"{}\" reasons=\"{}\">",
            escape_xml_text(&slice.path),
            escape_xml_text(&slice.symbol),
            escape_xml_text(&slice.kind),
            slice.line_start,
            slice.line_end,
            slice.score,
            escape_xml_text(&slice.reasons.join(","))
        );
        println!("<code>");
        print_bounded_numbered_code(&slice.code, false);
        println!("</code>");
        println!("</slice>");
    }
    println!("</exact_slices>");

    println!(
        "<workspace_related_files count=\"{}\">",
        workspace.related_files.len()
    );
    for related in workspace.related_files.iter().take(6) {
        println!(
            "  <file path=\"{}\" score=\"{}\" reason=\"{}\"/>",
            escape_xml_text(&related.path),
            related.score,
            escape_xml_text(&related.reason)
        );
    }
    println!("</workspace_related_files>");

    println!(
        "<workspace_tests count=\"{}\">",
        workspace.likely_tests.len()
    );
    for test in &workspace.likely_tests {
        println!(
            "  <file path=\"{}\" score=\"{}\"/>",
            escape_xml_text(&test.path),
            test.score
        );
    }
    println!("</workspace_tests>");
    print_verification_plan(&workspace.verification_plan);
    println!("</task_workspace>");
}

fn workspace_has_exact_slice(
    workspace: &TaskWorkspace,
    symbol: &anchor::storage::SymbolEntry,
) -> bool {
    workspace.exact_slices.iter().any(|slice| {
        slice.path == symbol.path
            && slice.symbol == symbol.name
            && slice.line_start == symbol.line_start
    })
}

fn top_scored_paths(
    scores: &std::collections::BTreeMap<String, usize>,
    limit: usize,
) -> Vec<(&String, usize)> {
    let mut items: Vec<_> = scores.iter().map(|(path, score)| (path, *score)).collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    items.truncate(limit);
    items
}

fn source_test_affinity_score(source_path: &str, test_path: &str) -> usize {
    let source_tokens = path_signal_tokens(source_path);
    let test_tokens = path_signal_tokens(test_path);
    let shared = source_tokens
        .iter()
        .filter(|source_token| {
            test_tokens
                .iter()
                .any(|test_token| soft_token_match(source_token, test_token))
        })
        .count();
    let mut score = shared * 600;

    if let (Some(source_stem), Some(test_stem)) = (
        normalised_file_stem(source_path),
        normalised_file_stem(test_path),
    ) {
        if source_stem == test_stem {
            score += 2400;
        }
    }

    if let (Some(source_parent), Some(test_parent)) = (
        Path::new(source_path).parent(),
        Path::new(test_path).parent(),
    ) {
        if source_parent == test_parent {
            score += 1200;
        }
    }

    score
}

fn normalised_file_stem(path: &str) -> Option<String> {
    let stem = Path::new(path).file_stem()?.to_str()?.to_ascii_lowercase();
    Some(
        stem.strip_prefix("test_")
            .unwrap_or(&stem)
            .strip_suffix("_test")
            .unwrap_or_else(|| stem.strip_suffix(".test").unwrap_or(&stem))
            .to_string(),
    )
}

fn build_task_verification_plan(likely_tests: &[(&String, usize)]) -> TaskVerificationPlan {
    let mut hints_with_rank: Vec<(usize, TaskCheckHint)> = Vec::new();

    if let Some((rank, command)) = python_test_command(likely_tests) {
        hints_with_rank.push((
            rank,
            TaskCheckHint {
                kind: "python_tests".to_string(),
                command,
            },
        ));
    }
    if let Some((rank, command)) = go_test_command(likely_tests) {
        hints_with_rank.push((
            rank,
            TaskCheckHint {
                kind: "go_tests".to_string(),
                command,
            },
        ));
    }
    if let Some((rank, command)) = rust_test_command(likely_tests) {
        hints_with_rank.push((
            rank,
            TaskCheckHint {
                kind: "rust_tests".to_string(),
                command,
            },
        ));
    }
    if let Some((rank, command)) = js_test_command(likely_tests) {
        hints_with_rank.push((
            rank,
            TaskCheckHint {
                kind: "javascript_tests".to_string(),
                command,
            },
        ));
    }

    hints_with_rank.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.kind.cmp(&b.1.kind)));
    let preferred_check = hints_with_rank
        .first()
        .map(|(_, hint)| hint.command.clone());
    let check_hints = hints_with_rank.into_iter().map(|(_, hint)| hint).collect();

    TaskVerificationPlan {
        steps: vec![
            "Run at least one focused test-like command through anchor check before handoff."
                .to_string(),
            "If a check fails, fix the cause and rerun that same command successfully.".to_string(),
        ],
        preferred_check,
        check_hints,
    }
}

fn python_test_command(likely_tests: &[(&String, usize)]) -> Option<(usize, String)> {
    likely_tests
        .iter()
        .enumerate()
        .find_map(|(rank, (path, _))| {
            let path = path.as_str();
            if path.ends_with(".py") && is_runnable_python_test_path(path) {
                Some((rank, format!("python -m pytest {path}")))
            } else {
                None
            }
        })
}

fn go_test_command(likely_tests: &[(&String, usize)]) -> Option<(usize, String)> {
    likely_tests
        .iter()
        .enumerate()
        .find_map(|(rank, (path, _))| {
            if path.ends_with("_test.go") {
                Some((rank, format!("go test {}", test_package_arg(path))))
            } else {
                None
            }
        })
}

fn rust_test_command(likely_tests: &[(&String, usize)]) -> Option<(usize, String)> {
    likely_tests
        .iter()
        .enumerate()
        .find_map(|(rank, (path, _))| {
            if !path.starts_with("tests/") || !path.ends_with(".rs") {
                return None;
            }
            let Some(stem) = Path::new(path).file_stem().and_then(|stem| stem.to_str()) else {
                return None;
            };
            Some((rank, format!("cargo test --test {stem}")))
        })
}

fn js_test_command(likely_tests: &[(&String, usize)]) -> Option<(usize, String)> {
    likely_tests
        .iter()
        .enumerate()
        .find_map(|(rank, (path, _))| {
            let lower = path.to_ascii_lowercase();
            if is_runnable_javascript_test_path(&lower) {
                Some((rank, format!("npm test -- {}", path.as_str())))
            } else {
                None
            }
        })
}

fn test_package_arg(path: &str) -> String {
    let parent = Path::new(path)
        .parent()
        .and_then(|parent| parent.to_str())
        .unwrap_or("");
    if parent.is_empty() {
        ".".to_string()
    } else {
        format!("./{}", parent.replace('\\', "/"))
    }
}

fn print_verification_plan(plan: &TaskVerificationPlan) {
    println!("<verification_plan>");
    for step in &plan.steps {
        println!("  <step>{}</step>", escape_xml_text(step));
    }
    if let Some(command) = &plan.preferred_check {
        println!(
            "  <preferred_check command=\"{}\"/>",
            escape_xml_text(command)
        );
    }
    println!("</verification_plan>");
    println!("<check_hints>");
    for hint in &plan.check_hints {
        println!(
            "  <hint kind=\"{}\" command=\"{}\"/>",
            escape_xml_text(&hint.kind),
            escape_xml_text(&hint.command)
        );
    }
    println!("</check_hints>");
}

fn is_runnable_python_test_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let file_name = Path::new(&lower)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if file_name == "conftest.py" || file_name == "__init__.py" {
        return false;
    }
    file_name.starts_with("test_") || file_name.ends_with("_test.py")
}

fn is_runnable_javascript_test_path(lower_path: &str) -> bool {
    lower_path.ends_with(".test.ts")
        || lower_path.ends_with(".spec.ts")
        || lower_path.ends_with(".test.tsx")
        || lower_path.ends_with(".spec.tsx")
        || lower_path.ends_with(".test.js")
        || lower_path.ends_with(".spec.js")
        || lower_path.ends_with(".test.jsx")
        || lower_path.ends_with(".spec.jsx")
}

fn add_path_score(
    scores: &mut std::collections::BTreeMap<String, usize>,
    path: String,
    score: usize,
) {
    let entry = scores.entry(path).or_default();
    *entry += score;
}

fn path_signal_tokens(path: &str) -> std::collections::BTreeSet<String> {
    const GENERIC_PATH_TOKENS: &[&str] = &[
        "app", "bin", "core", "lib", "main", "mod", "package", "packages", "python", "src", "test",
        "tests",
    ];

    path.split(|ch: char| !ch.is_ascii_alphanumeric())
        .map(|part| part.to_ascii_lowercase())
        .filter(|part| part.len() >= 3)
        .filter(|part| !GENERIC_PATH_TOKENS.contains(&part.as_str()))
        .collect()
}
