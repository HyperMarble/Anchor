/// Replace text in files (supports glob patterns)
pub fn replace(
    root: &Path,
    pattern: &str,
    old: &str,
    new: &str,
    expected_hash: Option<&str>,
) -> Result<()> {
    let paths = expand_glob(root, pattern)?;

    if paths.is_empty() {
        println!("<result>");
        println!("<status>no_match</status>");
        println!("<pattern>{}</pattern>", pattern);
        println!("</result>");
        return Ok(());
    }

    if paths.len() == 1 {
        // Single file
        let _lock = acquire_file_lock(root, &paths[0], pattern)?;
        let before_hash = file_hash(&paths[0]);
        let before_text = file_text(&paths[0]);
        let expected_hash = expected_hash_from_recent_read(root, &paths[0], pattern, expected_hash);
        verify_expected_hash(
            root,
            &paths[0],
            pattern,
            before_hash.as_deref(),
            expected_hash.value.as_deref(),
            "edit.guard",
            None,
        )?;
        enforce_read_requirement(root, &paths[0], pattern, &expected_hash, "edit.guard", None)?;
        record_write_attempt(root, &paths[0], pattern, "replace", None)?;
        match crate::write::governed(|| {
            protect::with_unlocked_path(root, &paths[0], || {
                replace_all(&paths[0], old, new).map_err(anyhow::Error::from)
            })
        }) {
            Ok(result) => {
                reindex_after_write(root, &paths[0])?;
                let after_hash = file_hash(&paths[0]);
                let after_text = file_text(&paths[0]).unwrap_or_default();
                let change = line_change_summary(before_text.as_deref(), &after_text);
                let count = result.replacements.unwrap_or(0);
                let receipt = WriteReceipt {
                    before_hash: before_hash.as_deref(),
                    after_hash: after_hash.as_deref(),
                    change: Some(&change),
                    stats: WriteStats {
                        lines: result.lines_written,
                        bytes: result.bytes_written,
                        replacements: Some(count),
                    },
                };
                if let Ok(store) = AnchorStore::discover(root).or_else(|_| AnchorStore::init(root))
                {
                    events::record_with_meta(
                        store.anchor_root(),
                        "edit.apply",
                        Some(lock_path(root, &paths[0], pattern)),
                        None,
                        "ok",
                        Some(format!(
                            "replaced before={} after={} old={} new={}",
                            before_hash.as_deref().unwrap_or("missing"),
                            after_hash.as_deref().unwrap_or("missing"),
                            content_hash_text(old),
                            content_hash_text(new)
                        )),
                        write_event_meta(
                            &expected_hash,
                            Some(content_hash_text(new)),
                            receipt,
                        ),
                    );
                }
                print_compact_write_receipt("replaced", &result.path, receipt);
            }
            Err(e) => {
                if let Ok(store) = AnchorStore::discover(root).or_else(|_| AnchorStore::init(root))
                {
                    events::record(
                        store.anchor_root(),
                        "edit.apply",
                        Some(lock_path(root, &paths[0], pattern)),
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
    } else {
        if expected_hash.is_some() {
            anyhow::bail!("--expect-hash is only supported for single-file edits");
        }
        // Batch replace
        let mut locks = Vec::with_capacity(paths.len());
        for path in &paths {
            locks.push(acquire_file_lock(root, path, &path.to_string_lossy())?);
        }
        for path in &paths {
            let requested = path.to_string_lossy().to_string();
            let expected = expected_hash_from_recent_read(root, path, &requested, None);
            verify_expected_hash(
                root,
                path,
                &requested,
                file_hash(path).as_deref(),
                expected.value.as_deref(),
                "edit.guard",
                None,
            )?;
            enforce_read_requirement(root, path, &requested, &expected, "edit.guard", None)?;
            record_write_attempt(root, path, &requested, "batch_replace", None)?;
        }
        let before_hashes: std::collections::HashMap<String, String> = paths
            .iter()
            .filter_map(|path| {
                file_hash(path).map(|hash| (path.to_string_lossy().to_string(), hash))
            })
            .collect();
        let results = crate::write::governed(|| {
            protect::with_unlocked_paths(root, &paths, || Ok(batch_replace_all(&paths, old, new)))
        })?;
        let summary = BatchWriteResult::from_results(results);
        for result in &summary.results {
            reindex_after_write(root, Path::new(&result.path))?;
            let after_hash = file_hash(Path::new(&result.path));
            if let Ok(store) = AnchorStore::discover(root).or_else(|_| AnchorStore::init(root)) {
                events::record(
                    store.anchor_root(),
                    "edit.apply",
                    Some(result.path.clone()),
                    None,
                    "ok",
                    Some(format!(
                        "batch_replaced before={} after={} old={} new={}",
                        before_hashes
                            .get(&result.path)
                            .map(String::as_str)
                            .unwrap_or("missing"),
                        after_hash.as_deref().unwrap_or("missing"),
                        content_hash_text(old),
                        content_hash_text(new)
                    )),
                );
            }
        }

        let total_replacements: usize = summary.results.iter().filter_map(|r| r.replacements).sum();

        println!("<result>");
        println!("<status>batch_replaced</status>");
        println!("<total_files>{}</total_files>", summary.total_files);
        println!("<successful>{}</successful>", summary.successful);
        println!("<failed>{}</failed>", summary.failed);
        println!(
            "<total_replacements>{}</total_replacements>",
            total_replacements
        );
        println!("<time_ms>{}</time_ms>", summary.total_time_ms);
        println!("<old_hash>{}</old_hash>", content_hash_text(old));
        println!("<new_hash>{}</new_hash>", content_hash_text(new));
        println!("<files>");
        for result in &summary.results {
            if let Some(count) = result.replacements {
                let after_hash = file_hash(Path::new(&result.path));
                println!(
                    "<file path=\"{}\" replacements=\"{}\" after_hash=\"{}\"/>",
                    result.path,
                    count,
                    after_hash.as_deref().unwrap_or("missing")
                );
            }
        }
        println!("</files>");
        println!("</result>");
    }
    Ok(())
}
