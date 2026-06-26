/// Create a new file
pub fn create(root: &Path, path: &str, content: &str, expected_hash: Option<&str>) -> Result<()> {
    let full_path = resolve_path(root, path);
    let _lock = acquire_file_lock(root, &full_path, path)?;
    let before_hash = file_hash(&full_path);
    let before_text = file_text(&full_path);
    block_existing_source_write(root, &full_path, path)?;
    let expected_hash = expected_hash_from_recent_read(root, &full_path, path, expected_hash);
    verify_expected_hash(
        root,
        &full_path,
        path,
        before_hash.as_deref(),
        expected_hash.value.as_deref(),
        "write.guard",
        None,
    )?;
    enforce_read_requirement(root, &full_path, path, &expected_hash, "write.guard", None)?;
    record_write_attempt(root, &full_path, path, "create", None)?;

    // Create parent directories if needed
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    match crate::write::governed(|| {
        protect::with_unlocked_path(root, &full_path, || {
            create_file(&full_path, content).map_err(anyhow::Error::from)
        })
    }) {
        Ok(result) => {
            reindex_after_write(root, &full_path)?;
            let after_hash = file_hash(&full_path);
            let after_text = file_text(&full_path).unwrap_or_default();
            let change = line_change_summary(before_text.as_deref(), &after_text);
            let receipt = WriteReceipt {
                before_hash: before_hash.as_deref(),
                after_hash: after_hash.as_deref(),
                change: Some(&change),
                stats: WriteStats {
                    lines: result.lines_written,
                    bytes: result.bytes_written,
                    replacements: None,
                },
            };
            if let Ok(store) = AnchorStore::discover(root).or_else(|_| AnchorStore::init(root)) {
                events::record_with_meta(
                    store.anchor_root(),
                    "write.apply",
                    Some(lock_path(root, &full_path, path)),
                    None,
                    "ok",
                    Some(format!(
                        "created before={} after={} content={}",
                        before_hash.as_deref().unwrap_or("missing"),
                        after_hash.as_deref().unwrap_or("missing"),
                        content_hash_text(content)
                    )),
                    write_event_meta(
                        &expected_hash,
                        Some(content_hash_text(content)),
                        receipt,
                    ),
                );
            }
            print_compact_write_receipt("created", &result.path, receipt);
        }
        Err(e) => {
            if let Ok(store) = AnchorStore::discover(root).or_else(|_| AnchorStore::init(root)) {
                events::record(
                    store.anchor_root(),
                    "write.apply",
                    Some(lock_path(root, &full_path, path)),
                    None,
                    "error",
                    Some(e.to_string()),
                );
            }
            println!("<result>");
            println!("<status>error</status>");
            println!("<message>{}</message>", e);
            println!("</result>");
        }
    }
    Ok(())
}
