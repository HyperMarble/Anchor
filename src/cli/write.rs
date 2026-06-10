//
//  write.rs
//  Anchor
//
//  Created by hak (tharun)
//

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

use crate::cli::protect;
use crate::events;
use crate::lock::lockd;
use crate::parser::language::is_source_path;
use crate::storage::{content_hash, AnchorStore};
use crate::write::{
    batch_replace_all, create_file, insert_after, replace_all, replace_range, BatchWriteResult,
};

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

struct ExpectedHash {
    value: Option<String>,
    source: &'static str,
}

fn expected_hash_from_recent_read(
    root: &Path,
    path: &Path,
    requested: &str,
    provided_hash: Option<&str>,
) -> ExpectedHash {
    if let Some(provided_hash) = provided_hash {
        return ExpectedHash {
            value: Some(provided_hash.to_string()),
            source: "provided",
        };
    }

    let Some(store) = AnchorStore::discover(root)
        .or_else(|_| AnchorStore::init(root))
        .ok()
    else {
        return ExpectedHash {
            value: None,
            source: "none",
        };
    };
    let repo_path = lock_path(root, path, requested);
    let session_id = std::env::var("ANCHOR_SESSION_ID").unwrap_or_else(|_| "local".into());
    let agent_id = lockd::agent_id().to_string();
    let value = events::last_read_hash(store.anchor_root(), &session_id, &agent_id, &repo_path);

    if value.is_some() {
        ExpectedHash {
            value,
            source: "context",
        }
    } else {
        ExpectedHash {
            value: None,
            source: "none",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChangeSummary {
    start_line: usize,
    old_end_line: usize,
    new_end_line: usize,
    old_changed_lines: usize,
    new_changed_lines: usize,
}

fn line_change_summary(before: Option<&str>, after: &str) -> ChangeSummary {
    let before_lines: Vec<&str> = before.unwrap_or("").lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();

    let mut prefix = 0;
    while prefix < before_lines.len()
        && prefix < after_lines.len()
        && before_lines[prefix] == after_lines[prefix]
    {
        prefix += 1;
    }

    let mut suffix = 0;
    while suffix < before_lines.len().saturating_sub(prefix)
        && suffix < after_lines.len().saturating_sub(prefix)
        && before_lines[before_lines.len() - 1 - suffix]
            == after_lines[after_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let old_end_line = before_lines.len().saturating_sub(suffix);
    let new_end_line = after_lines.len().saturating_sub(suffix);
    let old_changed_lines = old_end_line.saturating_sub(prefix);
    let new_changed_lines = new_end_line.saturating_sub(prefix);

    ChangeSummary {
        start_line: prefix + 1,
        old_end_line,
        new_end_line,
        old_changed_lines,
        new_changed_lines,
    }
}

/// Strict-mode read requirement: an existing source file may only be changed
/// by a session that has actually read it (a recorded `context.read`).
fn enforce_read_requirement(
    root: &Path,
    path: &Path,
    requested: &str,
    expected_hash: &ExpectedHash,
    event_kind: &str,
    event_symbol: Option<&str>,
) -> Result<()> {
    if !strict_mode()
        || expected_hash.value.is_some()
        || !path.exists()
        || !is_source_path(path)
    {
        return Ok(());
    }
    let repo_path = lock_path(root, path, requested);
    if let Ok(store) = AnchorStore::discover(root).or_else(|_| AnchorStore::init(root)) {
        events::record(
            store.anchor_root(),
            event_kind,
            Some(repo_path.clone()),
            event_symbol.map(str::to_string),
            "blocked",
            Some("strict mode: no recorded read for this session".to_string()),
        );
    }
    println!("<result>");
    println!("<path>{}</path>", repo_path);
    println!("<status>read_required</status>");
    println!("<message>strict mode: read this file through anchor context before editing it</message>");
    println!("</result>");
    anyhow::bail!("strict mode: no recorded read for {}", repo_path)
}

/// Provenance is load-bearing: the attempt is recorded *before* the file is
/// touched, so an unwritable event log refuses the mutation instead of
/// producing an unrecorded change.
fn record_write_attempt(
    root: &Path,
    path: &Path,
    requested: &str,
    operation: &str,
    symbol: Option<&str>,
) -> Result<()> {
    let store = AnchorStore::discover(root).or_else(|_| AnchorStore::init(root))?;
    events::record_required(
        store.anchor_root(),
        "write.attempt",
        Some(lock_path(root, path, requested)),
        symbol.map(str::to_string),
        "ok",
        Some(format!("operation={operation}")),
    )
}

fn verify_expected_hash(
    root: &Path,
    path: &Path,
    requested: &str,
    before_hash: Option<&str>,
    expected_hash: Option<&str>,
    event_kind: &str,
    event_symbol: Option<&str>,
) -> Result<()> {
    let Some(expected_hash) = expected_hash else {
        return Ok(());
    };
    let actual_hash = before_hash.unwrap_or("missing");
    if actual_hash == expected_hash {
        return Ok(());
    }

    if let Ok(store) = AnchorStore::discover(root).or_else(|_| AnchorStore::init(root)) {
        events::record(
            store.anchor_root(),
            event_kind,
            Some(lock_path(root, path, requested)),
            event_symbol.map(str::to_string),
            "blocked",
            Some(format!(
                "stale_file expected={} actual={}",
                expected_hash, actual_hash
            )),
        );
    }

    println!("<result>");
    println!("<path>{}</path>", lock_path(root, path, requested));
    println!("<status>stale_file</status>");
    println!("<expected_hash>{}</expected_hash>", expected_hash);
    println!("<actual_hash>{}</actual_hash>", actual_hash);
    println!("</result>");

    anyhow::bail!(
        "stale file: expected hash {}, actual {}",
        expected_hash,
        actual_hash
    )
}

fn write_event_meta(
    before_hash: Option<&str>,
    after_hash: Option<&str>,
    expected_hash: &ExpectedHash,
    content_hash: Option<String>,
    change: Option<&ChangeSummary>,
    lines: usize,
    bytes: usize,
    replacements: Option<usize>,
) -> std::collections::BTreeMap<String, String> {
    let mut meta = std::collections::BTreeMap::new();
    meta.insert(
        "before_hash".to_string(),
        before_hash.unwrap_or("missing").to_string(),
    );
    meta.insert(
        "after_hash".to_string(),
        after_hash.unwrap_or("missing").to_string(),
    );
    meta.insert(
        "expected_hash_source".to_string(),
        expected_hash.source.to_string(),
    );
    if let Some(expected_hash) = &expected_hash.value {
        meta.insert("expected_hash".to_string(), expected_hash.clone());
    }
    if let Some(content_hash) = content_hash {
        meta.insert("content_hash".to_string(), content_hash);
    }
    if let Some(change) = change {
        meta.insert(
            "changed_start_line".to_string(),
            change.start_line.to_string(),
        );
        meta.insert("old_end_line".to_string(), change.old_end_line.to_string());
        meta.insert("new_end_line".to_string(), change.new_end_line.to_string());
        meta.insert(
            "old_changed_lines".to_string(),
            change.old_changed_lines.to_string(),
        );
        meta.insert(
            "new_changed_lines".to_string(),
            change.new_changed_lines.to_string(),
        );
    }
    meta.insert("lines".to_string(), lines.to_string());
    meta.insert("bytes".to_string(), bytes.to_string());
    if let Some(replacements) = replacements {
        meta.insert("replacements".to_string(), replacements.to_string());
    }
    meta
}

fn print_compact_write_receipt(
    status: &str,
    path: &str,
    before_hash: Option<&str>,
    after_hash: Option<&str>,
    change: Option<&ChangeSummary>,
    lines: usize,
    bytes: usize,
    replacements: Option<usize>,
) {
    println!("<result>");
    println!("<path>{}</path>", path);
    println!("<status>{}</status>", status);
    if let Some(before_hash) = before_hash {
        println!("<before_hash>{}</before_hash>", before_hash);
    }
    if let Some(after_hash) = after_hash {
        println!("<after_hash>{}</after_hash>", after_hash);
    }
    if let Some(change) = change {
        println!(
            "<changed_range start=\"{}\" old_end=\"{}\" new_end=\"{}\" old_lines=\"{}\" new_lines=\"{}\"/>",
            change.start_line,
            change.old_end_line,
            change.new_end_line,
            change.old_changed_lines,
            change.new_changed_lines
        );
    }
    println!("<lines>{}</lines>", lines);
    println!("<bytes>{}</bytes>", bytes);
    if let Some(replacements) = replacements {
        println!("<replacements>{}</replacements>", replacements);
    }
    println!("</result>");
}

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
                        before_hash.as_deref(),
                        after_hash.as_deref(),
                        &expected_hash,
                        Some(content_hash_text(content)),
                        Some(&change),
                        result.lines_written,
                        result.bytes_written,
                        None,
                    ),
                );
            }
            print_compact_write_receipt(
                "created",
                &result.path,
                before_hash.as_deref(),
                after_hash.as_deref(),
                Some(&change),
                result.lines_written,
                result.bytes_written,
                None,
            );
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

/// Insert content after a pattern
pub fn insert(
    root: &Path,
    path: &str,
    pattern: &str,
    content: &str,
    expected_hash: Option<&str>,
) -> Result<()> {
    let full_path = resolve_path(root, path);
    let _lock = acquire_file_lock(root, &full_path, path)?;
    let before_hash = file_hash(&full_path);
    let before_text = file_text(&full_path);
    let expected_hash = expected_hash_from_recent_read(root, &full_path, path, expected_hash);
    verify_expected_hash(
        root,
        &full_path,
        path,
        before_hash.as_deref(),
        expected_hash.value.as_deref(),
        "edit.guard",
        None,
    )?;
    enforce_read_requirement(root, &full_path, path, &expected_hash, "edit.guard", None)?;
    record_write_attempt(root, &full_path, path, "insert", None)?;

    match crate::write::governed(|| {
        protect::with_unlocked_path(root, &full_path, || {
            insert_after(&full_path, pattern, content).map_err(anyhow::Error::from)
        })
    }) {
        Ok(result) => {
            reindex_after_write(root, &full_path)?;
            let after_hash = file_hash(&full_path);
            let after_text = file_text(&full_path).unwrap_or_default();
            let change = line_change_summary(before_text.as_deref(), &after_text);
            if let Ok(store) = AnchorStore::discover(root).or_else(|_| AnchorStore::init(root)) {
                events::record_with_meta(
                    store.anchor_root(),
                    "edit.apply",
                    Some(lock_path(root, &full_path, path)),
                    None,
                    "ok",
                    Some(format!(
                        "inserted before={} after={} content={}",
                        before_hash.as_deref().unwrap_or("missing"),
                        after_hash.as_deref().unwrap_or("missing"),
                        content_hash_text(content)
                    )),
                    write_event_meta(
                        before_hash.as_deref(),
                        after_hash.as_deref(),
                        &expected_hash,
                        Some(content_hash_text(content)),
                        Some(&change),
                        result.lines_written,
                        result.bytes_written,
                        None,
                    ),
                );
            }
            print_compact_write_receipt(
                "inserted",
                &result.path,
                before_hash.as_deref(),
                after_hash.as_deref(),
                Some(&change),
                result.lines_written,
                result.bytes_written,
                None,
            );
        }
        Err(e) => {
            if let Ok(store) = AnchorStore::discover(root).or_else(|_| AnchorStore::init(root)) {
                events::record(
                    store.anchor_root(),
                    "edit.apply",
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
    enforce_read_requirement(root, &full_path, path, &expected_hash, "edit.guard", Some(symbol))?;
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
            before_hash.as_deref(),
            after_hash.as_deref(),
            &expected_hash,
            Some(content_hash_text(content)),
            Some(&change),
            result.lines_written,
            result.bytes_written,
            None,
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
                            before_hash.as_deref(),
                            after_hash.as_deref(),
                            &expected_hash,
                            Some(content_hash_text(new)),
                            Some(&change),
                            result.lines_written,
                            result.bytes_written,
                            result.replacements,
                        ),
                    );
                }
                let count = result.replacements.unwrap_or(0);
                print_compact_write_receipt(
                    "replaced",
                    &result.path,
                    before_hash.as_deref(),
                    after_hash.as_deref(),
                    Some(&change),
                    result.lines_written,
                    result.bytes_written,
                    Some(count),
                );
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

/// Expand a glob pattern into a list of file paths
pub fn expand_glob(root: &Path, pattern: &str) -> Result<Vec<PathBuf>> {
    use std::fs;

    // If it's a simple path (no glob chars), just return it
    if !pattern.contains('*') && !pattern.contains('?') {
        let path = if Path::new(pattern).is_absolute() {
            PathBuf::from(pattern)
        } else {
            root.join(pattern)
        };
        return Ok(vec![path]);
    }

    let mut results = Vec::new();
    let glob_pattern = if Path::new(pattern).is_absolute() {
        pattern.to_string()
    } else {
        root.join(pattern).to_string_lossy().to_string()
    };

    let parts: Vec<&str> = glob_pattern.split("**").collect();

    fn walk_dir(dir: &Path, results: &mut Vec<PathBuf>, pattern: &str) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    walk_dir(&path, results, pattern);
                } else if matches_pattern(&path, pattern) {
                    results.push(path);
                }
            }
        }
    }

    fn matches_pattern(path: &Path, pattern: &str) -> bool {
        let path_str = path.to_string_lossy();

        // Simple wildcard matching
        if pattern.contains("**") {
            // Handle **/*.rs style patterns
            if let Some(ext) = pattern.strip_prefix("**/") {
                if ext.starts_with("*.") {
                    let ext = ext.strip_prefix("*.").unwrap();
                    return path.extension().map(|e| e == ext).unwrap_or(false);
                }
                return path_str.ends_with(ext);
            }
        }

        if pattern.contains('*') {
            // Handle *.rs style patterns
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 {
                let prefix = parts[0];
                let suffix = parts[1];
                return (prefix.is_empty() || path_str.starts_with(prefix))
                    && (suffix.is_empty() || path_str.ends_with(suffix));
            }
        }

        path_str.contains(pattern)
    }

    if parts.len() > 1 {
        // Has ** in pattern
        let base = if parts[0].is_empty() {
            root.to_path_buf()
        } else {
            PathBuf::from(parts[0].trim_end_matches('/'))
        };
        walk_dir(&base, &mut results, &glob_pattern);
    } else {
        // Simple glob
        let parent = Path::new(&glob_pattern).parent().unwrap_or(root);
        if let Ok(entries) = fs::read_dir(parent) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if matches_pattern(&path, &glob_pattern) {
                    results.push(path);
                }
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::{line_change_summary, lock_path, resolve_path, ChangeSummary};
    use std::path::Path;

    #[test]
    fn regression_cli_lock_path_is_repo_relative() {
        let root = Path::new("/repo");
        let path = Path::new("/repo/src/auth.py");

        assert_eq!(lock_path(root, path, "src/auth.py"), "src/auth.py");
    }

    #[test]
    fn regression_cli_resolves_relative_paths_under_root() {
        let root = Path::new("/repo");

        assert_eq!(
            resolve_path(root, "src/auth.py"),
            Path::new("/repo/src/auth.py")
        );
    }

    #[test]
    fn regression_symbol_lock_name_is_lockd_safe_and_stable() {
        let first = super::symbol_lock_name("src/auth.py", "Auth.login!");
        let second = super::symbol_lock_name("src/auth.py", "Auth.login!");

        assert_eq!(first, second);
        assert!(first.starts_with("sym:"));
        assert!(first
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == ':'));
    }

    #[test]
    fn regression_line_change_summary_detects_middle_replace() {
        assert_eq!(
            line_change_summary(Some("a\nb\nc\n"), "a\nB\nc\n"),
            ChangeSummary {
                start_line: 2,
                old_end_line: 2,
                new_end_line: 2,
                old_changed_lines: 1,
                new_changed_lines: 1,
            }
        );
    }

    #[test]
    fn regression_line_change_summary_detects_insert() {
        assert_eq!(
            line_change_summary(Some("a\nc\n"), "a\nb\nc\n"),
            ChangeSummary {
                start_line: 2,
                old_end_line: 1,
                new_end_line: 2,
                old_changed_lines: 0,
                new_changed_lines: 1,
            }
        );
    }
}
