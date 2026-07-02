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
    ensure_indexed_store_with_refresh(root, Some(true))
}

fn ensure_query_indexed_store(root: &Path) -> Result<AnchorStore> {
    ensure_indexed_store_with_refresh(root, None)
}

fn ensure_indexed_store_with_refresh(root: &Path, refresh_new_files: Option<bool>) -> Result<AnchorStore> {
    let store = open_store(root)?;
    let symbol_index = store.load_symbol_index();
    let path_index_unreadable = store.path_index_path().exists() && store.load_path_index().is_err();
    let symbol_index_unreadable = store.symbol_index_path().exists() && symbol_index.is_err();
    let missing_or_empty = !store.path_index_path().exists()
        || !store.symbol_index_path().exists()
        || !store.call_index_path().exists()
        || !store.history_index_path().exists()
        || path_index_unreadable
        || symbol_index_unreadable
        || symbol_index
            .map(|index| index.symbols.is_empty())
            .unwrap_or(true);
    if missing_or_empty {
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
    } else if let Some(discover_new) = refresh_new_files {
        let refresh = if discover_new {
            refresh_stale_index_paths(root, &store)?
        } else {
            IndexRefresh::Clean
        };
        match refresh {
            IndexRefresh::Clean => {}
            IndexRefresh::Incremental { refreshed } => {
                events::record(
                    store.anchor_root(),
                    "index.refresh",
                    None,
                    None,
                    "ok",
                    Some(format!("incremental refreshed={refreshed}")),
                );
            }
        }
    }

    open_store(root)
}
