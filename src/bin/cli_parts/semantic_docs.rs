fn materialize_semantic_workspace(
    store: &AnchorStore,
    packet: &TaskPacket,
    owner_limit: usize,
) -> Result<PathBuf> {
    let root = store.anchor_root().join("semantic").join("current");
    if root.exists() {
        std::fs::remove_dir_all(&root)?;
    }
    for dir in [
        "by-task/owners",
        "by-task/files",
        "by-task/tests",
        "by-symbol",
        "by-verification",
        "by-stale",
    ] {
        std::fs::create_dir_all(root.join(dir))?;
    }
    let owner_limit = owner_limit.clamp(3, 12);
    write_text(root.join("index.md"), &semantic_index(packet, owner_limit))?;
    write_text(
        root.join("by-verification").join("required.md"),
        &semantic_verification(packet),
    )?;
    write_text(
        root.join("by-stale").join("README.md"),
        "Writes must use the source_hash shown in owner/file documents.\n",
    )?;
    for (idx, chunk) in packet.owner_chunks.iter().take(owner_limit).enumerate() {
        let file = format!("{:02}_{}.md", idx + 1, safe_name(&chunk.symbol));
        let text = semantic_owner_doc(chunk);
        write_text(root.join("by-task").join("owners").join(&file), &text)?;
        let symbol_dir = root.join("by-symbol").join(safe_name(&chunk.symbol));
        std::fs::create_dir_all(&symbol_dir)?;
        write_text(symbol_dir.join("definition.md"), &text)?;
    }
    for (idx, file) in packet.likely_files.iter().enumerate() {
        let name = format!("{:02}_{}.md", idx + 1, safe_name(&file.path));
        write_text(
            root.join("by-task").join("files").join(name),
            &semantic_file_doc(file, packet),
        )?;
    }
    for (idx, test) in packet.likely_tests.iter().enumerate() {
        let name = format!("{:02}_{}.md", idx + 1, safe_name(&test.path));
        write_text(
            root.join("by-task").join("tests").join(name),
            &semantic_test_doc(test),
        )?;
    }
    append_semantic_contract_ledger(store.anchor_root(), packet, owner_limit)?;
    Ok(root)
}

fn semantic_index(packet: &TaskPacket, owner_limit: usize) -> String {
    let mut out = format!("# Anchor Semantic Workspace\n\nintent: {}\n\n", packet.intent);
    out.push_str("## Owner Chunks\n");
    for chunk in packet.owner_chunks.iter().take(owner_limit) {
        out.push_str(&format!(
            "- `{}` `{}` lines {}-{} hash `{}`\n",
            chunk.path, chunk.symbol, chunk.line_start, chunk.line_end, chunk.source_hash
        ));
    }
    out.push_str("\n## Likely Tests\n");
    for test in &packet.likely_tests {
        out.push_str(&format!("- `{}` score `{}`\n", test.path, test.score));
    }
    out.push_str("\n## Commands\n");
    out.push_str("- `cat .anchor/semantic/current/by-task/owners/*.md`\n");
    out.push_str("- `rg \"term\" .anchor/semantic/current`\n");
    out
}

fn semantic_owner_doc(chunk: &TaskSlice) -> String {
    let handle = chunk_handle(&chunk.path, &chunk.symbol, chunk.line_start, chunk.line_end);
    format!(
        "# Owner Chunk\n\nhandle: `{}`\npath: `{}`\nsymbol: `{}`\nkind: `{}`\nlines: {}-{}\nsource_hash: `{}`\nscore: `{}`\nreasons: `{}`\ntags: `{}`\nmeaning: {}\n\nread:\n`anchor read {}`\n\nedit_guard:\n`--expect-hash {}`\n\n```text\n{}```\n",
        handle,
        chunk.path,
        chunk.symbol,
        chunk.kind,
        chunk.line_start,
        chunk.line_end,
        chunk.source_hash,
        chunk.score,
        chunk.reasons.join(","),
        chunk.responsibility_tags.join(","),
        chunk.meaning,
        handle,
        chunk.source_hash,
        chunk.code
    )
}

fn semantic_file_doc(file: &TaskPath, packet: &TaskPacket) -> String {
    let mut out = format!(
        "# Likely File\n\nhandle: `file:{}`\npath: `{}`\nsource_hash: `{}`\nscore: `{}`\nrole: `{}`\nreasons: `{}`\n\nowner_chunks:\n",
        file.path,
        file.path,
        file.source_hash,
        file.score,
        file.role,
        file.reasons.join(",")
    );
    for chunk in packet.owner_chunks.iter().filter(|chunk| chunk.path == file.path) {
        out.push_str(&format!(
            "- `{}` lines {}-{}\n",
            chunk.symbol, chunk.line_start, chunk.line_end
        ));
    }
    out
}

fn semantic_test_doc(test: &TaskTest) -> String {
    format!(
        "# Likely Test\n\nhandle: `test:{}`\npath: `{}`\nscore: `{}`\nreasons: `{}`\n",
        test.path,
        test.path,
        test.score,
        test.reasons.join(",")
    )
}

fn semantic_verification(packet: &TaskPacket) -> String {
    let mut out = "# Verification Requirements\n\n".to_string();
    for step in &packet.verification_plan.steps {
        out.push_str(&format!("- {}\n", step));
    }
    if let Some(check) = &packet.verification_plan.preferred_check {
        out.push_str(&format!("\npreferred_check:\n`{}`\n", check));
    }
    for hint in &packet.verification_plan.check_hints {
        out.push_str(&format!("\n{}:\n`{}`\n", hint.kind, hint.command));
    }
    out
}

fn write_text(path: PathBuf, text: &str) -> Result<()> {
    std::fs::write(path, text)?;
    Ok(())
}

fn append_semantic_contract_ledger(
    anchor_root: &Path,
    packet: &TaskPacket,
    owner_limit: usize,
) -> Result<()> {
    let semantic_root = anchor_root.join("semantic");
    std::fs::create_dir_all(&semantic_root)?;
    let ledger = semantic_root.join("contracts.jsonl");
    let mut paths = std::collections::BTreeSet::new();
    for chunk in packet.owner_chunks.iter().take(owner_limit) {
        paths.insert(chunk.path.clone());
    }
    for file in &packet.likely_files {
        paths.insert(file.path.clone());
    }
    for test in &packet.likely_tests {
        paths.insert(test.path.clone());
    }
    let planned_new_paths = packet
        .likely_files
        .iter()
        .filter(|file| file.source_hash == "missing")
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    let record = serde_json::json!({
        "schema": "anchor.semantic_contract_ledger.v1",
        "intent": packet.intent,
        "planned_new_paths": planned_new_paths,
        "paths": paths.into_iter().collect::<Vec<_>>(),
    });
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(ledger)?;
    writeln!(file, "{}", serde_json::to_string(&record)?)?;
    Ok(())
}

fn safe_name(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').chars().take(80).collect()
}
