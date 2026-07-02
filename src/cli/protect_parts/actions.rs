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

