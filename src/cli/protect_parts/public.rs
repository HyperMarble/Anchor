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

    let update_result = if result.is_ok() {
        (|| -> Result<()> {
            let mut newly_protected = PermissionRestoreGuard::default();
            for path in paths {
                if !is_source_path(path) || !path.exists() {
                    continue;
                }
                let repo_path = repo_path(root, path);
                if state.entries.iter().all(|entry| entry.path != repo_path) {
                    let permissions = permission_snapshot(path)?;
                    state.entries.push(ProtectionEntry {
                        path: repo_path,
                        kind: ProtectionKind::File,
                        permissions: permissions.clone(),
                    });
                    if apply_readonly(path, ProtectionKind::File, &permissions)? {
                        newly_protected.push(path.to_path_buf(), permissions);
                    }
                }
            }
            save_state(root, &state)?;
            newly_protected.disarm();
            Ok(())
        })()
    } else {
        Ok(())
    };

    let restore_result = guard.try_restore();
    match (result, update_result, restore_result) {
        (Err(error), _, Err(restore_error)) => bail!(
            "protected write failed: {error}; restoring protection also failed: {restore_error}"
        ),
        (Err(error), _, Ok(())) => Err(error),
        (Ok(_), Err(error), Err(restore_error)) => bail!(
            "updating protection state failed: {error}; restoring protection also failed: \
             {restore_error}"
        ),
        (Ok(_), Err(error), Ok(())) => Err(error),
        (Ok(_), Ok(()), Err(error)) => Err(error),
        (Ok(value), Ok(()), Ok(())) => Ok(value),
    }
}

