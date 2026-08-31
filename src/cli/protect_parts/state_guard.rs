fn build_state(root: &Path) -> Result<ProtectionState> {
    let mut files = BTreeMap::new();
    #[cfg(unix)]
    let mut dirs = BTreeMap::new();
    #[cfg(windows)]
    let dirs = BTreeMap::new();

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
        files.insert(relative, permission_snapshot(path)?);

        #[cfg(unix)]
        let mut parent = path.parent();
        #[cfg(unix)]
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
            dirs.insert(repo_path(root, dir), permission_snapshot(dir)?);
            parent = dir.parent();
        }
    }

    let mut entries = Vec::with_capacity(files.len() + dirs.len());
    entries.extend(dirs.into_iter().map(|(path, permissions)| ProtectionEntry {
        path,
        kind: ProtectionKind::Dir,
        permissions,
    }));
    entries.extend(files.into_iter().map(|(path, permissions)| ProtectionEntry {
        path,
        kind: ProtectionKind::File,
        permissions,
    }));

    Ok(ProtectionState {
        schema: PROTECTION_SCHEMA.to_string(),
        entries,
    })
}

struct UnlockGuard {
    permissions: PermissionRestoreGuard,
}

impl UnlockGuard {
    fn new(root: &Path, paths: &[PathBuf], state: &ProtectionState) -> Result<Self> {
        let wanted = paths
            .iter()
            .flat_map(|path| unlock_paths(root, path))
            .collect::<BTreeSet<_>>();
        let mut permissions = PermissionRestoreGuard::default();

        for entry in &state.entries {
            if !wanted.contains(&entry.path) {
                continue;
            }
            let full_path = root.join(&entry.path);
            if !full_path.exists() {
                continue;
            }
            let original = permission_snapshot(&full_path)?;
            if apply_writable(&full_path, entry.kind, &original)? {
                permissions.push(full_path, original);
            }
        }

        Ok(Self { permissions })
    }

    fn restore(&mut self) {
        self.permissions.restore();
    }

    fn try_restore(&mut self) -> Result<()> {
        self.permissions.try_restore()
    }
}

#[derive(Default)]
struct PermissionRestoreGuard {
    paths: Vec<(PathBuf, PermissionSnapshot)>,
}

impl PermissionRestoreGuard {
    fn push(&mut self, path: PathBuf, permissions: PermissionSnapshot) {
        self.paths.push((path, permissions));
    }

    fn restore(&mut self) {
        let _ = self.try_restore();
    }

    fn try_restore(&mut self) -> Result<()> {
        let mut first_error = None;
        for (path, permissions) in self.paths.drain(..).rev() {
            if let Err(error) = restore_permissions(&path, &permissions) {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn disarm(&mut self) {
        self.paths.clear();
    }
}

impl Drop for PermissionRestoreGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

impl Drop for UnlockGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

