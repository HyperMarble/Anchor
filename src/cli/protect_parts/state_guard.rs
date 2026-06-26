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

