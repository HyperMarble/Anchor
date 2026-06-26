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

