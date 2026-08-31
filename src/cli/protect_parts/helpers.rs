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
    let bytes = fs::read(path)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    let schema = value
        .get("schema")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let state = match schema {
        PROTECTION_SCHEMA => serde_json::from_value(value)?,
        LEGACY_PROTECTION_SCHEMA => {
            let legacy: LegacyProtectionState = serde_json::from_value(value)?;
            let migrated = migrate_legacy_state(legacy)?;
            save_state(root, &migrated)?;
            migrated
        }
        _ => bail!("unsupported protection state schema: {schema}"),
    };
    validate_state_platform(&state)?;
    Ok(Some(state))
}

#[cfg(unix)]
fn validate_state_platform(state: &ProtectionState) -> Result<()> {
    if state
        .entries
        .iter()
        .any(|entry| !matches!(&entry.permissions, PermissionSnapshot::Unix { .. }))
    {
        bail!("protection state contains Windows permissions and cannot be restored on Unix");
    }
    Ok(())
}

#[cfg(windows)]
fn validate_state_platform(state: &ProtectionState) -> Result<()> {
    if state
        .entries
        .iter()
        .any(|entry| !matches!(&entry.permissions, PermissionSnapshot::Windows { .. }))
    {
        bail!("protection state contains Unix permissions and cannot be restored on Windows");
    }
    Ok(())
}

#[cfg(unix)]
fn migrate_legacy_state(legacy: LegacyProtectionState) -> Result<ProtectionState> {
    debug_assert_eq!(legacy.schema, LEGACY_PROTECTION_SCHEMA);
    Ok(ProtectionState {
        schema: PROTECTION_SCHEMA.to_string(),
        entries: legacy
            .entries
            .into_iter()
            .map(|entry| ProtectionEntry {
                path: entry.path,
                kind: entry.kind,
                permissions: PermissionSnapshot::Unix { mode: entry.mode },
            })
            .collect(),
    })
}

#[cfg(windows)]
fn migrate_legacy_state(legacy: LegacyProtectionState) -> Result<ProtectionState> {
    debug_assert_eq!(legacy.schema, LEGACY_PROTECTION_SCHEMA);
    bail!(
        "protection state uses Unix permission modes and cannot be restored safely on Windows; \
         run `anchor protect off` on Unix before moving the repository, or remove \
         .anchor/protection.json only after verifying the source files are writable"
    )
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

#[cfg(unix)]
fn permission_snapshot(path: impl AsRef<Path>) -> Result<PermissionSnapshot> {
    let mode = fs::metadata(path)?.permissions().mode() & 0o7777;
    Ok(PermissionSnapshot::Unix { mode })
}

#[cfg(windows)]
fn permission_snapshot(path: impl AsRef<Path>) -> Result<PermissionSnapshot> {
    let readonly = fs::metadata(path)?.permissions().readonly();
    Ok(PermissionSnapshot::Windows { readonly })
}

#[cfg(unix)]
fn restore_permissions(path: impl AsRef<Path>, snapshot: &PermissionSnapshot) -> Result<()> {
    let PermissionSnapshot::Unix { mode } = snapshot else {
        bail!("cannot restore Windows permission state on Unix");
    };
    fs::set_permissions(path, fs::Permissions::from_mode(*mode))?;
    Ok(())
}

#[cfg(windows)]
fn restore_permissions(path: impl AsRef<Path>, snapshot: &PermissionSnapshot) -> Result<()> {
    let PermissionSnapshot::Windows { readonly } = snapshot else {
        bail!("cannot restore Unix permission state on Windows");
    };
    let path = path.as_ref();
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_readonly(*readonly);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(unix)]
fn apply_readonly(
    path: impl AsRef<Path>,
    _kind: ProtectionKind,
    snapshot: &PermissionSnapshot,
) -> Result<bool> {
    let PermissionSnapshot::Unix { mode } = snapshot else {
        bail!("cannot apply Windows permission state on Unix");
    };
    fs::set_permissions(path, fs::Permissions::from_mode(mode & !0o222))?;
    Ok(true)
}

#[cfg(windows)]
fn apply_readonly(
    path: impl AsRef<Path>,
    kind: ProtectionKind,
    _snapshot: &PermissionSnapshot,
) -> Result<bool> {
    // Windows ignores the read-only attribute on directories, so only claim and
    // apply preventive protection for files. New files remain covered by run/gate.
    if kind == ProtectionKind::Dir {
        return Ok(false);
    }
    let path = path.as_ref();
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions)?;
    Ok(true)
}

#[cfg(unix)]
fn apply_writable(
    path: impl AsRef<Path>,
    _kind: ProtectionKind,
    snapshot: &PermissionSnapshot,
) -> Result<bool> {
    let PermissionSnapshot::Unix { mode } = snapshot else {
        bail!("cannot apply Windows permission state on Unix");
    };
    fs::set_permissions(path, fs::Permissions::from_mode(mode | 0o200))?;
    Ok(true)
}

#[cfg(windows)]
#[allow(clippy::permissions_set_readonly_false)]
fn apply_writable(
    path: impl AsRef<Path>,
    kind: ProtectionKind,
    _snapshot: &PermissionSnapshot,
) -> Result<bool> {
    if kind == ProtectionKind::Dir {
        return Ok(false);
    }
    let path = path.as_ref();
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_readonly(false);
    fs::set_permissions(path, permissions)?;
    Ok(true)
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
