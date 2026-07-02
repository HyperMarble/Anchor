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
