/// Rotate the log once it crosses this size so per-edit costs stay bounded on
/// long sessions. Rotated segments keep full provenance on disk.
const MAX_LOG_BYTES: u64 = 16 * 1024 * 1024;

pub fn append(anchor_root: &Path, event: &ExecutionEvent) -> anyhow::Result<()> {
    let dir = anchor_root.join("events");
    fs::create_dir_all(&dir)?;
    let path = dir.join("events.jsonl");
    let _guard = EventLogLock::acquire(&dir)?;
    rotate_if_oversized(&dir, &path, event.timestamp_ms)?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let line = format!("{}\n", serde_json::to_string(event)?);
    file.write_all(line.as_bytes())?;
    update_read_hash_index(&dir, event);
    Ok(())
}

fn rotate_if_oversized(dir: &Path, path: &Path, timestamp_ms: u128) -> anyhow::Result<()> {
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(());
    };
    if metadata.len() < MAX_LOG_BYTES {
        return Ok(());
    }
    let rotated = dir.join(format!("events-{}.jsonl", timestamp_ms));
    fs::rename(path, rotated)?;
    Ok(())
}

/// Compact index of the last source hash each (session, agent) read per path.
/// Lets the write guard answer "what did this agent last read?" without
/// re-scanning the whole event log on every edit.
fn read_hash_index_path(dir: &Path) -> PathBuf {
    dir.join("read_hashes.json")
}

fn read_hash_key(session_id: &str, agent_id: &str, path: &str) -> String {
    format!("{}\u{1f}{}\u{1f}{}", session_id, agent_id, path)
}

fn update_read_hash_index(dir: &Path, event: &ExecutionEvent) {
    if event.event_type != "context.read" || !matches!(event.status.as_str(), "ok" | "cached") {
        return;
    }
    let (Some(path), Some(source_hash)) = (&event.path, event.meta.get("source_hash")) else {
        return;
    };
    let index_path = read_hash_index_path(dir);
    let mut index: BTreeMap<String, String> = fs::read_to_string(&index_path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    index.insert(
        read_hash_key(&event.session_id, &event.agent_id, path),
        source_hash.clone(),
    );
    if let Ok(raw) = serde_json::to_vec(&index) {
        if let Err(err) = fs::write(&index_path, raw) {
            eprintln!(
                "anchor: failed to update read-hash index {}: {err}",
                index_path.display()
            );
        }
    }
}

/// Last source hash a (session, agent) read for `repo_path`, served from the
/// compact index. Falls back to a full log scan only when the index file does
/// not exist yet (logs written by older Anchor versions).
pub fn last_read_hash(
    anchor_root: &Path,
    session_id: &str,
    agent_id: &str,
    repo_path: &str,
) -> Option<String> {
    let dir = anchor_root.join("events");
    let index_path = read_hash_index_path(&dir);
    if index_path.exists() {
        let index: BTreeMap<String, String> =
            serde_json::from_str(&fs::read_to_string(&index_path).ok()?).ok()?;
        return index
            .get(&read_hash_key(session_id, agent_id, repo_path))
            .cloned();
    }

    let events = load(anchor_root).ok()?;
    events
        .iter()
        .rev()
        .find(|event| {
            event.event_type == "context.read"
                && matches!(event.status.as_str(), "ok" | "cached")
                && event.path.as_deref() == Some(repo_path)
                && event.session_id == session_id
                && event.agent_id == agent_id
        })
        .and_then(|event| event.meta.get("source_hash").cloned())
}
