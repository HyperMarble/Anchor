const CLI_FILE_LOCK: &str = "__file__";

/// Fail-closed mode. With `ANCHOR_STRICT=1`, governance gaps become refusals
/// instead of warnings: writes are blocked when lockd is unreachable and when
/// an existing source file has no recorded read for this session.
pub fn strict_mode() -> bool {
    std::env::var("ANCHOR_STRICT")
        .map(|value| matches!(value.as_str(), "1" | "true" | "on"))
        .unwrap_or(false)
}

struct CliLock {
    lock_symbol: String,
    event_symbol: Option<String>,
    path: String,
    acquired: bool,
    anchor_root: Option<PathBuf>,
}

impl Drop for CliLock {
    fn drop(&mut self) {
        if self.acquired {
            lockd::release(&self.lock_symbol, &self.path);
            if let Some(anchor_root) = &self.anchor_root {
                events::record(
                    anchor_root,
                    "lock.release",
                    Some(self.path.clone()),
                    self.event_symbol.clone(),
                    "ok",
                    None,
                );
            }
        }
    }
}

fn resolve_path(root: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn lock_path(root: &Path, path: &Path, requested: &str) -> String {
    path.strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| requested.trim_start_matches('/').replace('\\', "/"))
}

fn lock_event_root(root: &Path) -> Option<PathBuf> {
    AnchorStore::discover(root)
        .or_else(|_| AnchorStore::init(root))
        .ok()
        .map(|store| store.anchor_root().to_path_buf())
}

fn acquire_lock(
    root: &Path,
    path: &Path,
    requested: &str,
    lock_symbol: &str,
    event_symbol: Option<&str>,
) -> Result<CliLock> {
    let path = lock_path(root, path, requested);
    let anchor_root = lock_event_root(root);
    let event_symbol = event_symbol.map(|symbol| symbol.to_string());

    match lockd::acquire(lock_symbol, &path) {
        lockd::LockdResult::Acquired => {
            if let Some(anchor_root) = &anchor_root {
                events::record(
                    anchor_root,
                    "lock.acquire",
                    Some(path.clone()),
                    event_symbol.clone(),
                    "ok",
                    None,
                );
            }
            Ok(CliLock {
                lock_symbol: lock_symbol.to_string(),
                event_symbol,
                path,
                acquired: true,
                anchor_root,
            })
        }
        lockd::LockdResult::Blocked { owner, reason } => {
            if let Some(anchor_root) = &anchor_root {
                events::record(
                    anchor_root,
                    "lock.acquire",
                    Some(path),
                    event_symbol,
                    "blocked",
                    Some(format!("{owner}: {reason}")),
                );
            }
            anyhow::bail!("BLOCKED by {}: {}", owner, reason)
        }
        lockd::LockdResult::Unavailable => {
            if strict_mode() {
                if let Some(anchor_root) = &anchor_root {
                    events::record(
                        anchor_root,
                        "lock.acquire",
                        Some(path.clone()),
                        event_symbol,
                        "blocked",
                        Some("strict mode: lockd unavailable".to_string()),
                    );
                }
                println!("<result>");
                println!("<path>{}</path>", path);
                println!("<status>lockd_unavailable</status>");
                println!("<message>strict mode requires a reachable lockd before writes</message>");
                println!("</result>");
                anyhow::bail!("strict mode: lockd unavailable, refusing unlocked write");
            }
            if let Some(anchor_root) = &anchor_root {
                events::record(
                    anchor_root,
                    "lock.skip",
                    Some(path.clone()),
                    event_symbol.clone(),
                    "warn",
                    Some("lockd unavailable; proceeding without coordination".to_string()),
                );
            }
            Ok(CliLock {
                lock_symbol: lock_symbol.to_string(),
                event_symbol,
                path,
                acquired: false,
                anchor_root,
            })
        }
    }
}

fn acquire_file_lock(root: &Path, path: &Path, requested: &str) -> Result<CliLock> {
    acquire_lock(root, path, requested, CLI_FILE_LOCK, None)
}

fn symbol_lock_name(repo_path: &str, symbol: &str) -> String {
    format!(
        "sym:{}",
        content_hash(format!("{repo_path}\0{symbol}").as_bytes())
    )
}

fn reindex_after_write(root: &Path, path: &Path) -> Result<()> {
    let store = AnchorStore::discover(root).or_else(|_| AnchorStore::init(root))?;
    let _ = store.upsert_symbols_for_path(path)?;
    Ok(())
}

fn file_hash(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(|bytes| content_hash(&bytes))
}

fn file_text(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn content_hash_text(content: &str) -> String {
    content_hash(content.as_bytes())
}

fn block_existing_source_write(root: &Path, path: &Path, requested: &str) -> Result<()> {
    if !path.exists() || !is_source_path(path) {
        return Ok(());
    }

    let repo_path = lock_path(root, path, requested);
    if let Ok(store) = AnchorStore::discover(root).or_else(|_| AnchorStore::init(root)) {
        events::record(
            store.anchor_root(),
            "write.guard",
            Some(repo_path.clone()),
            None,
            "blocked",
            Some("existing source files must be changed through anchor edit".to_string()),
        );
    }

    println!("<result>");
    println!("<status>source_write_requires_edit</status>");
    println!("<path>{}</path>", repo_path);
    println!("<message>existing source files must be changed through anchor edit</message>");
    println!("</result>");
    bail!("existing source files must be changed through anchor edit");
}

