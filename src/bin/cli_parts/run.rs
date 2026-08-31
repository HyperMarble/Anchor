fn cmd_run(root: &Path, command: &[String]) -> Result<()> {
    if command.is_empty() {
        bail!("run requires a command");
    }

    if execroot_mode() {
        return cmd_run_execroot(root, command);
    }

    let store = open_store(root)?;
    let before = git_changed_paths(root)?
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    let command_text = command.join(" ");

    let mut cmd = std::process::Command::new(&command[0]);
    cmd.args(&command[1..]).current_dir(root);
    if cli_protect::is_active(root) {
        cmd.env("PYTHONDONTWRITEBYTECODE", "1");
    }
    let output = cmd.output()?;
    let code = output.status.code().unwrap_or(-1);
    let status = if output.status.success() {
        "ok"
    } else {
        "error"
    };

    events::record(
        store.anchor_root(),
        "terminal.run",
        None,
        None,
        status,
        Some(format!("exit={code} cmd={command_text}")),
    );

    let after = git_changed_paths(root)?
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    let newly_changed: Vec<String> = after.difference(&before).cloned().collect();
    let events_after = events::load(store.anchor_root())?;
    let summary_after = events::EventSummary::from_events(&events_after);
    let recorded_writes = summary_after
        .recorded_write_paths
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let raw_changed: Vec<String> = newly_changed
        .into_iter()
        .filter(|path| !recorded_writes.contains(path))
        .collect();

    for path in &raw_changed {
        events::record(
            store.anchor_root(),
            "terminal.raw_write",
            Some(path.clone()),
            None,
            "error",
            Some(format!("cmd={command_text}")),
        );
    }

    println!("<run>");
    println!("<command>{command_text}</command>");
    println!("<status>{status}</status>");
    println!("<exit_code>{code}</exit_code>");
    println!(
        "<raw_changed_files>{}</raw_changed_files>",
        raw_changed.len()
    );
    for path in &raw_changed {
        println!("  <raw_changed_file>{path}</raw_changed_file>");
    }
    println!("<stdout><![CDATA[");
    print!("{}", String::from_utf8_lossy(&output.stdout));
    println!("]]></stdout>");
    println!("<stderr><![CDATA[");
    print!("{}", String::from_utf8_lossy(&output.stderr));
    println!("]]></stderr>");
    println!("</run>");

    if !output.status.success() {
        bail!("run command failed with exit code {code}");
    }
    if !raw_changed.is_empty() {
        bail!("run command changed files outside Anchor writes");
    }
    Ok(())
}

fn execroot_mode() -> bool {
    std::env::var("ANCHOR_EXECROOT")
        .map(|value| matches!(value.as_str(), "1" | "true" | "on"))
        .unwrap_or(false)
}

