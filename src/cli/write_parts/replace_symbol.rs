/// Replace one indexed symbol in a file.
pub fn replace_symbol(
    root: &Path,
    path: &str,
    symbol: &str,
    content: &str,
    expected_hash: Option<&str>,
) -> Result<()> {
    let full_path = resolve_path(root, path);
    let store = AnchorStore::discover(root).or_else(|_| AnchorStore::init(root))?;
    let repo_path = lock_path(root, &full_path, path);
    let before_hash = file_hash(&full_path);
    let before_text = file_text(&full_path);
    let expected_hash = expected_hash_from_recent_read(root, &full_path, path, expected_hash);
    let index = store.load_symbol_index()?;
    let entry = index
        .symbols
        .iter()
        .find(|entry| entry.path == repo_path && entry.name == symbol)
        .ok_or_else(|| anyhow::anyhow!("symbol '{}' not found in {}", symbol, repo_path))?;

    let projection = store.create_projection(entry)?;
    let lock_symbol = symbol_lock_name(&repo_path, symbol);
    let _lock = acquire_lock(root, &full_path, path, &lock_symbol, Some(symbol))?;
    verify_expected_hash(
        root,
        &full_path,
        path,
        before_hash.as_deref(),
        expected_hash.value.as_deref(),
        "edit.guard",
        Some(symbol),
    )?;
    enforce_read_requirement(
        root,
        &full_path,
        path,
        &expected_hash,
        "edit.guard",
        Some(symbol),
    )?;
    record_write_attempt(root, &full_path, path, "replace_symbol", Some(symbol))?;
    let result = crate::write::governed(|| {
        protect::with_unlocked_path(root, &full_path, || {
            replace_range(
                &full_path,
                projection.line_start,
                projection.line_end,
                content,
            )
            .map_err(anyhow::Error::from)
        })
    })?;
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
    events::record_with_meta(
        store.anchor_root(),
        "edit.apply",
        Some(repo_path.clone()),
        Some(symbol.to_string()),
        "ok",
        Some(format!(
            "symbol_replaced before={} after={} content={}",
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

    println!("<result>");
    println!("<path>{}</path>", result.path);
    println!("<status>symbol_replaced</status>");
    println!("<symbol>{}</symbol>", symbol);
    println!("<line_start>{}</line_start>", projection.line_start);
    println!("<line_end>{}</line_end>", projection.line_end);
    if let Some(before_hash) = &before_hash {
        println!("<before_hash>{}</before_hash>", before_hash);
    }
    if let Some(after_hash) = &after_hash {
        println!("<after_hash>{}</after_hash>", after_hash);
    }
    println!(
        "<changed_range start=\"{}\" old_end=\"{}\" new_end=\"{}\" old_lines=\"{}\" new_lines=\"{}\"/>",
        change.start_line,
        change.old_end_line,
        change.new_end_line,
        change.old_changed_lines,
        change.new_changed_lines
    );
    println!(
        "<content_hash>{}</content_hash>",
        content_hash_text(content)
    );
    println!("<lines>{}</lines>", result.lines_written);
    println!("<bytes>{}</bytes>", result.bytes_written);
    println!("</result>");
    Ok(())
}
