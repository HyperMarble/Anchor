use anyhow::{bail, Result};
use ignore::Walk;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::events;
use crate::parser::language::is_source_path;
use crate::storage::AnchorStore;

const PROTECTION_SCHEMA: &str = "anchor.protection.v1";
const PROTECTION_FILE: &str = "protection.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ProtectionState {
    schema: String,
    entries: Vec<ProtectionEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ProtectionEntry {
    path: String,
    kind: ProtectionKind,
    mode: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ProtectionKind {
    File,
    Dir,
}

pub fn run(root: &Path, action: &str) -> Result<()> {
    match action {
        "on" => protect_on(root),
        "off" => protect_off(root),
        "status" => protect_status(root),
        other => bail!("unknown protect action: {other}; use on, off, or status"),
    }
}

pub fn is_active(root: &Path) -> bool {
    protection_path(root).is_file()
}

pub fn with_unlocked_path<T>(
    root: &Path,
    path: &Path,
    write_fn: impl FnOnce() -> Result<T>,
) -> Result<T> {
    with_unlocked_paths(root, &[path.to_path_buf()], write_fn)
}

pub fn with_unlocked_paths<T>(
    root: &Path,
    paths: &[PathBuf],
    write_fn: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let Some(mut state) = load_state(root)? else {
        return write_fn();
    };

    let mut guard = UnlockGuard::new(root, paths, &state)?;
    let result = write_fn();

    if result.is_ok() {
        for path in paths {
            if !is_source_path(path) || !path.exists() {
                continue;
            }
            let repo_path = repo_path(root, path);
            if state.entries.iter().all(|entry| entry.path != repo_path) {
                let mode = file_mode(path)?;
                state.entries.push(ProtectionEntry {
                    path: repo_path,
                    kind: ProtectionKind::File,
                    mode,
                });
                chmod(path, readonly_mode(mode))?;
            }
        }
        save_state(root, &state)?;
    }

    guard.restore();
    result
}

fn protect_on(root: &Path) -> Result<()> {
    let store = AnchorStore::init(root)?;
    if protection_path(root).is_file() {
        let Some(state) = load_state(root)? else {
            return Ok(());
        };
        print_status("active", &state);
        return Ok(());
    }

    let state = build_state(root)?;
    for entry in state
        .entries
        .iter()
        .filter(|entry| entry.kind == ProtectionKind::File)
    {
        chmod(root.join(&entry.path), readonly_mode(entry.mode))?;
    }
    for entry in state
        .entries
        .iter()
        .filter(|entry| entry.kind == ProtectionKind::Dir)
    {
        chmod(root.join(&entry.path), readonly_mode(entry.mode))?;
    }

    save_state(root, &state)?;
    record_event(store.anchor_root(), "protect.apply", "ok", &state);
    print_status("enabled", &state);
    Ok(())
}

fn protect_off(root: &Path) -> Result<()> {
    let store = AnchorStore::init(root)?;
    let Some(state) = load_state(root)? else {
        print_empty_status("inactive");
        return Ok(());
    };

    for entry in &state.entries {
        let path = root.join(&entry.path);
        if path.exists() {
            chmod(path, entry.mode)?;
        }
    }
    let _ = fs::remove_file(protection_path(root));
    record_event(store.anchor_root(), "protect.release", "ok", &state);
    print_status("disabled", &state);
    Ok(())
}

fn protect_status(root: &Path) -> Result<()> {
    let Some(state) = load_state(root)? else {
        print_empty_status("inactive");
        return Ok(());
    };
    print_status("active", &state);
    Ok(())
}

fn build_state(root: &Path) -> Result<ProtectionState> {
    let mut files = BTreeMap::new();
    let mut dirs = BTreeMap::new();

    for entry in Walk::new(root).filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        if path
            .components()
            .any(|component| component.as_os_str() == ".anchor")
        {
            continue;
        }
        if !is_source_path(path) {
            continue;
        }

        let relative = repo_path(root, path);
        files.insert(relative, file_mode(path)?);

        let mut parent = path.parent();
        while let Some(dir) = parent {
            if dir == root {
                break;
            }
            if dir
                .components()
                .any(|component| component.as_os_str() == ".anchor")
            {
                break;
            }
            dirs.insert(repo_path(root, dir), file_mode(dir)?);
            parent = dir.parent();
        }
    }

    let mut entries = Vec::with_capacity(files.len() + dirs.len());
    entries.extend(dirs.into_iter().map(|(path, mode)| ProtectionEntry {
        path,
        kind: ProtectionKind::Dir,
        mode,
    }));
    entries.extend(files.into_iter().map(|(path, mode)| ProtectionEntry {
        path,
        kind: ProtectionKind::File,
        mode,
    }));

    Ok(ProtectionState {
        schema: PROTECTION_SCHEMA.to_string(),
        entries,
    })
}

struct UnlockGuard {
    paths: Vec<(PathBuf, u32)>,
}

impl UnlockGuard {
    fn new(root: &Path, paths: &[PathBuf], state: &ProtectionState) -> Result<Self> {
        let wanted = paths
            .iter()
            .flat_map(|path| unlock_paths(root, path))
            .collect::<BTreeSet<_>>();
        let mut paths = Vec::new();

        for entry in &state.entries {
            if !wanted.contains(&entry.path) {
                continue;
            }
            let full_path = root.join(&entry.path);
            if !full_path.exists() {
                continue;
            }
            let mode = file_mode(&full_path)?;
            chmod(&full_path, writable_mode(mode))?;
            paths.push((full_path, mode));
        }

        Ok(Self { paths })
    }

    fn restore(&mut self) {
        for (path, mode) in self.paths.drain(..).rev() {
            let _ = chmod(path, mode);
        }
    }
}

impl Drop for UnlockGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

fn unlock_paths(root: &Path, path: &Path) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    if path.exists() {
        paths.insert(repo_path(root, path));
    }

    let mut parent = path.parent();
    while let Some(dir) = parent {
        if dir == root {
            break;
        }
        paths.insert(repo_path(root, dir));
        parent = dir.parent();
    }
    paths
}

fn load_state(root: &Path) -> Result<Option<ProtectionState>> {
    let path = protection_path(root);
    if !path.is_file() {
        return Ok(None);
    }
    let state: ProtectionState = serde_json::from_slice(&fs::read(path)?)?;
    Ok(Some(state))
}

fn save_state(root: &Path, state: &ProtectionState) -> Result<()> {
    let path = protection_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(state)?)?;
    Ok(())
}

fn protection_path(root: &Path) -> PathBuf {
    root.join(".anchor").join(PROTECTION_FILE)
}

fn repo_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().replace('\\', "/"))
}

fn file_mode(path: impl AsRef<Path>) -> Result<u32> {
    Ok(fs::metadata(path)?.permissions().mode() & 0o7777)
}

fn readonly_mode(mode: u32) -> u32 {
    mode & !0o222
}

fn writable_mode(mode: u32) -> u32 {
    mode | 0o200
}

fn chmod(path: impl AsRef<Path>, mode: u32) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}

fn record_event(anchor_root: &Path, event: &str, status: &str, state: &ProtectionState) {
    let files = state
        .entries
        .iter()
        .filter(|entry| entry.kind == ProtectionKind::File)
        .count();
    let dirs = state
        .entries
        .iter()
        .filter(|entry| entry.kind == ProtectionKind::Dir)
        .count();
    events::record(
        anchor_root,
        event,
        None,
        None,
        status,
        Some(format!("files={files} dirs={dirs}")),
    );
}

fn print_empty_status(status: &str) {
    println!("<protect>");
    println!("<status>{status}</status>");
    println!("<files>0</files>");
    println!("<dirs>0</dirs>");
    println!("</protect>");
}

fn print_status(status: &str, state: &ProtectionState) {
    let files = state
        .entries
        .iter()
        .filter(|entry| entry.kind == ProtectionKind::File)
        .count();
    let dirs = state
        .entries
        .iter()
        .filter(|entry| entry.kind == ProtectionKind::Dir)
        .count();

    println!("<protect>");
    println!("<status>{status}</status>");
    println!("<files>{files}</files>");
    println!("<dirs>{dirs}</dirs>");
    println!("</protect>");
}
