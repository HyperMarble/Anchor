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
    // Persist the originals before changing anything. If the process is interrupted,
    // `protect off` still has enough information to restore every touched path.
    save_state(root, &state)?;
    let mut guard = PermissionRestoreGuard::default();
    let apply_result = (|| -> Result<()> {
        for entry in &state.entries {
            let path = root.join(&entry.path);
            let current = permission_snapshot(&path)?;
            if apply_readonly(&path, entry.kind, &entry.permissions)? {
                guard.push(path, current);
            }
        }
        Ok(())
    })();
    if let Err(error) = apply_result {
        if let Err(rollback_error) = guard.try_restore() {
            bail!(
                "failed to enable protection: {error}; rollback also failed: {rollback_error}; \
                 protection state was retained for `anchor protect off`"
            );
        }
        if let Err(remove_error) = fs::remove_file(protection_path(root)) {
            bail!(
                "failed to enable protection: {error}; permissions were restored, but the \
                 protection state could not be removed: {remove_error}"
            );
        }
        return Err(error);
    }
    guard.disarm();

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
            restore_permissions(&path, &entry.permissions)?;
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

