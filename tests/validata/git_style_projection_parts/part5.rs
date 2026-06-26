#[test]
#[ignore = "real VS Code corpus probe; run explicitly when /Volumes/Hak_SSD/vscode is available"]
fn real_vscode_many_symbol_projection_metrics() {
    let vscode_repo = std::env::var("ANCHOR_REAL_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/Volumes/Hak_SSD/vscode"));
    let root = vscode_repo.join("src/vs/workbench/browser");
    assert!(
        root.exists(),
        "missing VS Code checkout at {}",
        root.display()
    );

    let dir = tempdir().unwrap();
    let mut files = Vec::new();
    collect_ts_files(&root, &mut files, 120);

    let mut reductions = Vec::new();
    let mut full_bytes_total = 0usize;
    let mut projection_bytes_total = 0usize;
    let mut lock_conflicts_rejected = 0usize;
    let mut verified_after_edit = 0usize;
    let mut index_hash_refreshed = 0usize;
    let mut failures = 0usize;
    let target_symbols = 50usize;

    'files: for real_file in &files {
        let source = match fs::read_to_string(real_file) {
            Ok(source) => source,
            Err(_) => continue,
        };
        let extraction = match anchor::parser::extract_file(real_file, &source) {
            Ok(extraction) => extraction,
            Err(_) => continue,
        };

        for symbol in extraction.symbols {
            if reductions.len() >= target_symbols {
                break 'files;
            }
            if symbol.line_end <= symbol.line_start
                || symbol.code_snippet.len() < 40
                || symbol.code_snippet.len() > 5_000
                || !symbol.code_snippet.contains("{\n")
            {
                continue;
            }

            let case_dir = dir.path().join(format!("case_{}", reductions.len()));
            fs::create_dir_all(&case_dir).unwrap();
            let temp_file = case_dir.join("sample.ts");
            fs::write(&temp_file, &source).unwrap();
            let anchor_dir = case_dir.join(".anchor");

            let original_hash = match index_file(&anchor_dir, &temp_file) {
                Ok(hash) => hash,
                Err(_) => {
                    failures += 1;
                    continue;
                }
            };
            let hits = search_symbol(&anchor_dir, &symbol.name);
            if hits.len() != 1 {
                continue;
            }

            let projection = create_projection_from_hit(&hits[0]);
            let reduction = context_reduction_percent(source.len(), projection.text.len());
            let second_owner_blocked = acquire_lock(&anchor_dir, &projection, "agent-a").is_ok()
                && acquire_lock(&anchor_dir, &projection, "agent-b")
                    == Err(ApplyError::LockConflict);
            if second_owner_blocked {
                lock_conflicts_rejected += 1;
            }

            let edited_text =
                projection
                    .text
                    .replacen("{\n", "{\n\t// anchor projection corpus probe\n", 1);
            if apply_locked_projection(&anchor_dir, &projection, "agent-a", &edited_text).is_err() {
                failures += 1;
                continue;
            }

            if anchor::parser::extract_file(&temp_file, &fs::read_to_string(&temp_file).unwrap())
                .is_ok()
            {
                verified_after_edit += 1;
            } else {
                failures += 1;
                continue;
            }

            let updated_hash = match index_file(&anchor_dir, &temp_file) {
                Ok(hash) => hash,
                Err(_) => {
                    failures += 1;
                    continue;
                }
            };
            if updated_hash != original_hash {
                index_hash_refreshed += 1;
            }

            reductions.push(reduction);
            full_bytes_total += source.len();
            projection_bytes_total += projection.text.len();
        }
    }

    let metrics = CorpusMetrics {
        files_seen: files.len(),
        symbols_tested: reductions.len(),
        avg_context_reduction_percent: reductions.iter().sum::<f64>() / reductions.len() as f64,
        median_context_reduction_percent: percentile(&reductions, 0.50),
        p90_context_reduction_percent: percentile(&reductions, 0.90),
        min_context_reduction_percent: percentile(&reductions, 0.00),
        max_context_reduction_percent: percentile(&reductions, 1.00),
        avg_full_context_bytes: full_bytes_total as f64 / reductions.len() as f64,
        avg_projection_bytes: projection_bytes_total as f64 / reductions.len() as f64,
        lock_conflicts_rejected,
        verified_after_edit,
        index_hash_refreshed,
        failures,
    };

    eprintln!("anchor real vscode corpus metrics: {metrics:?}");
    assert!(metrics.files_seen >= 20);
    assert!(metrics.symbols_tested >= 20);
    assert!(metrics.avg_context_reduction_percent >= 80.0);
    assert!(metrics.median_context_reduction_percent >= 80.0);
    assert!(metrics.p90_context_reduction_percent >= metrics.median_context_reduction_percent);
    assert!(metrics.min_context_reduction_percent <= metrics.max_context_reduction_percent);
    assert!(metrics.avg_full_context_bytes > metrics.avg_projection_bytes);
    assert_eq!(metrics.lock_conflicts_rejected, metrics.symbols_tested);
    assert_eq!(metrics.verified_after_edit, metrics.symbols_tested);
    assert_eq!(metrics.index_hash_refreshed, metrics.symbols_tested);
    assert_eq!(metrics.failures, 0);
}
