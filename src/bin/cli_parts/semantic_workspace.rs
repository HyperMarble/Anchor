fn cmd_semantic(
    root: &Path,
    intent_parts: &[String],
    limit: usize,
    context_limit: usize,
) -> Result<()> {
    let intent = intent_parts.join(" ");
    if let Some(path) = materialize_contract_semantic_workspace(root, &intent, context_limit)? {
        println!("anchor semantic workspace");
        println!("mode: contract");
        println!("path: {}", path.display());
        println!("start:");
        println!("  cat {}/index.md", path.display());
        println!("  ls {}/by-task/owners", path.display());
        println!("  rg \"<term>\" {}", path.display());
        return Ok(());
    }

    let prepared = prepare_task_workspace(root, intent_parts, limit, context_limit)?;
    let path =
        materialize_semantic_workspace(&prepared.store, &prepared.packet, prepared.context_limit)?;
    events::record(
        prepared.store.anchor_root(),
        "semantic.materialize",
        None,
        None,
        "ok",
        Some(format!(
            "intent={} owners={} tests={} path={}",
            prepared.intent,
            prepared.packet.owner_chunks.len(),
            prepared.packet.likely_tests.len(),
            path.display()
        )),
    );
    println!("anchor semantic workspace");
    println!("path: {}", path.display());
    println!("start:");
    println!("  ls {}/by-task", path.display());
    println!("  cat {}/index.md", path.display());
    println!("  rg \"<term>\" {}", path.display());
    Ok(())
}
