#[test]
#[ignore = "real MLflow corpus benchmark; run explicitly when /Volumes/Hak_SSD/mlflow is available"]
fn real_mlflow_anchor_store_projection_benchmark() {
    let mlflow_repo = std::env::var("ANCHOR_REAL_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/Volumes/Hak_SSD/mlflow"));
    let root = mlflow_repo.join("mlflow");
    assert!(
        root.exists(),
        "missing MLflow checkout at {}",
        root.display()
    );

    let dir = tempdir().unwrap();
    let store = AnchorStore::init(dir.path()).unwrap();
    let mut real_files = Vec::new();
    collect_python_files(&root, &mut real_files, 160);

    let mut reductions = Vec::new();
    let mut full_bytes_total = 0usize;
    let mut projection_bytes_total = 0usize;
    let mut stale_rejections = 0usize;
    let mut failures = 0usize;
    let target_symbols = 50usize;

    'files: for real_file in &real_files {
        let source = match fs::read_to_string(real_file) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let extraction = match anchor::parser::extract_file(real_file, &source) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let relative = real_file.strip_prefix(&root).unwrap();
        let temp_file = dir.path().join(relative);
        fs::create_dir_all(temp_file.parent().unwrap()).unwrap();
        fs::write(&temp_file, &source).unwrap();
        store.upsert_symbols_for_path(&temp_file).unwrap();

        for symbol in extraction.symbols {
            if reductions.len() >= target_symbols {
                break 'files;
            }
            if symbol.line_end <= symbol.line_start || symbol.code_snippet.len() < 40 {
                continue;
            }

            let relative_text = relative.to_string_lossy().to_string();
            let hits = store.search_symbols(&symbol.name, 100).unwrap();
            let Some(hit) = hits.iter().find(|h| {
                h.path.ends_with(&relative_text)
                    && h.line_start == symbol.line_start
                    && h.line_end == symbol.line_end
            }) else {
                failures += 1;
                continue;
            };

            let projection = match store.create_projection(hit) {
                Ok(p) => p,
                Err(_) => {
                    failures += 1;
                    continue;
                }
            };
            reductions.push(context_reduction_percent(
                source.len(),
                projection.text.len(),
            ));
            full_bytes_total += source.len();
            projection_bytes_total += projection.text.len();

            fs::write(&temp_file, format!("{source}\n# anchor stale probe\n")).unwrap();
            if store.create_projection(hit).is_err() {
                stale_rejections += 1;
            } else {
                failures += 1;
            }
            fs::write(&temp_file, &source).unwrap();
        }
    }

    let metrics = StoreProjectionBenchmark {
        files_seen: real_files.len(),
        symbols_tested: reductions.len(),
        avg_context_reduction_percent: reductions.iter().sum::<f64>() / reductions.len() as f64,
        median_context_reduction_percent: percentile(&reductions, 0.50),
        p90_context_reduction_percent: percentile(&reductions, 0.90),
        min_context_reduction_percent: percentile(&reductions, 0.00),
        max_context_reduction_percent: percentile(&reductions, 1.00),
        avg_full_context_bytes: full_bytes_total as f64 / reductions.len() as f64,
        avg_projection_bytes: projection_bytes_total as f64 / reductions.len() as f64,
        stale_rejections,
        failures,
    };

    eprintln!("anchor store real mlflow projection metrics: {metrics:?}");
    assert!(metrics.files_seen >= 20);
    assert_eq!(metrics.symbols_tested, target_symbols);
    assert!(metrics.avg_context_reduction_percent >= 80.0);
    assert!(metrics.median_context_reduction_percent >= 80.0);
    assert!(metrics.p90_context_reduction_percent >= metrics.median_context_reduction_percent);
    assert!(metrics.min_context_reduction_percent <= metrics.max_context_reduction_percent);
    assert!(metrics.avg_full_context_bytes > metrics.avg_projection_bytes);
    assert_eq!(metrics.stale_rejections, metrics.symbols_tested);
    assert_eq!(metrics.failures, 0);
}

#[test]
fn discover_stops_at_git_boundary_instead_of_escaping_to_ancestor_store() {
    use anchor::storage::AnchorStore;
    use std::fs;

    let outer = tempfile::tempdir().unwrap();
    // stray store above the repo, like a forgotten .anchor at a mount root
    fs::create_dir_all(outer.path().join(".anchor")).unwrap();
    let repo = outer.path().join("repo");
    fs::create_dir_all(repo.join(".git")).unwrap();
    fs::create_dir_all(repo.join("src")).unwrap();

    let result = AnchorStore::discover(&repo);
    assert!(
        result.is_err(),
        "a git repo without its own store must not attach to an ancestor store"
    );

    // without a .git boundary the ancestor store is still discoverable
    let plain = outer.path().join("plain");
    fs::create_dir_all(&plain).unwrap();
    let found = AnchorStore::discover(&plain);
    assert!(found.is_ok(), "non-repo directories may still walk up");
}
