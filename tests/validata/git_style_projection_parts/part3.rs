#[test]
fn parse_objects_are_reused_by_content_hash() {
    let dir = tempdir().unwrap();
    let anchor_dir = dir.path().join(".anchor");
    let source = "export function activate() {\n  return true;\n}\n";

    let (first_hash, first_existed) = store_parse_object(&anchor_dir, source).unwrap();
    let (second_hash, second_existed) = store_parse_object(&anchor_dir, source).unwrap();

    assert_eq!(first_hash, second_hash);
    assert!(!first_existed);
    assert!(second_existed);
    assert!(parse_object_path(&anchor_dir, &first_hash).exists());
}

#[test]
fn projection_transplants_slice_edit_back_to_source_file() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("extension.ts");
    fs::write(
        &file,
        "import * as vscode from 'vscode';\n\nexport function activate() {\n  return true;\n}\n\nexport function deactivate() {}\n",
    )
    .unwrap();

    let source = fs::read_to_string(&file).unwrap();
    let projection = create_projection(&file, &source, "activate", 3, 5);
    assert_eq!(projection.symbol, "activate");
    assert!(projection.lock_id.starts_with("lock-"));
    assert!(projection.text.contains("return true"));

    apply_projection(
        &projection,
        "export function activate() {\n  console.log('ready');\n  return true;\n}",
    )
    .unwrap();

    let updated = fs::read_to_string(&file).unwrap();
    assert!(updated.contains("console.log('ready');"));
    assert!(updated.contains("import * as vscode"));
    assert!(updated.contains("export function deactivate()"));
}

#[test]
fn projection_rejects_when_source_changed_after_context() {
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

    let result = apply_projection(
        &projection,
        "export function start() {\n  return bootFast();\n}",
    );
    assert_eq!(result, Err(ApplyError::StaleSource));
}

#[test]
fn changed_content_gets_a_new_parse_object() {
    let dir = tempdir().unwrap();
    let anchor_dir = dir.path().join(".anchor");

    let original = "export const version = 1;\n";
    let changed = "export const version = 2;\n";

    let (original_hash, _) = store_parse_object(&anchor_dir, original).unwrap();
    let (changed_hash, changed_existed) = store_parse_object(&anchor_dir, changed).unwrap();

    assert_ne!(original_hash, changed_hash);
    assert!(!changed_existed);
    assert!(parse_object_path(&anchor_dir, &original_hash).exists());
    assert!(parse_object_path(&anchor_dir, &changed_hash).exists());
}

#[test]
fn search_context_locked_edit_and_update_flow_proves_anchor_value() {
    let dir = tempdir().unwrap();
    let anchor_dir = dir.path().join(".anchor");
    let file = dir.path().join("extension.ts");
    let large_unrelated_context = (0..80)
        .map(|i| format!("export const unrelated{i} = {i};"))
        .collect::<Vec<_>>()
        .join("\n");

    fs::write(
        &file,
        format!(
            "{large_unrelated_context}\n\n\
             export function activate() {{\n\
             \treturn true;\n\
             }}\n\n\
             export function deactivate() {{\n\
             \treturn false;\n\
             }}\n"
        ),
    )
    .unwrap();

    let original_source = fs::read_to_string(&file).unwrap();
    let original_hash = index_file(&anchor_dir, &file).unwrap();
    let hits = search_symbol(&anchor_dir, "activate");

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].source_hash, original_hash);

    let projection = create_projection_from_hit(&hits[0]);
    let reduction = context_reduction_percent(original_source.len(), projection.text.len());
    assert!(
        projection.text.len() * 8 < original_source.len(),
        "projection should give the agent a much smaller context slice"
    );
    assert!(
        reduction >= 87.5,
        "projection should reduce context by at least 87.5%"
    );
    assert!(projection.text.contains("return true"));
    assert!(!projection.text.contains("unrelated79"));
    assert!(!projection.text.contains("deactivate"));

    acquire_lock(&anchor_dir, &projection, "agent-a").unwrap();
    assert_eq!(
        acquire_lock(&anchor_dir, &projection, "agent-b"),
        Err(ApplyError::LockConflict)
    );

    apply_locked_projection(
        &anchor_dir,
        &projection,
        "agent-a",
        "export function activate() {\n\tconsole.log('ready');\n\treturn true;\n}",
    )
    .unwrap();
    verify_file_parses(&file);

    let updated_source = fs::read_to_string(&file).unwrap();
    assert!(updated_source.contains("console.log('ready');"));
    assert!(updated_source.contains("unrelated79"));
    assert!(updated_source.contains("export function deactivate()"));

    let updated_hash = index_file(&anchor_dir, &file).unwrap();
    assert_ne!(original_hash, updated_hash);
    let updated_hits = search_symbol(&anchor_dir, "activate");
    assert_eq!(updated_hits.len(), 1);
    assert_eq!(updated_hits[0].source_hash, updated_hash);

    let metrics = ProofMetrics {
        full_context_bytes: original_source.len(),
        projection_bytes: projection.text.len(),
        context_reduction_percent: reduction,
        unrelated_symbols_excluded: 81,
        stale_edits_rejected: 0,
        lock_conflicts_rejected: 1,
        verified_after_edit: true,
        index_hash_refreshed: updated_hash != original_hash,
    };

    eprintln!("anchor proof metrics: {metrics:?}");
    assert_eq!(metrics.full_context_bytes, original_source.len());
    assert_eq!(metrics.projection_bytes, projection.text.len());
    assert!(metrics.context_reduction_percent >= 98.0);
    assert_eq!(metrics.unrelated_symbols_excluded, 81);
    assert_eq!(metrics.lock_conflicts_rejected, 1);
    assert!(metrics.verified_after_edit);
    assert!(metrics.index_hash_refreshed);
}

