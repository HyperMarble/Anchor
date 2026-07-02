fn cmd_task(
    root: &Path,
    intent_parts: &[String],
    limit: usize,
    context_limit: usize,
) -> Result<()> {
    let prepared = prepare_task_workspace(root, intent_parts, limit, context_limit)?;
    let semantic_path = materialize_semantic_workspace(
        &prepared.store,
        &prepared.packet,
        prepared.context_limit,
    )?;
    let likely_tests: Vec<(&String, usize)> = prepared
        .likely_tests_owned
        .iter()
        .map(|test| (&test.path, test.score))
        .collect();

    events::record(
        prepared.store.anchor_root(),
        "task.intake",
        None,
        None,
        "ok",
        Some(format!(
            "intent={} scoped_files={} symbols={} context_symbols={} related_files={} tests={} historical_files={} historical_tests={} semantic={}",
            prepared.intent,
            prepared.scoped_files,
            prepared.candidates.len(),
            context_limit.min(prepared.candidates.len()),
            prepared.related_files.len(),
            prepared.likely_tests_owned.len(),
            prepared.historical_files.len(),
            prepared.historical_tests.len(),
            semantic_path.display()
        )),
    );

    print_task_intake_output(TaskIntakeOutput {
        store: &prepared.store,
        symbol_index: &prepared.symbol_index,
        call_index: &prepared.call_index,
        history_index: &prepared.history_index,
        scoped_files: prepared.scoped_files,
        intent: &prepared.intent,
        candidates: &prepared.candidates,
        context_limit: prepared.context_limit,
        packet: &prepared.packet,
        related_files: &prepared.related_files,
        historical_files: &prepared.historical_files,
        likely_tests: &likely_tests,
        likely_test_count: prepared.likely_tests_owned.len(),
        historical_tests: &prepared.historical_tests,
    })?;

    Ok(())
}
