#[test]
fn stale_edit_rejection_has_a_countable_safety_metric() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("service.ts");
    fs::write(&file, "export function start() {\n  return boot();\n}\n").unwrap();

    let source = fs::read_to_string(&file).unwrap();
    let projection = create_projection(&file, &source, "start", 1, 3);

    fs::write(
        &file,
        "export function start() {\n  audit();\n  return boot();\n}\n",
    )
    .unwrap();

    let rejected = apply_projection(
        &projection,
        "export function start() {\n  return bootFast();\n}",
    ) == Err(ApplyError::StaleSource);

    let metrics = ProofMetrics {
        full_context_bytes: source.len(),
        projection_bytes: projection.text.len(),
        context_reduction_percent: context_reduction_percent(source.len(), projection.text.len()),
        unrelated_symbols_excluded: 0,
        stale_edits_rejected: usize::from(rejected),
        lock_conflicts_rejected: 0,
        verified_after_edit: false,
        index_hash_refreshed: false,
    };

    eprintln!("anchor stale-edit safety metrics: {metrics:?}");
    assert_eq!(metrics.full_context_bytes, source.len());
    assert_eq!(metrics.projection_bytes, projection.text.len());
    assert!(metrics.context_reduction_percent >= 0.0);
    assert_eq!(metrics.unrelated_symbols_excluded, 0);
    assert_eq!(metrics.stale_edits_rejected, 1);
}

#[test]
#[ignore = "real VS Code repo probe; run explicitly when /Volumes/Hak_SSD/vscode is available"]
fn real_vscode_file_projection_metrics() {
    let vscode_repo = std::env::var("ANCHOR_REAL_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/Volumes/Hak_SSD/vscode"));
    let real_file = vscode_repo.join("src/vs/workbench/browser/dnd.ts");
    assert!(
        real_file.exists(),
        "missing VS Code checkout at {}",
        real_file.display()
    );

    let dir = tempdir().unwrap();
    let anchor_dir = dir.path().join(".anchor");
    let file = dir.path().join("dnd.ts");
    fs::copy(&real_file, &file).unwrap();

    let original_source = fs::read_to_string(&file).unwrap();
    let original_hash = index_file(&anchor_dir, &file).unwrap();
    let hits = search_symbol(&anchor_dir, "extractTreeDropData");
    assert_eq!(hits.len(), 1);

    let projection = create_projection_from_hit(&hits[0]);
    let reduction = context_reduction_percent(original_source.len(), projection.text.len());
    assert!(projection.text.contains("extractTreeDropData"));
    assert!(!projection
        .text
        .contains("export class ResourcesDropHandler"));

    acquire_lock(&anchor_dir, &projection, "agent-a").unwrap();
    let edited_text = projection.text.replacen(
        "{\n",
        "{\n\tconst anchorProjectionProbe = true;\n\tvoid anchorProjectionProbe;\n",
        1,
    );
    apply_locked_projection(&anchor_dir, &projection, "agent-a", &edited_text).unwrap();
    verify_file_parses(&file);

    let updated_hash = index_file(&anchor_dir, &file).unwrap();
    let updated_hits = search_symbol(&anchor_dir, "extractTreeDropData");
    assert_eq!(updated_hits.len(), 1);
    assert_eq!(updated_hits[0].source_hash, updated_hash);

    let metrics = ProofMetrics {
        full_context_bytes: original_source.len(),
        projection_bytes: projection.text.len(),
        context_reduction_percent: reduction,
        unrelated_symbols_excluded: search_symbol(&anchor_dir, "ResourcesDropHandler").len(),
        stale_edits_rejected: 0,
        lock_conflicts_rejected: 0,
        verified_after_edit: true,
        index_hash_refreshed: updated_hash != original_hash,
    };

    eprintln!("anchor real vscode metrics: {metrics:?}");
    assert!(metrics.context_reduction_percent > 90.0);
    assert!(metrics.verified_after_edit);
    assert!(metrics.index_hash_refreshed);
}

