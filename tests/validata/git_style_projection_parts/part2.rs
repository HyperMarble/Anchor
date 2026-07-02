fn create_projection(
    source_path: &Path,
    source: &str,
    symbol: &str,
    line_start: usize,
    line_end: usize,
) -> Projection {
    let lines: Vec<&str> = source.lines().collect();
    assert!(line_start >= 1);
    assert!(line_end >= line_start);
    assert!(line_end <= lines.len());

    let slice = lines[line_start - 1..line_end].join("\n");
    let prefix = lines[..line_start - 1].join("\n");
    let suffix = lines[line_end..].join("\n");

    Projection {
        source_path: source_path.to_path_buf(),
        source_hash: content_hash(source.as_bytes()),
        symbol: symbol.to_string(),
        line_start,
        line_end,
        slice_hash: content_hash(slice.as_bytes()),
        prefix_hash: content_hash(prefix.as_bytes()),
        suffix_hash: content_hash(suffix.as_bytes()),
        lock_id: format!("lock-{}", content_hash(symbol.as_bytes())),
        text: slice,
    }
}

fn create_projection_from_hit(hit: &SearchHit) -> Projection {
    let source = fs::read_to_string(&hit.source_path).unwrap();
    assert_eq!(content_hash(source.as_bytes()), hit.source_hash);
    create_projection(
        &hit.source_path,
        &source,
        &hit.symbol,
        hit.line_start,
        hit.line_end,
    )
}

fn acquire_lock(anchor_dir: &Path, projection: &Projection, owner: &str) -> Result<(), ApplyError> {
    let path = lock_path(anchor_dir, &projection.lock_id);
    if path.exists() {
        let raw = fs::read_to_string(path).map_err(|_| ApplyError::MissingLock)?;
        let lock: Value = serde_json::from_str(&raw).map_err(|_| ApplyError::MissingLock)?;
        if lock["owner"].as_str() != Some(owner) {
            return Err(ApplyError::LockConflict);
        }
        return Ok(());
    }

    fs::create_dir_all(path.parent().unwrap()).map_err(|_| ApplyError::MissingLock)?;
    fs::write(
        path,
        serde_json::to_string_pretty(&json!({
            "id": projection.lock_id,
            "owner": owner,
            "path": projection.source_path,
            "symbol": projection.symbol,
            "source_hash": projection.source_hash,
            "line_start": projection.line_start,
            "line_end": projection.line_end,
        }))
        .unwrap(),
    )
    .map_err(|_| ApplyError::MissingLock)
}

fn assert_lock_owner(
    anchor_dir: &Path,
    projection: &Projection,
    owner: &str,
) -> Result<(), ApplyError> {
    let path = lock_path(anchor_dir, &projection.lock_id);
    if !path.exists() {
        return Err(ApplyError::MissingLock);
    }

    let raw = fs::read_to_string(path).map_err(|_| ApplyError::MissingLock)?;
    let lock: Value = serde_json::from_str(&raw).map_err(|_| ApplyError::MissingLock)?;
    if lock["owner"].as_str() != Some(owner) {
        return Err(ApplyError::LockConflict);
    }

    Ok(())
}

fn apply_projection(projection: &Projection, edited_text: &str) -> Result<(), ApplyError> {
    let current =
        fs::read_to_string(&projection.source_path).map_err(|_| ApplyError::InvalidRange)?;
    if content_hash(current.as_bytes()) != projection.source_hash {
        return Err(ApplyError::StaleSource);
    }

    let lines: Vec<&str> = current.lines().collect();
    if projection.line_start < 1
        || projection.line_end < projection.line_start
        || projection.line_end > lines.len()
    {
        return Err(ApplyError::InvalidRange);
    }

    let current_slice = lines[projection.line_start - 1..projection.line_end].join("\n");
    if content_hash(current_slice.as_bytes()) != projection.slice_hash {
        return Err(ApplyError::StaleSlice);
    }

    let prefix = lines[..projection.line_start - 1].join("\n");
    let suffix = lines[projection.line_end..].join("\n");
    if content_hash(prefix.as_bytes()) != projection.prefix_hash
        || content_hash(suffix.as_bytes()) != projection.suffix_hash
    {
        return Err(ApplyError::StaleSource);
    }

    let mut next = String::new();
    if !prefix.is_empty() {
        next.push_str(&prefix);
        next.push('\n');
    }
    next.push_str(edited_text.trim_end_matches('\n'));
    if !suffix.is_empty() {
        next.push('\n');
        next.push_str(&suffix);
    }
    if current.ends_with('\n') {
        next.push('\n');
    }

    fs::write(&projection.source_path, next).map_err(|_| ApplyError::InvalidRange)
}

fn apply_locked_projection(
    anchor_dir: &Path,
    projection: &Projection,
    owner: &str,
    edited_text: &str,
) -> Result<(), ApplyError> {
    assert_lock_owner(anchor_dir, projection, owner)?;
    apply_projection(projection, edited_text)
}

fn verify_file_parses(source_path: &Path) {
    let source = fs::read_to_string(source_path).unwrap();
    anchor::parser::extract_file(source_path, &source).unwrap();
}

fn context_reduction_percent(full_bytes: usize, projection_bytes: usize) -> f64 {
    assert!(full_bytes > 0);
    100.0 - ((projection_bytes as f64 / full_bytes as f64) * 100.0)
}

fn percentile(values: &[f64], percentile: f64) -> f64 {
    assert!(!values.is_empty());
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
    sorted[index]
}

fn collect_ts_files(root: &Path, out: &mut Vec<PathBuf>, max_files: usize) {
    if out.len() >= max_files {
        return;
    }

    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        if out.len() >= max_files {
            return;
        }

        let path = entry.path();
        if path.is_dir() {
            collect_ts_files(&path, out, max_files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("ts")
            && !path.to_string_lossy().ends_with(".d.ts")
        {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if (5_000..=90_000).contains(&meta.len()) {
                out.push(path);
            }
        }
    }
}

